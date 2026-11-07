//! App Store finance APIs used for catalog restore.
//!
//! ipatool-cpp on Windows can finish GSA login and still return an empty
//! `songList` because it does not send the Store passwordToken, cookies and
//! anisette headers that `volumeStoreDownloadProduct` requires. This module
//! mirrors the Android `AppStoreClient` (PET/password + anisette + cookie jar).

use plist::{Dictionary, Value as PlistValue};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE, LOCATION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const DEFAULT_STOREFRONT: &str = "143469-2,8";
const DEFAULT_POD: &str = "25";
/// Same product string ipatool already sends. The native store client used to
/// go out as a library default, which Apple treats as an unknown test app.
pub const CONFIGURATOR_UA: &str =
    "Configurator/2.17 (Macintosh; OS X 15.2; 24C5089c) AppleWebKit/0620.1.16.11.6";

static V6_GUID: Mutex<Option<String>> = Mutex::new(None);
static V6_SERIAL: Mutex<Option<String>> = Mutex::new(None);
static PROCESS_MAC: Mutex<Option<String>> = Mutex::new(None);
const AUTH_NATIVE: &str = "https://auth.itunes.apple.com/auth/v1/native/fast/";
const AUTH_LEGACY: &str = "https://buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/authenticate";
const BUY_PRODUCT: &str = "https://buy.itunes.apple.com/WebObjects/MZBuy.woa/wa/buyProduct";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreSession {
    #[serde(default)]
    pub kbsync: String,
    #[serde(default)]
    pub fserial: String,
    pub email: String,
    pub password_token: String,
    pub ds_person_id: String,
    #[serde(default = "default_storefront")]
    pub store_front: String,
    #[serde(default = "default_pod")]
    pub pod: String,
    pub guid: String,
    #[serde(default)]
    pub cookies: HashMap<String, String>,
}

fn default_storefront() -> String {
    DEFAULT_STOREFRONT.into()
}
fn default_pod() -> String {
    DEFAULT_POD.into()
}

impl StoreSession {
    pub fn is_ready(&self) -> bool {
        !self.password_token.is_empty() && !self.ds_person_id.is_empty() && !self.guid.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct VersionList {
    pub external_ids: Vec<String>,
    pub latest_id: Option<String>,
    pub latest_display: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VersionMeta {
    #[allow(dead_code)]
    pub external_id: String,
    pub display: String,
}

struct StorePlist {
    status: u16,
    headers: HeaderMap,
    dict: Option<Dictionary>,
}

fn session_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn memory_sessions() -> &'static Mutex<HashMap<String, StoreSession>> {
    static MAP: OnceLock<Mutex<HashMap<String, StoreSession>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn auth_failures() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    static MAP: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn recent_auth_failure(email: &str) -> Option<String> {
    let email = normalize_email(email);
    let map = auth_failures().lock().ok()?;
    let (msg, at) = map.get(&email)?;
    if at.elapsed() < Duration::from_secs(90) {
        Some(msg.clone())
    } else {
        None
    }
}

fn remember_auth_failure(email: &str, error: &str) {
    if let Ok(mut map) = auth_failures().lock() {
        map.insert(
            normalize_email(email),
            (error.to_string(), Instant::now()),
        );
    }
}

fn clear_auth_failure(email: &str) {
    if let Ok(mut map) = auth_failures().lock() {
        map.remove(&normalize_email(email));
    }
}

fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn email_key(email: &str) -> String {
    hex::encode(Sha256::digest(normalize_email(email).as_bytes()))
}

fn session_dir() -> Result<PathBuf, String> {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "Не удалось определить папку данных пользователя.".to_string())?;
    let dir = PathBuf::from(base).join("Signum").join("store-session");
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать хранилище сессии App Store: {e}"))?;
    Ok(dir)
}

fn session_path(email: &str) -> Result<PathBuf, String> {
    Ok(session_dir()?.join(format!("{}.json", &email_key(email)[..16.min(email_key(email).len())])))
}

fn guid_path() -> Result<PathBuf, String> {
    Ok(session_dir()?.join("guid.txt"))
}

/// V6 test: store guid is the account MAC (12 hex), not a machine-wide UUID.
pub fn set_v6_guid(guid: Option<String>) {
    let cleaned = guid.and_then(|raw| {
        let hex: String = raw
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect();
        if hex.len() == 12 {
            Some(hex.to_ascii_uppercase())
        } else {
            None
        }
    });
    if let Ok(mut slot) = V6_GUID.lock() {
        *slot = cleaned;
    }
}

pub fn set_process_mac(mac: Option<String>) {
    let cleaned = mac.and_then(|raw| {
        let hex: String = raw
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect();
        if hex.len() != 12 {
            return None;
        }
        let first = u8::from_str_radix(&hex[..2], 16).ok()?;
        let all_zero = hex.bytes().all(|byte| byte == b'0');
        let all_ff = hex.bytes().all(|byte| byte == b'f' || byte == b'F');
        let apple_placeholder = first == 0x02 && hex[2..].bytes().all(|byte| byte == b'0');
        if first & 0x01 != 0 || all_zero || all_ff || apple_placeholder {
            return None;
        }
        Some(hex.to_ascii_uppercase())
    });
    if let Ok(mut slot) = PROCESS_MAC.lock() {
        *slot = cleaned;
    }
}

fn process_mac() -> Option<String> {
    PROCESS_MAC.lock().ok().and_then(|slot| slot.clone())
}

fn v6_guid() -> Option<String> {
    V6_GUID.lock().ok().and_then(|slot| slot.clone())
}

/// V6: one Mac serial per Apple ID. Empty means the historical placeholder.
pub fn set_v6_serial(serial: Option<String>) {
    let cleaned = serial.and_then(|raw| {
        let serial = raw.trim().to_ascii_uppercase();
        if serial_looks_real(&serial) {
            Some(serial)
        } else {
            None
        }
    });
    if let Ok(mut slot) = V6_SERIAL.lock() {
        *slot = cleaned;
    }
}

pub fn v6_serial() -> Option<String> {
    V6_SERIAL.lock().ok().and_then(|slot| slot.clone())
}

fn serial_looks_real(serial: &str) -> bool {
    serial.len() == 10
        && serial
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && !serial.contains('I')
        && !serial.contains('O')
}

fn account_guid() -> String {
    process_mac()
        .or_else(v6_guid)
        .unwrap_or_else(load_or_create_guid)
}

fn load_or_create_guid() -> String {
    if let Ok(path) = guid_path() {
        if let Ok(raw) = fs::read_to_string(&path) {
            let cleaned: String = raw
                .chars()
                .filter(|c| c.is_ascii_hexdigit())
                .collect::<String>()
                .to_ascii_uppercase();
            if cleaned.len() >= 12 {
                return cleaned[..12].to_string();
            }
        }
        let guid = Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>()
            .to_ascii_uppercase();
        let _ = fs::write(path, &guid);
        return guid;
    }
    Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(12)
        .collect::<String>()
        .to_ascii_uppercase()
}

pub fn load_session(email: &str) -> Option<StoreSession> {
    let email = normalize_email(email);
    if email.is_empty() {
        return None;
    }
    if let Ok(map) = memory_sessions().lock() {
        if let Some(s) = map.get(&email) {
            if s.is_ready() {
                return Some(s.clone());
            }
        }
    }
    let path = session_path(&email).ok()?;
    let raw = fs::read(path).ok()?;
    let session: StoreSession = serde_json::from_slice(&raw).ok()?;
    if !session.is_ready() {
        return None;
    }
    if let Ok(mut map) = memory_sessions().lock() {
        map.insert(email, session.clone());
    }
    Some(session)
}

fn persist_session(session: &StoreSession) {
    let email = normalize_email(&session.email);
    if email.is_empty() || !session.is_ready() {
        return;
    }
    if let Ok(mut map) = memory_sessions().lock() {
        map.insert(email.clone(), session.clone());
    }
    if let Ok(path) = session_path(&email) {
        if let Ok(raw) = serde_json::to_vec(session) {
            let _ = fs::write(path, raw);
        }
    }
}

pub fn clear_session(email: &str) {
    let email = normalize_email(email);
    if let Ok(mut map) = memory_sessions().lock() {
        map.remove(&email);
    }
    clear_auth_failure(&email);
    if let Ok(path) = session_path(&email) {
        let _ = fs::remove_file(path);
    }
}

pub async fn lookup_adam_id(bundle_id: &str) -> Option<String> {
    let bundle = bundle_id.trim();
    if bundle.is_empty() || bundle == "NONE" {
        return None;
    }
    let client = cdn_client().ok()?;
    let url = format!(
        "https://itunes.apple.com/lookup?bundleId={}&entity=software&limit=1",
        urlencoding_lite(bundle)
    );
    let resp = client.get(&url).send().await.ok()?;
    let status = resp.status().as_u16();
    let body = resp.text().await.ok()?;
    crate::diag_log::http("GET", &url, status, "", body.as_bytes());
    let value: serde_json::Value = serde_json::from_str(&body).ok()?;
    let adam = value
        .pointer("/results/0/trackId")
        .and_then(|v| v.as_u64().map(|n| n.to_string()).or_else(|| v.as_str().map(|s| s.to_string())))?;
    adam.trim()
        .chars()
        .all(|c| c.is_ascii_digit())
        .then_some(adam)
}

fn urlencoding_lite(value: &str) -> String {
    let mut out = String::new();
    for b in value.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn helper_available(anisette: Option<&str>) -> bool {
    anisette
        .map(Path::new)
        .is_some_and(|path| path.is_file())
}

struct CachedAnisette {
    path: String,
    at: Instant,
    headers: HashMap<String, String>,
}

fn anisette_cache() -> &'static Mutex<Option<CachedAnisette>> {
    static CACHE: OnceLock<Mutex<Option<CachedAnisette>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn anisette_headers(anisette: Option<&str>) -> Result<HashMap<String, String>, String> {
    // macOS signs Store requests natively via CommerceKit; here `anisette` is the
    // bundled native sapsigner. No remote anisette headers — CommerceKit is the
    // device attestation, and remote anisette would make Apple treat this as a new
    // device. Register the sapsigner so token requests get X-Apple-ActionSignature.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        return match anisette {
            Some(p) if Path::new(p).is_file() => {
                set_sap_helper(Some(p.to_string()));
                Ok(HashMap::new())
            }
            _ => Err("Не найден SAP-подписант Apple. Добавьте sapsigner рядом с Linux ipatool или переустановите Signum.".to_string()),
        };
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
    {
    let path = anisette
        .map(Path::new)
        .filter(|p| p.is_file())
        .ok_or_else(|| {
            "Не хватает компонента Apple. Переустановите Signum и повторите вход.".to_string()
        })?;
    let path_key = path.to_string_lossy().into_owned();
    if let Ok(slot) = anisette_cache().lock() {
        if let Some(cached) = slot.as_ref() {
            if cached.path == path_key && cached.at.elapsed() < Duration::from_secs(20) {
                set_sap_helper(sap_helper_next_to(path));
                return Ok(cached.headers.clone());
            }
        }
    }
    let mut cmd = Command::new(path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = path.parent() {
        cmd.current_dir(dir);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Не удалось запустить anisette: {e}"))?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err("Apple не подтвердила этот компьютер. Повторите попытку.".into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut headers = HashMap::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        if key.is_empty() || val.is_empty() {
            continue;
        }
        headers.insert(key.to_string(), val.to_string());
    }
    if !headers.contains_key("X-Apple-I-MD") || !headers.contains_key("X-Apple-I-MD-M") {
        return Err("Apple не подтвердила этот компьютер. Проверьте интернет и повторите попытку.".into());
    }
    // Remember the sapsigner helper living next to anisette: Store requests
    // with a token need its signature (see sap_sign_body).
    set_sap_helper(sap_helper_next_to(path));
    if let Ok(mut slot) = anisette_cache().lock() {
        *slot = Some(CachedAnisette {
            path: path_key,
            at: Instant::now(),
            headers: headers.clone(),
        });
    }
    Ok(headers)
    }
}

#[cfg(not(target_os = "macos"))]
fn sap_helper_next_to(path: &Path) -> Option<String> {
    path.parent().and_then(|dir| {
        ["sapsigner.exe", "sapsigner"]
            .iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.is_file())
            .map(|p| p.to_string_lossy().into_owned())
    })
}

fn store_debug(msg: &str) {
    if let Ok(dir) = session_dir() {
        let line = format!("{} {msg}\n", chrono_lite());
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("store-debug.log"))
            .and_then(|mut f| {
                use std::io::Write;
                f.write_all(line.as_bytes())
            });
    }
}

fn chrono_lite() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn store_client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    match CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .use_native_tls()
            .user_agent(CONFIGURATOR_UA)
            .timeout(Duration::from_secs(40))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .cookie_store(true)
            .build()
            .map_err(|e| format!("HTTP client: {e}"))
    }) {
        Ok(client) => Ok(client),
        Err(e) => Err(e.clone()),
    }
}

fn cdn_client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    match CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .use_native_tls()
            .user_agent(CONFIGURATOR_UA)
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(12))
            .build()
            .map_err(|e| format!("HTTP client: {e}"))
    }) {
        Ok(client) => Ok(client),
        Err(e) => Err(e.clone()),
    }
}

fn plist_xml(fields: &[(&str, PlistValue)]) -> Result<Vec<u8>, String> {
    let mut dict = Dictionary::new();
    for (k, v) in fields {
        dict.insert((*k).to_string(), v.clone());
    }
    let mut buf = Vec::new();
    PlistValue::Dictionary(dict)
        .to_writer_xml(&mut buf)
        .map_err(|e| format!("Не удалось собрать запрос App Store: {e}"))?;
    Ok(buf)
}

fn parse_plist_dict(bytes: &[u8]) -> Option<Dictionary> {
    let value = PlistValue::from_reader_xml(bytes).ok()?;
    match value {
        PlistValue::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn dict_str(dict: &Dictionary, key: &str) -> Option<String> {
    dict.get(key).and_then(plist_to_string)
}

fn plist_to_string(value: &PlistValue) -> Option<String> {
    match value {
        PlistValue::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        }
        PlistValue::Integer(i) => i
            .as_unsigned()
            .map(|n| n.to_string())
            .or_else(|| i.as_signed().map(|n| n.to_string())),
        PlistValue::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn nested_dict<'a>(dict: &'a Dictionary, key: &str) -> Option<&'a Dictionary> {
    match dict.get(key) {
        Some(PlistValue::Dictionary(d)) => Some(d),
        _ => None,
    }
}

fn remember_pod(session: &mut StoreSession, url: &str) {
    let lower = url.to_ascii_lowercase();
    if let Some(idx) = lower.find("-buy.itunes.apple.com") {
        let prefix = &url[..idx];
        if let Some(ppos) = prefix.rfind(['p', 'P']) {
            let num = &prefix[ppos + 1..];
            if !num.is_empty() && num.len() <= 3 && num.chars().all(|c| c.is_ascii_digit()) {
                session.pod = num.to_string();
            }
        }
    }
    if let Some(idx) = lower.find("pod=") {
        let num: String = url[idx + 4..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !num.is_empty() {
            session.pod = num;
        }
    }
}

fn ingest_cookies(session: &mut StoreSession, headers: &HeaderMap) {
    for value in headers.get_all("set-cookie") {
        let Ok(line) = value.to_str() else { continue };
        let pair = line.split(';').next().unwrap_or("");
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim();
        if k.is_empty() {
            continue;
        }
        if v.is_empty() {
            session.cookies.remove(k);
        } else {
            session.cookies.insert(k.to_string(), v.to_string());
        }
        if k.eq_ignore_ascii_case("itspod") {
            session.pod = v.to_string();
        }
    }
    if let Some(sf) = headers
        .get("x-set-apple-store-front")
        .or_else(|| headers.get("X-Set-Apple-Store-Front"))
        .or_else(|| headers.get("x-apple-store-front"))
        .and_then(|v| v.to_str().ok())
    {
        let sf = sf.trim();
        if !sf.is_empty() {
            session.store_front = sf.to_string();
        }
    }
}

fn resolve_url(base: &str, loc: &str) -> String {
    let loc = loc.trim();
    if loc.starts_with("http://") || loc.starts_with("https://") {
        return loc.to_string();
    }
    if let Some(rest) = loc.strip_prefix("//") {
        return format!("https:{rest}");
    }
    if loc.starts_with('/') {
        if let Ok(url) = reqwest::Url::parse(base) {
            return format!(
                "{}://{}{}",
                url.scheme(),
                url.host_str().unwrap_or("buy.itunes.apple.com"),
                loc
            );
        }
    }
    loc.to_string()
}

/// SAP-sign a Store request body with the bundled sapsigner helper.
/// Apple rejects token-carrying requests (volumeStoreDownloadProduct, …) with
/// an empty songList / AMD-Action when `X-Apple-ActionSignature` is missing.
/// The helper location is remembered by anisette_headers() (same tools dir).
fn sap_sign_body(body: &[u8]) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let helper = sap_helper_path().ok_or_else(|| {
        "Не найден Apple-подписант SAP. Переустановите Signum, чтобы восстановить компоненты App Store.".to_string()
    })?;
    let mut cmd = Command::new(&helper);
    #[cfg(target_os = "macos")]
    if let Some(dir) = Path::new(&helper).parent() {
        let mut paths = vec![dir.to_path_buf()];
        if let Some(existing) = std::env::var_os("DYLD_FALLBACK_LIBRARY_PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(paths) = std::env::join_paths(paths) {
            cmd.env("DYLD_FALLBACK_LIBRARY_PATH", paths);
        }
    }
    if let Some(guid) = process_mac().or_else(v6_guid) {
        cmd.env("SAPSIGNER_MAC", guid.to_ascii_lowercase());
    }
    // Подписант запускается только с ключом, который Signum отдаёт и ipatool.
    // Без него он падал с «запуск разрешён только из Signum», и подписи версий
    // в каталоге шли медленным путём через ipatool.
    cmd.env(crate::SEALED_ASSET_KEY_ENV, crate::sealed_asset_key());
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = Path::new(&helper).parent() {
        cmd.current_dir(dir);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            // A bundled signer that lost its executable bit fails exactly here,
            // which is why macOS builds showed raw build numbers in the picker.
            crate::diag_log::note("app", &format!("sap signer not runnable: {helper}: {error}"));
            return Err(format!("Не удалось запустить Apple-подписант SAP: {error}"));
        }
    };
    {
        use std::io::Write as _;
        let mut stdin = child.stdin.take().ok_or_else(|| "Apple-подписант SAP не открыл входной поток.".to_string())?;
        if stdin.write_all(body).is_err() {
            let _ = child.kill();
            return Err("Не удалось передать запрос Apple-подписанту SAP.".into());
        }
        // stdin dropped here → EOF for the helper.
    }
    let output = child.wait_with_output().map_err(|error| format!("Не удалось получить ответ SAP-подписанта: {error}"))?;
    if !output.status.success() || output.stdout.is_empty() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().chars().take(240).collect::<String>();
        crate::diag_log::note(
            "app",
            &format!(
                "sap sign failed helper={helper}: {detail}"
            ),
        );
        return Err(if detail.is_empty() {
            format!("Apple-подписант SAP завершился с кодом {} без подписи. Проверьте комплект библиотек Signum.", output.status)
        } else {
            format!("Apple-подписант SAP не смог подписать запрос: {detail}")
        });
    }
    Ok(B64.encode(output.stdout))
}

// Global, NOT thread-local: sap_sign_body runs inside tokio::spawn_blocking on a
// blocking-pool thread, which would never see a value set on the async worker.
// version fetches serialize under session_lock and all use the same signer path,
// so a single shared slot is race-free.
static SAP_HELPER: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Remember the sapsigner location (next to anisette on Windows, the bundled
/// native CommerceKit signer on macOS).
fn set_sap_helper(path: Option<String>) {
    if let Ok(mut slot) = SAP_HELPER.lock() {
        *slot = path;
    }
}

fn sap_helper_path() -> Option<String> {
    SAP_HELPER.lock().ok().and_then(|slot| slot.clone())
}

fn apply_common_headers(
    mut builder: reqwest::RequestBuilder,
    session: &StoreSession,
    ani: &HashMap<String, String>,
    with_token: bool,
) -> reqwest::RequestBuilder {
    builder = builder
        .header("Accept", "*/*")
        .header("Accept-Language", "en-us")
        .header("User-Agent", CONFIGURATOR_UA)
        .header("X-Apple-Store-Front", &session.store_front);
    if with_token {
        builder = builder
            .header("X-Dsid", &session.ds_person_id)
            .header("iCloud-DSID", &session.ds_person_id)
            .header("X-Token", &session.password_token);
        // X-Apple-ActionSignature set by post_plist (real SAP signature).
    }
    if !session.cookies.is_empty() {
        let cookie = session
            .cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        builder = builder.header("Cookie", cookie);
    }
    for (k, v) in ani {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            builder = builder.header(name, val);
        }
    }
    builder
}

async fn post_plist(
    session: &mut StoreSession,
    start_url: &str,
    body: Vec<u8>,
    ani: &HashMap<String, String>,
    with_token: bool,
    content_type: &str,
    sap_sign: bool,
) -> Result<StorePlist, String> {
    let client = store_client()?;
    let mut url = start_url.to_string();
    // Purchase requests are SAP-signed once; the body never changes across
    // pod redirects (Apple expects the original plist bytes, signature incl.).
    // Metadata requests skip the signature: sapsigner costs ~6 s per run.
    let action_signature = if with_token && sap_sign {
        let body_for_sign = body.clone();
        Some(tokio::task::spawn_blocking(move || sap_sign_body(&body_for_sign))
            .await
            .map_err(|e| e.to_string())??)
    } else {
        None
    };
    for _ in 0..6 {
        remember_pod(session, &url);
        let mut builder = client
            .post(&url)
            .header(CONTENT_TYPE, content_type)
            .body(body.clone());
        if let Some(signature) = action_signature.as_deref() {
            builder = builder.header("X-Apple-ActionSignature", signature);
        }
        builder = apply_common_headers(builder, session, ani, with_token);
        let resp = builder.send().await.map_err(|e| {
            crate::diag_log::http_err("POST", &url, &e.to_string());
            "App Store не ответил. Проверьте интернет и повторите.".to_string()
        })?;
        let status = resp.status().as_u16();
        store_debug(&format!(
            "http {status} {} token={}",
            url.split('/').nth(2).unwrap_or("?"),
            with_token
        ));
        let headers = resp.headers().clone();
        ingest_cookies(session, &headers);
        if (300..400).contains(&status) {
            let loc = headers
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .trim()
                .to_string();
            crate::diag_log::http(
                "POST",
                &url,
                status,
                &format!("redirect loc={}", loc.chars().take(180).collect::<String>()),
                b"",
            );
            if loc.is_empty() {
                return Err("App Store вернул редирект без адреса.".into());
            }
            url = resolve_url(&url, &loc);
            remember_pod(session, &url);
            continue;
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|_| "App Store вернул пустой ответ.".to_string())?;
        let ct = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        crate::diag_log::http("POST", &url, status, &format!("ct={ct} token={with_token}"), &bytes);
        let dict = parse_plist_dict(&bytes);
        if let Some(d) = dict.as_ref() {
            if let Some(sf) = dict_str(d, "storeFront") {
                session.store_front = sf;
            }
        }
        return Ok(StorePlist {
            status,
            headers,
            dict,
        });
    }
    Err("Слишком много редиректов App Store.".into())
}

fn failure_text(dict: &Dictionary) -> (String, String) {
    let fail = dict_str(dict, "failureType").unwrap_or_default();
    let msg = dict_str(dict, "customerMessage")
        .or_else(|| {
            nested_dict(dict, "dialog").and_then(|d| {
                dict_str(d, "message").or_else(|| dict_str(d, "explanation"))
            })
        })
        .unwrap_or_default();
    (fail, msg)
}

fn is_password_token_failure(fail: &str, msg: &str) -> bool {
    let text = format!("{fail} {msg}").to_ascii_lowercase();
    fail == "2034"
        || fail == "2042"
        || fail == "2059"
        || text.contains("password token")
        || (text.contains("passwordtoken") && text.contains("expir"))
        || (text.contains("sign in") && text.contains("required"))
}

fn is_unknown_store_failure(fail: &str, msg: &str) -> bool {
    format!("{fail} {msg}")
        .to_ascii_lowercase()
        .contains("unknown error")
}

fn is_itunes_signin(fail: &str, msg: &str) -> bool {
    let text = format!("{fail} {msg}").to_ascii_lowercase();
    text.contains("sign in to the itunes") || text.contains("sign in to the app store")
}

fn is_two_factor(fail: &str, msg: &str) -> bool {
    let text = format!("{fail} {msg}").to_ascii_lowercase();
    fail == "2034"
        || text.contains("two-factor")
        || text.contains("two factor")
        || (text.contains("code") && text.contains("enter"))
}

fn is_license_failure(fail: &str, msg: &str) -> bool {
    let text = format!("{fail} {msg}").to_ascii_lowercase();
    fail == "9610"
        || fail == "1002"
        || text.contains("license")
        || text.contains("not purchased")
        || text.contains("songlist")
}

fn is_terms_text(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("terms and conditions")
        || t.contains("terms & conditions")
        || t.contains("media services")
        || t.contains("условия")
}

pub async fn authenticate(
    anisette: Option<&str>,
    email: &str,
    password: &str,
) -> Result<StoreSession, String> {
    let _guard = session_lock().lock().await;
    authenticate_locked(anisette, email, password).await
}

async fn authenticate_locked(
    anisette: Option<&str>,
    email: &str,
    password: &str,
) -> Result<StoreSession, String> {
    let email = normalize_email(email);
    let password = password.trim();
    if email.is_empty() || password.is_empty() {
        return Err("Введите Apple ID и пароль".into());
    }
    if let Some(prev) = recent_auth_failure(&email) {
        return Err(prev);
    }
    let ani = anisette_headers(anisette)?;
    let guid = account_guid();
    let mut session = StoreSession {
        kbsync: String::new(),
        fserial: String::new(),
        email: email.clone(),
        password_token: String::new(),
        ds_person_id: String::new(),
        store_front: DEFAULT_STOREFRONT.into(),
        pod: DEFAULT_POD.into(),
        guid: guid.clone(),
        cookies: HashMap::new(),
    };
    let mut last_error = "App Store не принял вход. Войдите в Apple ID снова.".to_string();
    for endpoint in [AUTH_NATIVE, AUTH_LEGACY] {
        let mut url = endpoint.to_string();
        for attempt in 1..=4 {
            let request_attempt = if url != endpoint { 1 } else { attempt };
            let body = plist_xml(&[
                ("appleId", PlistValue::String(email.clone())),
                ("attempt", PlistValue::String(request_attempt.to_string())),
                ("guid", PlistValue::String(guid.clone())),
                ("password", PlistValue::String(password.to_string())),
                ("rmp", PlistValue::String("0".into())),
                ("why", PlistValue::String("signIn".into())),
            ])?;
            let result = post_plist(
                &mut session,
                &url,
                body,
                &ani,
                false,
                "application/x-www-form-urlencoded",
                false,
            )
            .await;
            let plist = match result {
                Ok(p) => p,
                Err(e) => {
                    last_error = e;
                    break;
                }
            };
            if (300..400).contains(&plist.status) {
                if let Some(loc) = plist.headers.get(LOCATION).and_then(|v| v.to_str().ok()) {
                    url = resolve_url(&url, loc);
                    continue;
                }
            }
            let Some(dict) = plist.dict.as_ref() else {
                last_error = "App Store вернул страницу входа вместо сессии. Войдите в Apple ID снова.".into();
                break;
            };
            if let Some(token) = dict_str(dict, "passwordToken") {
                session.password_token = token;
            }
            session.ds_person_id = dict_str(dict, "dsPersonId")
                .or_else(|| {
                    nested_dict(dict, "accountInfo").and_then(|a| dict_str(a, "dsPersonId"))
                })
                .unwrap_or_default();
            if session.is_ready() {
                clear_auth_failure(&email);
                persist_session(&session);
                store_debug(&format!(
                    "auth-ok host={} token_len={} dsid={}",
                    url.split('/').nth(2).unwrap_or("?"),
                    session.password_token.len(),
                    !session.ds_person_id.is_empty()
                ));
                return Ok(session);
            }
            let (fail, msg) = failure_text(dict);
            if is_two_factor(&fail, &msg) {
                return Err(format!("SIGNUM_2FA_REQUIRED:{msg}"));
            }
            if attempt == 1 && (fail == "-5000" || msg.to_ascii_lowercase().contains("password")) {
                continue;
            }
            last_error = if !msg.is_empty() {
                msg
            } else if !fail.is_empty() {
                fail
            } else {
                last_error
            };
            break;
        }
    }
    store_debug(&format!("auth-fail {last_error}"));
    remember_auth_failure(&email, &last_error);
    Err(last_error)
}

async fn ensure_session_locked(
    anisette: Option<&str>,
    email: &str,
    force: bool,
) -> Result<StoreSession, String> {
    if !force {
        if let Some(existing) = load_session(email) {
            if existing.is_ready() {
                return Ok(existing);
            }
        }
    }
    let password = crate::history_secrets::load_history_password(email).ok_or_else(|| {
        "Сессия Apple ID для App Store устарела. Откройте «Аккаунт», выйдите и войдите снова."
            .to_string()
    })?;
    authenticate_locked(anisette, email, &password).await
}

fn decrypt_ipatool_account(raw: &[u8], machine: &str, passphrase: &str) -> Option<serde_json::Value> {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    if raw.len() < 49 || raw[0] != 2 || raw.get(1..5)? != 16u32.to_le_bytes() {
        return None;
    }
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(
        format!("{machine}nice_key_is_nice{passphrase}").as_bytes(), &raw[5..21], 100_000, &mut key,
    );
    let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
    let mut ciphertext = raw[49..].to_vec();
    ciphertext.extend_from_slice(&raw[33..49]);
    let plaintext = cipher.decrypt(Nonce::from_slice(&raw[21..33]), ciphertext.as_ref()).ok()?;
    serde_json::from_slice(&plaintext).ok()
}

#[cfg(windows)]
fn ipatool_machine_identity() -> Option<(String, String)> {
    use std::os::windows::process::CommandExt;
    let registry = |path: &str, name: &str| -> Option<String> {
        let output = Command::new("reg.exe").args(["query", path, "/v", name])
            .creation_flags(0x08000000).output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines().find_map(|line| line.split_once("REG_SZ").map(|(_, value)| value.trim().to_string()))
    };
    let product = registry(r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "ProductId")?;
    let guid = registry(r"HKLM\SOFTWARE\Microsoft\Cryptography", "MachineGuid")?;
    let machine = hex::encode(Sha256::digest(format!("{product}{guid}").as_bytes()));
    let output = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command",
        "Get-NetAdapter -Physical | Where-Object { $_.NdisPhysicalMedium -ne 10 } | Sort-Object @{Expression={if ($_.InterfaceDescription -match 'USB') {2} else {0}}}, @{Expression={if ($_.InterfaceType -eq 6) {0} else {1}}}, PermanentAddress | ForEach-Object { if ($_.PermanentAddress) {$_.PermanentAddress} else {$_.MacAddress} }"])
        .creation_flags(0x08000000).output().ok()?;
    let hardware_guid = String::from_utf8_lossy(&output.stdout).lines()
        .find_map(valid_hardware_guid)?;
    Some((machine, hardware_guid))
}

fn valid_hardware_guid(raw: &str) -> Option<String> {
    let value: String = raw.trim().chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if value.len() != 12 { return None; }
    let first = u8::from_str_radix(&value[..2], 16).ok()?;
    let value = value.to_ascii_uppercase();
    (first & 1 == 0 && !["000000000000", "FFFFFFFFFFFF", "020000000000", "AABBCCDDEEFF"]
        .contains(&value.as_str())).then_some(value)
}

#[cfg(target_os = "linux")]
fn ipatool_machine_identity() -> Option<(String, String)> {
    let id = fs::read_to_string("/etc/machine-id")
        .or_else(|_| fs::read_to_string("/var/lib/dbus/machine-id")).unwrap_or_default();
    let uuid = fs::read_to_string("/sys/class/dmi/id/product_uuid").unwrap_or_default();
    let material = format!("{}{}", id.trim(), uuid.trim());
    let machine = if material.is_empty() { "fallback-linux".into() }
        else { hex::encode(Sha256::digest(material.as_bytes())) };
    let mut adapters: Vec<_> = fs::read_dir("/sys/class/net").ok()?.filter_map(Result::ok)
        .filter(|entry| entry.path().join("device").exists())
        .filter_map(|entry| {
            let mac = valid_hardware_guid(&fs::read_to_string(entry.path().join("address")).ok()?)?;
            Some((entry.path().join("wireless").exists(), entry.file_name(), mac))
        }).collect();
    adapters.sort();
    Some((machine, adapters.first()?.2.clone()))
}

#[cfg(target_os = "macos")]
fn ipatool_machine_identity() -> Option<(String, String)> {
    let output = Command::new("/usr/sbin/ioreg").args(["-a", "-rd1", "-c", "IOPlatformExpertDevice"]).output().ok()?;
    let nodes: Vec<Dictionary> = plist::from_bytes(&output.stdout).ok()?;
    let node = nodes.first()?;
    let serial = dict_str(node, "IOPlatformSerialNumber")?;
    let uuid = dict_str(node, "IOPlatformUUID")?;
    let machine = hex::encode(Sha256::digest(format!("{serial}{uuid}").as_bytes()));
    let output = Command::new("/sbin/ifconfig").arg("en0").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mac = text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next()? == "ether").then(|| fields.next().and_then(valid_hardware_guid)).flatten()
    })?;
    Some((machine, mac))
}

pub fn load_ipatool_session(email: &str, passphrase: &str) -> Option<StoreSession> {
    let (machine, hardware_guid) = ipatool_machine_identity()?;
    let dir = crate::ipatool_data_dir()?;
    let account = decrypt_ipatool_account(&fs::read(dir.join("account")).ok()?, &machine, passphrase)?;
    if normalize_email(account.get("email")?.as_str()?) != normalize_email(email) {
        return None;
    }
    let mut session = StoreSession {
        kbsync: account.get("kbsync").and_then(|v| v.as_str()).unwrap_or_default().into(),
        fserial: account.get("fserial").and_then(|v| v.as_str()).unwrap_or_default().into(),
        email: normalize_email(email),
        password_token: account.get("passwordToken")?.as_str()?.to_string(),
        ds_person_id: account.get("directoryServicesID")?.as_str()?.to_string(),
        store_front: account.get("storeFront")?.as_str()?.to_string(),
        pod: account.get("pod")?.as_str()?.to_string(),
        guid: if crate::ACCOUNT_IDENTITY_SPOOF {
            crate::load_or_create_account_mac(email).0.replace(":", "")
        } else { hardware_guid },
        cookies: HashMap::new(),
    };
    for line in fs::read_to_string(dir.join("cookies")).ok()?.lines() {
        let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        if line.starts_with('#') { continue; }
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() == 7 && (parts[0] == ".apple.com" || parts[0].ends_with(".itunes.apple.com")) {
            session.cookies.insert(parts[5].into(), parts[6].into());
        }
    }
    if !session.is_ready() { return None; }
    persist_session(&session);
    Some(session)
}

fn download_url(session: &StoreSession) -> String {
    format!(
        "https://p{}-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/volumeStoreDownloadProduct?guid={}",
        session.pod, session.guid
    )
}

async fn download_plist(
    session: &mut StoreSession,
    anisette: Option<&str>,
    adam_id: &str,
    external_version_id: Option<&str>,
    purchase: bool,
) -> Result<Dictionary, String> {
    let ani = anisette_headers(anisette)?;
    let mut fields = vec![
        ("creditDisplay", PlistValue::String(String::new())),
        ("guid", PlistValue::String(session.guid.clone())),
        ("salableAdamId", PlistValue::String(adam_id.to_string())),
        (
            "passwordToken",
            PlistValue::String(session.password_token.clone()),
        ),
        (
            "dsPersonId",
            PlistValue::String(session.ds_person_id.clone()),
        ),
    ];
    if purchase {
        fields.push(("ageCheck", PlistValue::Boolean(true)));
    }
    if let Some(ext) = external_version_id.map(str::trim).filter(|v| !v.is_empty() && *v != "latest")
    {
        fields.push(("externalVersionId", PlistValue::String(ext.to_string())));
    }
    let body = plist_xml(&fields)?;
    let url = download_url(session);
    // SAP signature required for both metadata and purchase requests: Apple
    // answers unsigned token calls with an empty songList (no version labels).
    // The per-run cost is handled by the version label cache upstream.
    let plist = post_plist(session, &url, body, &ani, true, "application/x-apple-plist", true)
        .await?;
    let dict = plist
        .dict
        .ok_or_else(|| "Не удалось получить данные App Store".to_string())?;

    // Since about 2026-09-01 Apple stopped answering volumeStoreDownloadProduct
    // for builds published on or after that date — the reply carries no
    // songList. The bundled ipatool already learned to fall back to
    // DownloadDispatch, which is why those builds still download while their
    // version labels came back empty and the picker showed a raw build number.
    // Same rung here, so the native client sees what ipatool sees.
    if first_song(&dict).is_none() {
        if let Some(updated) =
            update_product_plist(session, anisette, adam_id, external_version_id).await
        {
            if first_song(&updated).is_some() {
                return Ok(updated);
            }
        }
    }
    Ok(dict)
}

/// DownloadDispatch's updateProduct. The Store bag names this endpoint; the
/// bundled ipatool reads it from there and falls back to exactly this URL when
/// the bag is silent, so the constant matches what already works in practice.
const UPDATE_PRODUCT: &str = "https://downloaddispatch.itunes.apple.com/up/updateProduct";

/// Keys differ from volumeStoreDownloadProduct on purpose: this endpoint wants
/// `appExtVrsId` rather than `externalVersionId`, a numeric `salableAdamId`,
/// and a `serialNumber`. Sending the old shape returns an empty songList.
async fn update_product_plist(
    session: &mut StoreSession,
    anisette: Option<&str>,
    adam_id: &str,
    external_version_id: Option<&str>,
) -> Option<Dictionary> {
    let ani = anisette_headers(anisette).ok()?;
    let adam: i64 = adam_id.trim().parse().ok()?;
    let mut fields = vec![
        ("creditDisplay", PlistValue::String(String::new())),
        ("guid", PlistValue::String(session.guid.clone())),
        ("salableAdamId", PlistValue::Integer(adam.into())),
        (
            "serialNumber",
            PlistValue::String(v6_serial().unwrap_or_else(|| "0".into())),
        ),
    ];
    if let Some(ext) = external_version_id
        .map(str::trim)
        .filter(|v| !v.is_empty() && *v != "latest")
    {
        fields.push(("appExtVrsId", PlistValue::String(ext.to_string())));
    }
    let body = plist_xml(&fields).ok()?;
    let url = format!("{UPDATE_PRODUCT}?guid={}", session.guid);
    let plist = post_plist(
        session,
        &url,
        body,
        &ani,
        true,
        "application/x-apple-plist",
        true,
    )
    .await
    .ok()?;
    plist.dict
}

fn first_song(dict: &Dictionary) -> Option<&Dictionary> {
    match dict.get("songList") {
        Some(PlistValue::Array(items)) => items.iter().find_map(|item| match item {
            PlistValue::Dictionary(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

fn collect_ids(value: &PlistValue) -> Vec<String> {
    match value {
        PlistValue::Array(items) => items.iter().filter_map(plist_to_string).collect(),
        other => plist_to_string(other).into_iter().collect(),
    }
}

fn extract_version_ids(song: &Dictionary) -> VersionList {
    let meta = nested_dict(song, "metadata")
        .or_else(|| nested_dict(song, "item-meta-data"))
        .unwrap_or(song);
    let id_keys = [
        "softwareVersionExternalIdentifiers",
        "software-version-external-identifiers",
        "externalVersionIdentifiers",
        "external-version-identifiers",
    ];
    let mut ids = Vec::new();
    for key in id_keys {
        if let Some(value) = song.get(key).or_else(|| meta.get(key)) {
            for id in collect_ids(value) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    let latest_keys = [
        "softwareVersionExternalIdentifier",
        "software-version-external-identifier",
        "externalVersionId",
        "external-id",
    ];
    let mut latest = None;
    for key in latest_keys {
        if let Some(id) = song
            .get(key)
            .or_else(|| meta.get(key))
            .and_then(plist_to_string)
        {
            latest = Some(id.clone());
            if !ids.contains(&id) {
                ids.push(id);
            }
            break;
        }
    }
    let display_keys = [
        "bundleShortVersionString",
        "bundle-short-version-string",
        "shortVersionString",
        "displayVersion",
        "versionString",
        "bundleVersion",
    ];
    let mut latest_display = None;
    for key in display_keys {
        if let Some(raw) = dict_str(song, key).or_else(|| dict_str(meta, key)) {
            if raw.chars().all(|c| c.is_ascii_digit()) && raw.len() >= 8 {
                continue;
            }
            latest_display = Some(raw);
            break;
        }
    }
    if let Some(latest_id) = latest.clone() {
        if let Some(pos) = ids.iter().position(|id| id == &latest_id) {
            ids.remove(pos);
            ids.insert(0, latest_id);
        }
    }
    VersionList {
        external_ids: ids,
        latest_id: latest,
        latest_display,
    }
}

pub async fn list_versions(
    anisette: Option<&str>,
    email: &str,
    adam_id: &str,
) -> Result<VersionList, String> {
    let adam_id = adam_id.trim();
    if adam_id.is_empty() || !adam_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Не указан корректный идентификатор App Store".into());
    }
    let _guard = session_lock().lock().await;
    let mut session = ensure_session_locked(anisette, email, false).await?;
    let dict = download_plist(&mut session, anisette, adam_id, None, false).await?;
    persist_session(&session);
    let (fail, msg) = failure_text(&dict);
    if is_password_token_failure(&fail, &msg)
        || is_unknown_store_failure(&fail, &msg)
        || is_itunes_signin(&fail, &msg)
    {
        let mut session = ensure_session_locked(anisette, email, true).await?;
        let dict = download_plist(&mut session, anisette, adam_id, None, false).await?;
        persist_session(&session);
        return versions_from_download(&dict);
    }
    versions_from_download(&dict)
}

fn versions_from_download(dict: &Dictionary) -> Result<VersionList, String> {
    let (fail, msg) = failure_text(dict);
    if is_license_failure(&fail, &msg) {
        return Ok(VersionList::default());
    }
    let Some(song) = first_song(dict) else {
        if fail.is_empty() || fail == "0" {
            return Ok(VersionList::default());
        }
        if !msg.is_empty() {
            return Err(msg);
        }
        return Ok(VersionList::default());
    };
    Ok(extract_version_ids(song))
}

// Marketing version labels ("v8.0.1") are stable per (adamId, externalVersionId).
// Resolving one costs a signed store request (~6 s of SAP emulation), so a
// page of 12 versions without this cache used to take a minute+ every time.
fn version_label_cache_path() -> Result<PathBuf, String> {
    Ok(session_dir()?.join("version-labels.json"))
}

fn version_label_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(load_version_labels_from_disk()))
}

fn load_version_labels_from_disk() -> HashMap<String, String> {
    let Ok(path) = version_label_cache_path() else {
        return HashMap::new();
    };
    fs::read(path)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn store_version_label(adam_id: &str, ext: &str, display: &str) {
    if display.trim().is_empty() || display == ext {
        return;
    }
    let key = format!("{}:{}", adam_id.trim(), ext.trim());
    let cache = version_label_cache();
    let Ok(mut map) = cache.lock() else {
        return;
    };
    if map.get(&key).map(String::as_str) == Some(display) {
        return;
    }
    map.insert(key.clone(), display.to_string());
    if let Ok(path) = version_label_cache_path() {
        if let Ok(raw) = serde_json::to_vec(&*map) {
            let _ = fs::write(path, raw);
        }
    }
}

/// Remember a marketing label resolved outside this module (ipatool fallback).
///
/// The picker keeps one shared label cache, so a label found by either engine
/// is instant on the next visit.
pub fn remember_version_label(adam_id: &str, ext: &str, display: &str) {
    store_version_label(adam_id, ext, display);
}

/// Labels already known for these versions, without touching the network.
pub fn cached_version_labels(adam_id: &str, external_ids: &[String]) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    for ext in external_ids {
        let ext = ext.trim();
        if let Some(display) = cached_version_label(adam_id, ext) {
            labels.insert(ext.to_string(), display);
        }
    }
    labels
}

fn cached_version_label(adam_id: &str, ext: &str) -> Option<String> {
    let key = format!("{}:{}", adam_id.trim(), ext.trim());
    let map = version_label_cache().lock().ok()?;
    map.get(&key)
        .filter(|d| !d.trim().is_empty() && d.as_str() != ext)
        .cloned()
}

pub async fn version_display(
    anisette: Option<&str>,
    email: &str,
    adam_id: &str,
    external_version_id: &str,
) -> Result<VersionMeta, String> {
    let ext = external_version_id.trim();
    if ext.is_empty() {
        return Ok(VersionMeta {
            external_id: ext.to_string(),
            display: ext.to_string(),
        });
    }
    // Labels are immutable per (adamId, ext): one slow signed resolve ever,
    // then instant from cache — this is what makes the version picker fast.
    if let Some(display) = cached_version_label(adam_id, ext) {
        return Ok(VersionMeta {
            external_id: ext.to_string(),
            display,
        });
    }
    let _guard = session_lock().lock().await;
    let mut session = ensure_session_locked(anisette, email, false).await?;
    let dict = download_plist(&mut session, anisette, adam_id, Some(ext), false).await?;
    persist_session(&session);
    let display = first_song(&dict)
        .map(extract_version_ids)
        .and_then(|v| v.latest_display)
        .filter(|s| !(s.chars().all(|c| c.is_ascii_digit()) && s.len() >= 8))
        .unwrap_or_else(|| ext.to_string());
    if display != ext {
        store_version_label(adam_id, ext, &display);
    }
    Ok(VersionMeta {
        external_id: ext.to_string(),
        display,
    })
}

/// How many version labels are resolved at once.
///
/// Each one costs a SAP signature (a helper process, seconds) plus a round trip
/// to Apple, so overlapping them is the whole win. Kept modest because the
/// signer processes share a cache directory.
const VERSION_LABEL_CONCURRENCY: usize = 4;

/// Resolve the labels for a whole page of versions in one go.
///
/// [`version_display`] holds the global session mutex for its entire call, so a
/// page of twelve resolved strictly one after another — minutes for an app with
/// a long history like VK. Here the session is established once under the lock
/// and then cloned per request, so the requests actually overlap. Results are
/// written to the same label cache, so a second visit stays instant.
pub async fn version_displays(
    anisette: Option<&str>,
    email: &str,
    adam_id: &str,
    external_ids: &[String],
) -> HashMap<String, String> {
    use futures_util::{stream, StreamExt};

    let mut labels = HashMap::new();
    let mut missing = Vec::new();
    for ext in external_ids {
        let ext = ext.trim();
        if ext.is_empty() {
            continue;
        }
        match cached_version_label(adam_id, ext) {
            Some(display) => {
                labels.insert(ext.to_string(), display);
            }
            None => missing.push(ext.to_string()),
        }
    }
    if missing.is_empty() {
        return labels;
    }

    // Establish the session once; everything after this is read-only as far as
    // the stored session is concerned.
    let base = {
        let _guard = session_lock().lock().await;
        match ensure_session_locked(anisette, email, false).await {
            Ok(session) => {
                persist_session(&session);
                session
            }
            Err(error) => {
                // Silent here used to mean «every version shows as «sborka»» with
                // nothing in the logs to say why. Record it once per page.
                store_debug(&format!("version-labels session failed: {error}"));
                crate::diag_log::note("app", &format!("version labels unavailable: {error}"));
                return labels;
            }
        }
    };

    let fetched: Vec<Option<(String, String)>> = stream::iter(missing)
        .map(|ext| {
            let mut session = base.clone();
            let anisette = anisette.map(str::to_string);
            let adam = adam_id.to_string();
            async move {
                let dict =
                    download_plist(&mut session, anisette.as_deref(), &adam, Some(&ext), false)
                        .await
                        .ok()?;
                let display = first_song(&dict)
                    .map(extract_version_ids)
                    .and_then(|ids| ids.latest_display)
                    .filter(|s| !(s.chars().all(|c| c.is_ascii_digit()) && s.len() >= 8))?;
                Some((ext, display))
            }
        })
        .buffer_unordered(VERSION_LABEL_CONCURRENCY)
        .collect()
        .await;

    for (ext, display) in fetched.into_iter().flatten() {
        store_version_label(adam_id, &ext, &display);
        labels.insert(ext, display);
    }
    labels
}

#[derive(Debug, Default)]
pub struct VersionFacts {
    pub labels: HashMap<String, String>,
    pub min_ios: HashMap<String, String>,
}

enum VersionRow {
    Ready {
        ext: String,
        label: Option<String>,
        url: Option<String>,
    },
    /// Store session is stale. One re-login covers the whole page.
    Auth(String),
}

fn version_row(ext: String, dict: &Dictionary) -> VersionRow {
    let (fail, msg) = failure_text(dict);
    if is_password_token_failure(&fail, &msg)
        || is_unknown_store_failure(&fail, &msg)
        || is_itunes_signin(&fail, &msg)
    {
        return VersionRow::Auth(ext);
    }
    let song = first_song(dict);
    let label = song
        .map(extract_version_ids)
        .and_then(|ids| ids.latest_display)
        .filter(|display| !(display.chars().all(|c| c.is_ascii_digit()) && display.len() >= 8));
    let url = song
        .and_then(|song| dict_str(song, "URL"))
        .filter(|url| !url.trim().is_empty());
    VersionRow::Ready { ext, label, url }
}

async fn fetch_version_rows(
    session: &StoreSession,
    anisette: Option<&str>,
    adam_id: &str,
    missing: Vec<String>,
) -> (Vec<(String, Option<String>, Option<String>)>, Vec<String>) {
    use futures_util::{stream, StreamExt};

    let rows: Vec<VersionRow> = stream::iter(missing)
        .map(|ext| {
            let mut session = session.clone();
            let anisette = anisette.map(str::to_string);
            let adam = adam_id.to_string();
            async move {
                // purchase=true: the metadata-only URL is rejected by the CDN,
                // so the same signed reply has to carry both the label and a
                // readable archive link. One signature instead of two.
                match download_plist(
                    &mut session,
                    anisette.as_deref(),
                    &adam,
                    Some(&ext),
                    true,
                )
                .await
                {
                    Ok(dict) => version_row(ext, &dict),
                    Err(_) => VersionRow::Ready {
                        ext,
                        label: None,
                        url: None,
                    },
                }
            }
        })
        .buffer_unordered(VERSION_LABEL_CONCURRENCY)
        .collect()
        .await;

    let mut ready = Vec::new();
    let mut auth = Vec::new();
    for row in rows {
        match row {
            VersionRow::Ready { ext, label, url } => ready.push((ext, label, url)),
            VersionRow::Auth(ext) => auth.push(ext),
        }
    }
    (ready, auth)
}

fn absorb_version_rows(adam_id: &str, facts: &mut VersionFacts, rows: Vec<(String, Option<String>, Option<String>)>) -> Vec<(String, String)> {
    let mut urls = Vec::new();
    for (ext, label, url) in rows {
        if let Some(display) = label {
            store_version_label(adam_id, &ext, &display);
            facts.labels.insert(ext.clone(), display);
        }
        if let Some(url) = url {
            if !facts.min_ios.contains_key(&ext) {
                urls.push((ext, url));
            }
        }
    }
    urls
}

/// Labels and minimum iOS for a visible page, from one signed request each.
///
/// The old path signed every build twice: once for the name, once for the
/// archive link. A stale session then failed all of them and the picker fell
/// through to a second, slower tool. Here a dead session is refreshed once.
pub async fn version_facts(
    anisette: Option<&str>,
    email: &str,
    adam_id: &str,
    external_ids: &[String],
) -> VersionFacts {
    use futures_util::{stream, StreamExt};

    let mut facts = VersionFacts {
        labels: cached_version_labels(adam_id, external_ids),
        min_ios: cached_version_min_ios(adam_id, external_ids),
    };
    let missing: Vec<String> = external_ids
        .iter()
        .map(|ext| ext.trim().to_string())
        .filter(|ext| !ext.is_empty() && (!facts.labels.contains_key(ext) || !facts.min_ios.contains_key(ext)))
        .collect();
    if missing.is_empty() {
        return facts;
    }

    let base = {
        let _guard = session_lock().lock().await;
        match ensure_session_locked(anisette, email, false).await {
            Ok(session) => {
                persist_session(&session);
                session
            }
            Err(error) => {
                store_debug(&format!("version-facts session failed: {error}"));
                crate::diag_log::note("app", &format!("version list unavailable: {error}"));
                return facts;
            }
        }
    };

    let (ready, auth) = fetch_version_rows(&base, anisette, adam_id, missing).await;
    let mut urls = absorb_version_rows(adam_id, &mut facts, ready);
    if !auth.is_empty() {
        let refreshed = {
            let _guard = session_lock().lock().await;
            ensure_session_locked(anisette, email, true).await.ok()
        };
        if let Some(session) = refreshed {
            persist_session(&session);
            let (ready, _) = fetch_version_rows(&session, anisette, adam_id, auth).await;
            urls.extend(absorb_version_rows(adam_id, &mut facts, ready));
        }
    }

    let read: Vec<(String, String)> = stream::iter(urls)
        .map(|(ext, url)| {
            let adam = adam_id.to_string();
            async move {
                let value = min_ios_from_url(&adam, &ext, &url).await?;
                Some((ext, value))
            }
        })
        .buffer_unordered(MIN_IOS_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect();
    for (ext, value) in read {
        facts.min_ios.insert(ext, value);
    }
    facts
}

async fn buy_app(
    session: &mut StoreSession,
    anisette: Option<&str>,
    adam_id: &str,
) -> Result<(), String> {
    let ani = anisette_headers(anisette)?;
    let body = plist_xml(&[
        ("guid", PlistValue::String(session.guid.clone())),
        ("salableAdamId", PlistValue::String(adam_id.to_string())),
        ("price", PlistValue::String("0".into())),
        ("pricingParameters", PlistValue::String("STDQ".into())),
        ("productType", PlistValue::String("C".into())),
        ("appExtVrsId", PlistValue::String("0".into())),
    ])?;
    let plist = post_plist(
        session,
        BUY_PRODUCT,
        body,
        &ani,
        true,
        "application/x-apple-plist",
        true,
    )
    .await?;
    if let Some(dict) = plist.dict.as_ref() {
        let (fail, msg) = failure_text(dict);
        if is_terms_text(&format!("{fail} {msg}")) {
            return Err(
                "Apple просит подтвердить условия. Подождите минуту и повторите установку.".into(),
            );
        }
        if !fail.is_empty()
            && fail != "0"
            && !msg.to_ascii_lowercase().contains("already")
            && fail != "5002"
        {
            if msg.to_ascii_lowercase().contains("not found") {
                return Ok(());
            }
            if !msg.is_empty() {
                return Err(msg);
            }
        }
    }
    Ok(())
}

pub async fn download_ipa(
    anisette: Option<&str>,
    email: &str,
    adam_id: &str,
    external_version_id: Option<&str>,
    already_owned: bool,
    dest: &Path,
) -> Result<PathBuf, String> {
    let adam_id = adam_id.trim();
    if adam_id.is_empty() || !adam_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Не указан корректный идентификатор App Store".into());
    }
    let _guard = session_lock().lock().await;
    let mut session = ensure_session_locked(anisette, email, false).await?;
    if !already_owned {
        let _ = buy_app(&mut session, anisette, adam_id).await;
    }
    let dict = download_plist(
        &mut session,
        anisette,
        adam_id,
        external_version_id,
        true,
    )
    .await?;
    persist_session(&session);
    let (fail, msg) = failure_text(&dict);
    if is_password_token_failure(&fail, &msg)
        || is_unknown_store_failure(&fail, &msg)
        || is_itunes_signin(&fail, &msg)
    {
        let mut session = ensure_session_locked(anisette, email, true).await?;
        let dict = download_plist(
            &mut session,
            anisette,
            adam_id,
            external_version_id,
            true,
        )
        .await?;
        persist_session(&session);
        return save_song_ipa(&dict, dest).await;
    }
    if !fail.is_empty() && fail != "0" {
        if is_terms_text(&format!("{fail} {msg}")) {
            return Err(
                "Apple просит подтвердить условия. Подождите минуту и повторите установку.".into(),
            );
        }
        return Err(if msg.is_empty() {
            format!("App Store отклонил загрузку ({fail})")
        } else {
            msg
        });
    }
    save_song_ipa(&dict, dest).await
}

async fn save_song_ipa(dict: &Dictionary, dest: &Path) -> Result<PathBuf, String> {
    let song = first_song(dict).ok_or_else(|| "App Store не вернул ссылку на приложение.".to_string())?;
    let url = dict_str(song, "URL").ok_or_else(|| "App Store не вернул ссылку на приложение.".to_string())?;
    let url = if url.starts_with("http://") {
        format!("https://{}", url.trim_start_matches("http://"))
    } else {
        url
    };
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Не удалось создать папку загрузки: {e}"))?;
    }
    let client = cdn_client()?;
    let resp = client
        .get(&url)
        .header("Accept", "*/*")
        .header("Accept-Encoding", "identity")
        .send()
        .await
        .map_err(|_| "Не удалось скачать IPA. Проверьте интернет и повторите.".to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Не удалось скачать IPA ({})",
            resp.status().as_u16()
        ));
    }
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("Не удалось записать IPA: {e}"))?;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "Скачивание IPA оборвалось. Повторите установку.".to_string())?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Не удалось записать IPA: {e}"))?;
    }
    file.flush()
        .await
        .map_err(|e| format!("Не удалось сохранить IPA: {e}"))?;
    let size = tokio::fs::metadata(dest)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if size < 1000 {
        let _ = tokio::fs::remove_file(dest).await;
        return Err("Скачанный файл повреждён".into());
    }
    Ok(dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_label_cache_roundtrip_and_rejects_raw_ids() {
        // store→get within one process
        store_version_label("564177498", "884557660", "8.0.1");
        assert_eq!(
            cached_version_label("564177498", "884557660").as_deref(),
            Some("8.0.1")
        );
        // Display equal to the raw id is never cached (means "no label").
        store_version_label("564177498", "777777777", "777777777");
        assert_eq!(cached_version_label("564177498", "777777777"), None);
        // Empty labels are dropped too.
        store_version_label("564177498", "666666666", "  ");
        assert_eq!(cached_version_label("564177498", "666666666"), None);
        // Different adam/ext pairs never collide.
        assert_eq!(cached_version_label("999999999", "884557660"), None);
    }

    #[test]
    fn extracts_version_ids_from_song_metadata() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>songList</key>
  <array>
    <dict>
      <key>URL</key>
      <string>https://example.invalid/app.ipa</string>
      <key>metadata</key>
      <dict>
        <key>bundleShortVersionString</key>
        <string>5.12.0</string>
        <key>softwareVersionExternalIdentifier</key>
        <integer>900001</integer>
        <key>softwareVersionExternalIdentifiers</key>
        <array>
          <integer>800001</integer>
          <integer>900001</integer>
          <integer>700001</integer>
        </array>
      </dict>
    </dict>
  </array>
</dict>
</plist>"#;
        let dict = parse_plist_dict(xml).expect("plist");
        let versions = versions_from_download(&dict).expect("versions");
        assert_eq!(versions.latest_display.as_deref(), Some("5.12.0"));
        assert_eq!(versions.latest_id.as_deref(), Some("900001"));
        assert_eq!(
            versions.external_ids,
            vec!["900001".to_string(), "800001".to_string(), "700001".to_string()]
        );
    }

    #[test]
    fn empty_song_list_is_not_owned() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>songList</key><array/></dict></plist>"#;
        let dict = parse_plist_dict(xml).expect("plist");
        let versions = versions_from_download(&dict).expect("versions");
        assert!(versions.external_ids.is_empty());
    }
}

// --- Minimum iOS per build -------------------------------------------------
//
// The Store never reports it: the songList metadata carries the marketing
// version, supported device ids and versionRestrictions, but no minimum OS.
// The only source is MinimumOSVersion in that build's own Info.plist, so pull
// just that one entry out of the remote IPA with a handful of range requests
// (~1 MB of a 400 MB file) instead of downloading the package.

/// Largest possible end-of-central-directory record, comment included.
const IPA_TAIL: u64 = 65_557;

/// How many builds are resolved at once. Same ceiling as the label batch: each
/// one costs a SAP signature, and the signers share a cache directory.
const MIN_IOS_CONCURRENCY: usize = 4;

fn le16(buf: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes([*buf.get(at)?, *buf.get(at + 1)?]) as usize)
}

fn le32(buf: &[u8], at: usize) -> Option<u64> {
    Some(u32::from_le_bytes([
        *buf.get(at)?,
        *buf.get(at + 1)?,
        *buf.get(at + 2)?,
        *buf.get(at + 3)?,
    ]) as u64)
}

fn fetch_range(
    client: &reqwest::blocking::Client,
    url: &str,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .header("Range", format!("bytes={start}-{end}"))
        .header("Accept-Encoding", "identity")
        .send()
        .map_err(|e| e.to_string())?;
    if response.status().as_u16() != 206 {
        return Err(format!("CDN ответил {}", response.status().as_u16()));
    }
    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|e| e.to_string())
}

/// Payload/<name>.app/Info.plist — the app's own, not one from a framework.
fn is_main_info_plist(name: &str) -> bool {
    name.starts_with("Payload/")
        && name.ends_with(".app/Info.plist")
        && name.matches('/').count() == 2
}

fn read_min_ios_from_ipa(url: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(CONFIGURATOR_UA)
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let probe = client
        .get(url)
        .header("Range", "bytes=0-0")
        .header("Accept-Encoding", "identity")
        .send()
        .map_err(|e| e.to_string())?;
    if probe.status().as_u16() != 206 {
        return Err(format!("CDN ответил {}", probe.status().as_u16()));
    }
    let size: u64 = probe
        .headers()
        .get("content-range")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit('/').next()?.parse().ok())
        .ok_or_else(|| "CDN не сообщил размер файла".to_string())?;
    if size < 100 {
        return Err("файл пуст".into());
    }

    let tail = fetch_range(&client, url, size.saturating_sub(IPA_TAIL), size - 1)?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&at| tail[at..at + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or_else(|| "не найдено оглавление архива".to_string())?;
    let cd_size = le32(&tail, eocd + 12).unwrap_or(0);
    let cd_offset = le32(&tail, eocd + 16).unwrap_or(0);
    // 0xFFFFFFFF marks a ZIP64 archive; no App Store IPA reaches 4 GB.
    if cd_size == 0 || cd_offset == u32::MAX as u64 || cd_offset + cd_size > size {
        return Err("оглавление архива не разобрать".into());
    }
    let cd = fetch_range(&client, url, cd_offset, cd_offset + cd_size - 1)?;

    let truncated = || "оглавление архива обрезано".to_string();
    let mut at = 0usize;
    while at + 46 <= cd.len() && cd[at..at + 4] == [0x50, 0x4b, 0x01, 0x02] {
        let name_len = le16(&cd, at + 28).ok_or_else(truncated)?;
        let extra_len = le16(&cd, at + 30).ok_or_else(truncated)?;
        let comment_len = le16(&cd, at + 32).ok_or_else(truncated)?;
        let name_end = at + 46 + name_len;
        let name =
            String::from_utf8_lossy(cd.get(at + 46..name_end).ok_or_else(truncated)?).to_string();
        if is_main_info_plist(&name) {
            let method = le16(&cd, at + 10).ok_or_else(truncated)?;
            let packed = le32(&cd, at + 20).ok_or_else(truncated)?;
            let local = le32(&cd, at + 42).ok_or_else(truncated)?;
            return read_min_ios_entry(&client, url, size, local, packed, method);
        }
        at = name_end + extra_len + comment_len;
    }
    Err("Info.plist не найден в архиве".into())
}

fn read_min_ios_entry(
    client: &reqwest::blocking::Client,
    url: &str,
    size: u64,
    local_offset: u64,
    packed: u64,
    method: usize,
) -> Result<String, String> {
    use std::io::Read as _;

    // The local header repeats the name and extra fields with its own lengths,
    // so take a slab wide enough to cover both plus the entry itself.
    let end = (local_offset + 30 + 4096 + packed).min(size) - 1;
    let raw = fetch_range(client, url, local_offset, end)?;
    if raw.len() < 30 || raw[..4] != [0x50, 0x4b, 0x03, 0x04] {
        return Err("запись архива повреждена".into());
    }
    let truncated = || "запись архива обрезана".to_string();
    let start =
        30 + le16(&raw, 26).ok_or_else(truncated)? + le16(&raw, 28).ok_or_else(truncated)?;
    let data = raw
        .get(start..start + packed as usize)
        .ok_or_else(truncated)?;
    let plist_bytes = match method {
        0 => data.to_vec(),
        8 => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| format!("не распаковать Info.plist: {e}"))?;
            out
        }
        other => return Err(format!("неизвестный способ сжатия {other}")),
    };
    let dict: Dictionary =
        plist::from_bytes(&plist_bytes).map_err(|e| format!("не разобрать Info.plist: {e}"))?;
    dict.get("MinimumOSVersion")
        .and_then(plist_to_string)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "в Info.plist нет минимальной системы".to_string())
}

/// Minimum-iOS values share the label cache: same immutable (adamId, build) key
/// space, same file, one less thing to persist and invalidate.
fn min_ios_key(external_version_id: &str) -> String {
    format!("ios:{}", external_version_id.trim())
}

/// Share exact-build requirements obtained by ipatool with the native reader.
pub fn remember_version_min_ios(adam_id: &str, external_version_id: &str, value: &str) {
    store_version_label(adam_id, &min_ios_key(external_version_id), value);
}

/// What is already known, without touching the network.
pub fn cached_version_min_ios(adam_id: &str, external_ids: &[String]) -> HashMap<String, String> {
    let mut known = HashMap::new();
    for ext in external_ids {
        let ext = ext.trim();
        if let Some(value) = cached_version_label(adam_id, &min_ios_key(ext)) {
            known.insert(ext.to_string(), value);
        }
    }
    known
}

async fn version_ipa_url(
    session: &mut StoreSession,
    anisette: Option<&str>,
    adam_id: &str,
    ext: &str,
) -> Option<String> {
    if !session.kbsync.is_empty() {
        let cookies = session.cookies.iter().map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>().join("; ");
        for authed in [false, true] {
            let mut fields = vec![
                ("creditDisplay", PlistValue::String(String::new())),
                ("guid", PlistValue::String(session.guid.clone())),
                ("kbsync", PlistValue::String(session.kbsync.clone())),
                ("salableAdamId", PlistValue::String(adam_id.into())),
                ("serialNumber", PlistValue::String(session.fserial.clone())),
                ("externalVersionId", PlistValue::String(ext.into())),
            ];
            if authed { fields.push(("hasBeenAuthedForBuy", PlistValue::String("true".into()))); }
            let response = store_client().ok()?.post(download_url(session))
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded; charset=utf-8")
                .header("User-Agent", "Configurator/2.18 (Macintosh; OS X 15.3.2; 24D81) AppleWebKit/0620.2.4.11.6")
                .header("iCloud-DSID", &session.ds_person_id)
                .header("X-Dsid", &session.ds_person_id)
                .header("X-Apple-Store-Front", &session.store_front)
                .header("X-Token", &session.password_token)
                .header("Cookie", &cookies)
                .body(plist_xml(&fields).ok()?).send().await.ok()?;
            let status = response.status();
            let bytes = response.bytes().await.ok()?;
            if let Ok(dict) = plist::from_bytes::<Dictionary>(&bytes) {
                if let Some(url) = first_song(&dict).and_then(|song| dict_str(song, "URL")) {
                    return Some(url);
                }
                let (fail, _) = failure_text(&dict);
                store_debug(&format!("min-ios kbsync {adam_id}:{ext} status={status} failure={fail}"));
            }
        }
    }
    let adam: i64 = adam_id.parse().ok()?;
    let body = plist_xml(&[
        ("creditDisplay", PlistValue::String(String::new())),
        ("guid", PlistValue::String(session.guid.clone())),
        ("salableAdamId", PlistValue::Integer(adam.into())),
        ("serialNumber", PlistValue::String("0".into())),
        ("appExtVrsId", PlistValue::String(ext.into())),
    ]).ok()?;
    let cookies = session.cookies.iter().map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>().join("; ");
    let response = store_client().ok()?.post(format!("{UPDATE_PRODUCT}?guid={}", session.guid))
        .header(CONTENT_TYPE, "application/x-apple-plist")
        .header("iCloud-DSID", &session.ds_person_id)
        .header("X-Dsid", &session.ds_person_id)
        .header("Cookie", cookies)
        .body(body).send().await.ok()?;
    if let Ok(bytes) = response.bytes().await {
        if let Ok(dict) = plist::from_bytes::<Dictionary>(&bytes) {
            if let Some(url) = first_song(&dict).and_then(|song| dict_str(song, "URL")) {
                return Some(url);
            }
            let (fail, _) = failure_text(&dict);
            store_debug(&format!("min-ios dispatch {adam_id}:{ext} failure={fail}"));
        }
    }
    // `purchase` must be set: the metadata-only grant returns a URL the CDN
    // answers with 403, so the archive could never be read through it.
    let dict = match download_plist(session, anisette, adam_id, Some(ext), true).await {
        Ok(dict) => dict,
        Err(error) => {
            store_debug(&format!("min-ios metadata {adam_id}:{ext} {error}"));
            return None;
        }
    };
    let (fail, msg) = failure_text(&dict);
    if is_password_token_failure(&fail, &msg)
        || is_unknown_store_failure(&fail, &msg)
        || is_itunes_signin(&fail, &msg)
    {
        *session = ensure_session_locked(anisette, &session.email, true).await.ok()?;
        let dict = download_plist(session, anisette, adam_id, Some(ext), true).await.ok()?;
        return first_song(&dict).and_then(|song| dict_str(song, "URL"));
    }
    let song = first_song(&dict)?;
    dict_str(song, "URL").filter(|url| !url.trim().is_empty())
}

/// Minimum iOS for one build. Cached forever, since it can never change.
/// Минимальная iOS по готовой ссылке на файл.
///
/// Тот же диапазонный дочитыватель `Info.plist`, что и в [`version_min_ios`],
/// но ссылку приносит вызывающий. Нужно, когда нативная сессия мертва и URL
/// добыт через ipatool: без этого у свежих сборок минимальная система
/// оставалась пустой, хотя сам файл прекрасно скачивался.
pub async fn min_ios_from_url(adam_id: &str, external_version_id: &str, url: &str) -> Option<String> {
    let ext = external_version_id.trim();
    let url = url.trim().to_string();
    if ext.is_empty() || url.is_empty() {
        return None;
    }
    if let Some(known) = cached_version_label(adam_id, &min_ios_key(ext)) {
        return Some(known);
    }
    let value = match tokio::task::spawn_blocking(move || read_min_ios_from_ipa(&url)).await {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            store_debug(&format!("min-ios(url) {adam_id}:{ext} {error}"));
            return None;
        }
        Err(_) => return None,
    };
    store_version_label(adam_id, &min_ios_key(ext), &value);
    Some(value)
}

pub async fn version_min_ios(
    anisette: Option<&str>,
    email: &str,
    adam_id: &str,
    external_version_id: &str,
) -> Option<String> {
    let ext = external_version_id.trim();
    if ext.is_empty() {
        return None;
    }
    if let Some(known) = cached_version_label(adam_id, &min_ios_key(ext)) {
        return Some(known);
    }
    let url = {
        let _guard = session_lock().lock().await;
        let mut session = ensure_session_locked(anisette, email, false).await.ok()?;
        let url = version_ipa_url(&mut session, anisette, adam_id, ext).await?;
        persist_session(&session);
        url
    };
    let value = match tokio::task::spawn_blocking(move || read_min_ios_from_ipa(&url)).await {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            store_debug(&format!("min-ios {adam_id}:{ext} {error}"));
            return None;
        }
        Err(_) => return None,
    };
    store_version_label(adam_id, &min_ios_key(ext), &value);
    Some(value)
}

/// Resolve a whole page at once, the way [`version_displays`] does: the session
/// is established once under the lock and then cloned, so the requests overlap
/// instead of queueing behind it.
pub async fn version_min_ios_batch(
    anisette: Option<&str>,
    email: &str,
    adam_id: &str,
    external_ids: &[String],
    on_ready: impl Fn(&str, &str) + Send + Sync,
) -> HashMap<String, String> {
    use futures_util::{stream, StreamExt};

    let mut known = cached_version_min_ios(adam_id, external_ids);
    for (ext, value) in &known {
        on_ready(ext, value);
    }
    let missing: Vec<String> = external_ids
        .iter()
        .map(|ext| ext.trim().to_string())
        .filter(|ext| !ext.is_empty() && !known.contains_key(ext))
        .collect();
    if missing.is_empty() {
        return known;
    }

    let base = {
        let _guard = session_lock().lock().await;
        match ensure_session_locked(anisette, email, false).await {
            Ok(session) => {
                persist_session(&session);
                session
            }
            Err(error) => {
                store_debug(&format!("min-ios session failed: {error}"));
                return known;
            }
        }
    };

    let fetched: Vec<Option<(String, String)>> = stream::iter(missing)
        .map(|ext| {
            let mut session = base.clone();
            let anisette = anisette.map(str::to_string);
            let adam = adam_id.to_string();
            let on_ready = &on_ready;
            async move {
                let url = version_ipa_url(&mut session, anisette.as_deref(), &adam, &ext).await?;
                let value = min_ios_from_url(&adam, &ext, &url).await?;
                store_version_label(adam_id, &min_ios_key(&ext), &value);
                on_ready(&ext, &value);
                Some((ext, value))
            }
        })
        .buffer_unordered(MIN_IOS_CONCURRENCY)
        .collect()
        .await;

    for (ext, value) in fetched.into_iter().flatten() {
        store_version_label(adam_id, &min_ios_key(&ext), &value);
        known.insert(ext, value);
    }
    known
}

#[cfg(test)]
mod min_ios_tests {
    use super::*;

    #[test]
    fn hardware_identity_rejects_placeholder_and_multicast_addresses() {
        assert_eq!(valid_hardware_guid("10:22:33:44:55:66"), Some("102233445566".into()));
        for mac in ["", "000000000000", "FFFFFFFFFFFF", "020000000000", "AABBCCDDEEFF", "112233445566"] {
            assert!(valid_hardware_guid(mac).is_none(), "{mac}");
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "live Apple ID request; run explicitly with --ignored --nocapture"]
    async fn live_min_ios_without_cache() {
        let passphrase = crate::read_passphrase_from_file().expect("saved keychain key");
        let session = fs::read_dir(session_dir().unwrap()).unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read(entry.path()).ok())
            .filter_map(|raw| serde_json::from_slice::<StoreSession>(&raw).ok())
            .find_map(|session| load_ipatool_session(&session.email, &passphrase))
            .expect("active ipatool session");
        let labels = load_version_labels_from_disk();
        let mut builds: Vec<_> = labels.keys().filter_map(|key| {
            let (adam, ext) = key.split_once(':')?;
            ((adam == "6805045741" && !ext.contains(':')) || ext == "884253731")
                .then_some((adam.to_string(), ext.to_string()))
        }).collect();
        builds.sort();
        builds.dedup();
        assert!(!builds.is_empty(), "no requested builds in local version list");
        let home = std::env::var("LOCALAPPDATA").unwrap();
        let helper = PathBuf::from(home).join("Signum/resources/apple-tools/windows-x64/v4/anisette.exe");
        let helper = helper.to_string_lossy();
        let mut failures = Vec::new();
        for (adam, ext) in builds {
            let mut session = session.clone();
            let Some(url) = version_ipa_url(&mut session, Some(&helper), &adam, &ext).await else {
                println!("{adam}:{ext}: no IPA URL (cache bypassed)");
                failures.push(format!("{adam}:{ext}"));
                continue;
            };
            let ios = tokio::task::spawn_blocking(move || read_min_ios_from_ipa(&url)).await.unwrap()
                .unwrap_or_else(|error| panic!("{adam}:{ext}: {error}"));
            println!("{adam}:{ext}: iOS {ios} (cache bypassed)");
        }
        assert!(failures.is_empty(), "No IPA URL: {}", failures.join(", "));
    }

    #[test]
    fn ipatool_account_requires_correct_device_key_and_passphrase() {
        use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
        let salt = [1u8; 16];
        let nonce = [2u8; 12];
        let mut key = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(b"devicenice_key_is_nicepass", &salt, 100_000, &mut key);
        let encrypted = Aes256Gcm::new_from_slice(&key).unwrap()
            .encrypt(Nonce::from_slice(&nonce), br#"{"email":"test@example.com"}"#.as_ref()).unwrap();
        let mut raw = vec![2];
        raw.extend_from_slice(&16u32.to_le_bytes());
        raw.extend_from_slice(&salt);
        raw.extend_from_slice(&nonce);
        raw.extend_from_slice(&encrypted[encrypted.len() - 16..]);
        raw.extend_from_slice(&encrypted[..encrypted.len() - 16]);
        assert_eq!(decrypt_ipatool_account(&raw, "device", "pass").unwrap()["email"], "test@example.com");
        assert!(decrypt_ipatool_account(&raw, "other-device", "pass").is_none());
        assert!(decrypt_ipatool_account(&raw, "device", "wrong-pass").is_none());
        assert!(decrypt_ipatool_account(&raw[..20], "device", "pass").is_none());
    }

    #[tokio::test]
    async fn cached_ios_is_sent_without_authentication() {
        let adam = "test-cached-ios-channel";
        let ext = "test-build";
        store_version_label(adam, &min_ios_key(ext), "15.0");
        let updates = Mutex::new(Vec::new());
        let result = version_min_ios_batch(None, "", adam, &[ext.into()], |id, value| {
            updates.lock().unwrap().push((id.to_string(), value.to_string()));
        }).await;
        assert_eq!(result.get(ext).map(String::as_str), Some("15.0"));
        assert_eq!(*updates.lock().unwrap(), vec![(ext.into(), "15.0".into())]);
    }

    #[test]
    fn only_the_apps_own_info_plist_matches() {
        assert!(is_main_info_plist("Payload/VKClient.app/Info.plist"));
        // A framework or a localisation carries its own Info.plist; picking one
        // of those would report the wrong minimum iOS.
        assert!(!is_main_info_plist(
            "Payload/VKClient.app/Frameworks/Shared.framework/Info.plist"
        ));
        assert!(!is_main_info_plist("Payload/VKClient.app/Base.lproj/Info.plist"));
        assert!(!is_main_info_plist("Info.plist"));
    }

    #[test]
    fn little_endian_readers_refuse_to_run_off_the_end() {
        let buf = [1u8, 0, 2, 0];
        assert_eq!(le16(&buf, 0), Some(1));
        assert_eq!(le32(&buf, 0), Some(0x0002_0001));
        assert_eq!(le16(&buf, 3), None);
        assert_eq!(le32(&buf, 1), None);
    }
}
