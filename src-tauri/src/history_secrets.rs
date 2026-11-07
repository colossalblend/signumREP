                                                                         
   
                                                    
                                                                           

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::{fs, path::PathBuf};

const KEYRING_SERVICE: &str = "Signum";
const KEYRING_MASTER: &str = "history_secrets_master_v1";
const FILE_MAGIC: &[u8] = b"SGH1";
const NONCE_LEN: usize = 12;

                                                                                   
fn memory_passwords() -> &'static Mutex<HashMap<String, String>> {
    static MEMORY_PASSWORDS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    MEMORY_PASSWORDS.get_or_init(|| Mutex::new(HashMap::new()))
}

                                                                                   
fn memory_sessions() -> &'static Mutex<HashMap<String, HashMap<String, String>>> {
    static MEMORY_SESSIONS: OnceLock<Mutex<HashMap<String, HashMap<String, String>>>> =
        OnceLock::new();
    MEMORY_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

                                                                                    
fn memory_region_sessions(
) -> &'static Mutex<HashMap<String, crate::apple_history::AccountAuthSession>> {
    static MEMORY_REGION_SESSIONS: OnceLock<
        Mutex<HashMap<String, crate::apple_history::AccountAuthSession>>,
    > = OnceLock::new();
    MEMORY_REGION_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn email_key(email: &str) -> String {
    let hash = Sha256::digest(normalize_email(email).as_bytes());
    hex::encode(hash)
}

fn keyring_password_entry(email: &str) -> Result<keyring::Entry, String> {
                                                                    
    let name = format!(
        "hist_pw_{}",
        &email_key(email)[..32.min(email_key(email).len())]
    );
    keyring::Entry::new(KEYRING_SERVICE, &name).map_err(|e| e.to_string())
}

fn secrets_dir() -> Result<PathBuf, String> {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "Не удалось определить папку данных пользователя.".to_string())?;
    let dir = PathBuf::from(base).join("Signum").join("history-secrets");
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать хранилище: {e}"))?;
    Ok(dir)
}

fn secret_path(email: &str) -> Result<PathBuf, String> {
    Ok(secrets_dir()?.join(format!("{}.bin", email_key(email))))
}

fn master_key_file_path() -> Result<PathBuf, String> {
    Ok(secrets_dir()?.join("master.key"))
}

fn read_key32_from_b64(s: &str) -> Option<[u8; 32]> {
    let raw = B64.decode(s.trim()).ok()?;
    if raw.len() != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&raw);
    Some(key)
}

fn persist_master_key(key: &[u8; 32]) {
    let encoded = B64.encode(key);
                                                              
    if let Ok(path) = master_key_file_path() {
        let _ = fs::write(path, encoded.as_bytes());
    }
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_MASTER) {
        let _ = entry.set_password(&encoded);
    }
}

                                      
                                                                        
fn master_key_bytes() -> Result<[u8; 32], String> {
                 
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_MASTER) {
        if let Ok(s) = entry.get_password() {
            if let Some(key) = read_key32_from_b64(&s) {
                                                                              
                if let Ok(path) = master_key_file_path() {
                    if !path.is_file() {
                        let _ = fs::write(path, B64.encode(key).as_bytes());
                    }
                }
                return Ok(key);
            }
        }
    }
                                                             
    if let Ok(path) = master_key_file_path() {
        if let Ok(s) = fs::read_to_string(path) {
            if let Some(key) = read_key32_from_b64(&s) {
                                                        
                if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_MASTER) {
                    let _ = entry.set_password(&B64.encode(key));
                }
                return Ok(key);
            }
        }
    }
                   
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    persist_master_key(&key);
    Ok(key)
}

fn store_file_encrypted(email: &str, password: &str) -> Result<(), String> {
    let key = master_key_bytes()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, password.as_bytes())
        .map_err(|_| "Не удалось зашифровать пароль.".to_string())?;

    let mut blob = Vec::with_capacity(FILE_MAGIC.len() + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(FILE_MAGIC);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);

    let path = secret_path(email)?;
    fs::write(&path, &blob).map_err(|e| format!("Не удалось сохранить пароль: {e}"))?;
                                                                                    
    if load_file_encrypted(email).as_deref() != Some(password) {
        return Err("Не удалось надёжно сохранить пароль. Повторите вход.".into());
    }
    Ok(())
}

fn load_file_encrypted(email: &str) -> Option<String> {
    let path = secret_path(email).ok()?;
    if !path.is_file() {
        return None;
    }
    let blob = fs::read(&path).ok()?;
    if blob.len() < FILE_MAGIC.len() + NONCE_LEN + 16 {
        return None;
    }
    if &blob[..FILE_MAGIC.len()] != FILE_MAGIC {
        return None;
    }
    let nonce_start = FILE_MAGIC.len();
    let nonce_end = nonce_start + NONCE_LEN;
    let nonce = Nonce::from_slice(&blob[nonce_start..nonce_end]);
    let ciphertext = &blob[nonce_end..];
    let key = master_key_bytes().ok()?;
    let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
    let plain = cipher.decrypt(nonce, ciphertext).ok()?;
    String::from_utf8(plain).ok().filter(|s| !s.is_empty())
}

                                                  
                                                                              
pub fn store_history_password(email: &str, password: &str) -> Result<(), String> {
    let email = normalize_email(email);
    let password = password.trim();
    if email.is_empty() || password.is_empty() {
        return Ok(());
    }

                                           
    if let Ok(mut map) = memory_passwords().lock() {
        map.insert(email.clone(), password.to_string());
    }

    let mut errors = Vec::new();

                                                                                 
    if let Err(e) = store_file_encrypted(&email, password) {
        errors.push(format!("file: {e}"));
    }

                                                
    match keyring_password_entry(&email) {
        Ok(entry) => {
            if let Err(e) = entry.set_password(password) {
                errors.push(format!("keyring: {e}"));
            }
        }
        Err(e) => errors.push(format!("keyring entry: {e}")),
    }

                                                                          
    if load_history_password(&email).is_some() {
        return Ok(());
    }
    Err(if errors.is_empty() {
        "Не удалось сохранить пароль.".into()
    } else {
        format!("Не удалось сохранить пароль ({})", errors.join("; "))
    })
}

                                                     
pub fn load_history_password(email: &str) -> Option<String> {
    let email = normalize_email(email);
    if email.is_empty() {
        return None;
    }
    if let Ok(map) = memory_passwords().lock() {
        if let Some(p) = map.get(&email) {
            let p = p.trim();
            if !p.is_empty() {
                return Some(p.to_string());
            }
        }
    }
                                                                           
    if let Some(p) = load_file_encrypted(&email) {
        if let Ok(mut map) = memory_passwords().lock() {
            map.insert(email.clone(), p.clone());
        }
                                             
        if let Ok(entry) = keyring_password_entry(&email) {
            let _ = entry.set_password(&p);
        }
        return Some(p);
    }
    if let Ok(entry) = keyring_password_entry(&email) {
        if let Ok(p) = entry.get_password() {
            let p = p.trim().to_string();
            if !p.is_empty() {
                if let Ok(mut map) = memory_passwords().lock() {
                    map.insert(email.clone(), p.clone());
                }
                                                                            
                let _ = store_file_encrypted(&email, &p);
                return Some(p);
            }
        }
    }
    None
}

pub fn has_history_password(email: &str) -> bool {
    load_history_password(email).is_some()
}

pub fn delete_history_password(email: &str) {
    let email = normalize_email(email);
    if let Ok(mut map) = memory_passwords().lock() {
        map.remove(&email);
    }
    if let Ok(entry) = keyring_password_entry(&email) {
        let _ = entry.delete_credential();
    }
    if let Ok(path) = secret_path(&email) {
        let _ = fs::remove_file(path);
    }
}

                                                                          
pub fn resolve_history_password(email: &str, provided: &str) -> Result<String, String> {
    let provided = provided.trim();
    if !provided.is_empty() {
        let _ = store_history_password(email, provided);
        return Ok(provided.to_string());
    }
    load_history_password(email)
        .ok_or_else(|| "Пароль не найден. Войдите в Apple ID на странице «Аккаунт» ещё раз.".into())
}

                                                                              
                                                                             
                                                                              

fn keyring_session_entry(email: &str) -> Result<keyring::Entry, String> {
    let name = format!(
        "hist_sess_{}",
        &email_key(email)[..32.min(email_key(email).len())]
    );
    keyring::Entry::new(KEYRING_SERVICE, &name).map_err(|e| e.to_string())
}

                                                                                           
pub fn merge_cookie_maps(
    base: &HashMap<String, String>,
    patch: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut out = base.clone();
    for (k, v) in patch {
        if !k.is_empty() && !v.is_empty() {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

fn session_json_path(email: &str) -> Result<PathBuf, String> {
    Ok(secrets_dir()?.join(format!("{}.session.json", email_key(email))))
}

                                                                    
                                                             
   
                                                                                      
                                                                                      
pub fn store_history_session(email: &str, cookies: &HashMap<String, String>) -> Result<(), String> {
    let email = normalize_email(email);
    if email.is_empty() || cookies.is_empty() {
        return Ok(());
    }
                                                                         
    let prev = {
        if let Ok(map) = memory_sessions().lock() {
            if let Some(c) = map.get(&email) {
                if !c.is_empty() {
                    c.clone()
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        }
    };
    let prev = if prev.is_empty() {
        load_history_session_from_file(&email).unwrap_or_default()
    } else {
        prev
    };
    let merged = merge_cookie_maps(&prev, cookies);
    if merged.is_empty() {
        return Ok(());
    }

    if let Ok(mut map) = memory_sessions().lock() {
        map.insert(email.clone(), merged.clone());
    }

    let json = serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())?;
    let path = session_json_path(&email)?;
    fs::write(&path, json.as_bytes())
        .map_err(|e| format!("Не удалось сохранить сессию истории: {e}"))?;
                                                                                     
    let _ = store_file_blob(&email, ".session.bin", json.as_bytes());
    Ok(())
}

fn load_history_session_from_file(email: &str) -> Option<HashMap<String, String>> {
                                              
    if let Ok(path) = session_json_path(email) {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(raw.trim()) {
                if !map.is_empty() {
                    return Some(map);
                }
            }
        }
    }
                     
    if let Some(bytes) = load_file_blob(email, ".session.bin") {
        if let Ok(map) = serde_json::from_slice::<HashMap<String, String>>(&bytes) {
            if !map.is_empty() {
                return Some(map);
            }
        }
    }
    None
}

fn store_file_blob(email: &str, suffix: &str, plain: &[u8]) -> Result<(), String> {
    let key = master_key_bytes()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plain)
        .map_err(|_| "Не удалось зашифровать сессию.".to_string())?;
    let mut blob = Vec::with_capacity(FILE_MAGIC.len() + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(FILE_MAGIC);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    let path = secrets_dir()?.join(format!("{}{}", email_key(email), suffix));
    fs::write(&path, &blob).map_err(|e| format!("Не удалось сохранить сессию: {e}"))?;
    Ok(())
}

fn load_file_blob(email: &str, suffix: &str) -> Option<Vec<u8>> {
    let path = secrets_dir()
        .ok()?
        .join(format!("{}{}", email_key(email), suffix));
    if !path.is_file() {
        return None;
    }
    let blob = fs::read(&path).ok()?;
    if blob.len() < FILE_MAGIC.len() + NONCE_LEN + 16 {
        return None;
    }
    if &blob[..FILE_MAGIC.len()] != FILE_MAGIC {
        return None;
    }
    let nonce_start = FILE_MAGIC.len();
    let nonce_end = nonce_start + NONCE_LEN;
    let nonce = Nonce::from_slice(&blob[nonce_start..nonce_end]);
    let ciphertext = &blob[nonce_end..];
    let key = master_key_bytes().ok()?;
    let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
    cipher.decrypt(nonce, ciphertext).ok()
}

                                                      
pub fn load_history_session(email: &str) -> Option<HashMap<String, String>> {
    let email = normalize_email(email);
    if email.is_empty() {
        return None;
    }
                
    if let Ok(map) = memory_sessions().lock() {
        if let Some(c) = map.get(&email) {
            if !c.is_empty() {
                return Some(c.clone());
            }
        }
    }
                                                                                    
    if let Some(map) = load_history_session_from_file(&email) {
        if let Ok(mut mem) = memory_sessions().lock() {
            mem.insert(email.clone(), map.clone());
        }
        return Some(map);
    }
                                                                      
    if let Ok(entry) = keyring_session_entry(&email) {
        if let Ok(raw) = entry.get_password() {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(raw.trim()) {
                if map.contains_key("myacinfo") && map.len() >= 4 {
                    if let Ok(mut mem) = memory_sessions().lock() {
                        mem.insert(email.clone(), map.clone());
                    }
                    return Some(map);
                }
            }
        }
    }
    None
}

pub fn has_history_session(email: &str) -> bool {
    load_history_session(email).is_some()
}

                                                                     
pub fn delete_history_session_cookies_only(email: &str) {
    let email = normalize_email(email);
    if let Ok(mut map) = memory_sessions().lock() {
        map.remove(&email);
    }
    if let Ok(entry) = keyring_session_entry(&email) {
        let _ = entry.delete_credential();
    }
    if let Ok(path) = session_json_path(&email) {
        let _ = fs::remove_file(path);
    }
    if let Ok(path) = secrets_dir() {
        let _ = fs::remove_file(path.join(format!("{}.session.bin", email_key(&email))));
    }
}

                                                                     
                                                                       
                                                                     

const REGION_SESSION_SUFFIX: &str = ".region-session.bin";

pub fn store_region_session(
    email: &str,
    session: &crate::apple_history::AccountAuthSession,
) -> Result<(), String> {
    let email = normalize_email(email);
    if email.is_empty() || session.cookies.is_empty() {
        return Ok(());
    }
    if let Ok(mut sessions) = memory_region_sessions().lock() {
        sessions.insert(email.clone(), session.clone());
    }
    let bytes = serde_json::to_vec(session).map_err(|e| e.to_string())?;
    store_file_blob(&email, REGION_SESSION_SUFFIX, &bytes)
}

pub fn load_region_session(email: &str) -> Option<crate::apple_history::AccountAuthSession> {
    let email = normalize_email(email);
    if email.is_empty() {
        return None;
    }
    if let Ok(sessions) = memory_region_sessions().lock() {
        if let Some(session) = sessions.get(&email) {
            return Some(session.clone());
        }
    }
    let bytes = load_file_blob(&email, REGION_SESSION_SUFFIX)?;
    let session =
        serde_json::from_slice::<crate::apple_history::AccountAuthSession>(&bytes).ok()?;
    if session.cookies.is_empty() {
        return None;
    }
    if let Ok(mut sessions) = memory_region_sessions().lock() {
        sessions.insert(email, session.clone());
    }
    Some(session)
}

pub fn delete_region_session(email: &str) {
    let email = normalize_email(email);
    if let Ok(mut sessions) = memory_region_sessions().lock() {
        sessions.remove(&email);
    }
    if let Ok(path) = secrets_dir() {
        let _ =
            fs::remove_file(path.join(format!("{}{}", email_key(&email), REGION_SESSION_SUFFIX)));
    }
}

                                                                 
pub fn delete_history_session(email: &str) {
    delete_history_session_cookies_only(email);
    delete_history_apps_cache(&email);
}

                                                          
pub fn delete_all_history_secrets(email: &str) {
    delete_history_password(email);
    delete_history_session(email);
}

                                                                              
                                                                  
                                                                              

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedHistoryApp {
    pub adam_id: String,
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub price: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub bundle_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryAppsCache {
    pub email: String,
    pub apps: Vec<CachedHistoryApp>,
    #[serde(default)]
    pub next_batch_id: Option<String>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub updated_unix: u64,
                                                                          
    #[serde(default)]
    pub rap_dsid: Option<String>,
                                                                                     
    #[serde(default)]
    pub rap_xsrf: Option<String>,
}

fn history_cache_path(email: &str) -> Result<PathBuf, String> {
    let dir = secrets_dir()?.join("apps-cache");
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать кэш истории: {e}"))?;
    Ok(dir.join(format!("{}.json", email_key(email))))
}

pub fn load_history_apps_cache(email: &str) -> Option<HistoryAppsCache> {
    let email = normalize_email(email);
    let path = history_cache_path(&email).ok()?;
    let raw = fs::read_to_string(path).ok()?;
    let mut cache: HistoryAppsCache = serde_json::from_str(&raw).ok()?;
    cache.email = email;
    if cache.apps.is_empty() && !cache.has_more {
        return None;
    }
    Some(cache)
}

pub fn store_history_apps_cache(cache: &HistoryAppsCache) -> Result<(), String> {
    let email = normalize_email(&cache.email);
    if email.is_empty() {
        return Err("empty email".into());
    }
    let path = history_cache_path(&email)?;
    let mut to_save = cache.clone();
    to_save.email = email;
                                                 
    if to_save
        .next_batch_id
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        to_save.next_batch_id = None;
        to_save.has_more = false;
    }
    let json = serde_json::to_string_pretty(&to_save).map_err(|e| e.to_string())?;
    fs::write(&path, json.as_bytes())
        .map_err(|e| format!("Не удалось сохранить кэш истории ({}): {e}", path.display()))
}

                                                           
pub fn merge_history_apps_cache(
    email: &str,
    new_apps: &[CachedHistoryApp],
    next_batch_id: Option<String>,
    has_more: bool,
    replace: bool,
) -> HistoryAppsCache {
    merge_history_apps_cache_with_rap(
        email,
        new_apps,
        next_batch_id,
        has_more,
        replace,
        None,
        None,
    )
}

                                                                                                      
pub fn merge_history_apps_cache_with_rap(
    email: &str,
    new_apps: &[CachedHistoryApp],
    next_batch_id: Option<String>,
    has_more: bool,
    replace: bool,
    rap_dsid: Option<String>,
    rap_xsrf: Option<String>,
) -> HistoryAppsCache {
    let email = normalize_email(email);
    let mut cache = if replace {
        HistoryAppsCache {
            email: email.clone(),
            apps: Vec::new(),
            next_batch_id: None,
            has_more: false,
            updated_unix: 0,
            rap_dsid: None,
            rap_xsrf: None,
        }
    } else {
        load_history_apps_cache(&email).unwrap_or(HistoryAppsCache {
            email: email.clone(),
            apps: Vec::new(),
            next_batch_id: None,
            has_more: false,
            updated_unix: 0,
            rap_dsid: None,
            rap_xsrf: None,
        })
    };
    let mut seen: std::collections::HashSet<String> =
        cache.apps.iter().map(|a| a.adam_id.clone()).collect();
    for a in new_apps {
        if seen.insert(a.adam_id.clone()) {
            cache.apps.push(a.clone());
        } else if !a.bundle_id.trim().is_empty() {
                                                                         
            if let Some(existing) = cache.apps.iter_mut().find(|x| x.adam_id == a.adam_id) {
                if existing.bundle_id.trim().is_empty()
                    || existing.bundle_id.starts_with("app.store.")
                {
                    existing.bundle_id = a.bundle_id.clone();
                }
            }
        }
    }
    cache.next_batch_id = next_batch_id.filter(|s| !s.trim().is_empty());
    cache.has_more = has_more && cache.next_batch_id.is_some();
                                                                       
    if let Some(d) = rap_dsid.filter(|s| !s.trim().is_empty()) {
        cache.rap_dsid = Some(d);
    }
    if let Some(x) = rap_xsrf.filter(|s| !s.trim().is_empty()) {
        cache.rap_xsrf = Some(x);
    }
    cache.updated_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Err(e) = store_history_apps_cache(&cache) {
                                                                              
        eprintln!("[signum] history apps cache write failed: {e}");
    }
    cache
}

pub fn delete_history_apps_cache(email: &str) {
    let email = normalize_email(email);
    if let Ok(path) = history_cache_path(&email) {
        let _ = fs::remove_file(path);
    }
}

                                                                      
pub fn update_history_app_bundle(email: &str, adam_id: &str, bundle_id: &str) {
    let email = normalize_email(email);
    let adam_id = adam_id.trim();
    let bundle_id = bundle_id.trim();
    if email.is_empty() || adam_id.is_empty() || bundle_id.is_empty() {
        return;
    }
    if bundle_id.starts_with("app.store.") {
        return;
    }
    let Some(mut cache) = load_history_apps_cache(&email) else {
        return;
    };
    let mut changed = false;
    for app in &mut cache.apps {
        if app.adam_id == adam_id
            && (app.bundle_id.trim().is_empty()
                || app.bundle_id.starts_with("app.store.")
                || app.bundle_id != bundle_id)
        {
            app.bundle_id = bundle_id.to_string();
            changed = true;
        }
    }
    if changed {
        let _ = store_history_apps_cache(&cache);
    }
}
