                                                                                  
   
                                                                      
                                                          

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const UA_ROCKET: &str = "Mozilla/5.0 (Linux; Android 13; SM-G981B) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/148.0.0.0 Mobile Safari/537.36";
const UA_SOVCOM: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BankKey {
    Sovcombank,
    Rocketbank,
}

impl BankKey {
    pub fn parse(raw: &str) -> Option<Self> {
        let k = raw.trim().to_lowercase();
        match k.as_str() {
            "sovcombank" | "sovcom" | "совком" | "совкомбанк" | "халва" => {
                Some(Self::Sovcombank)
            }
            "rocketbank" | "rocket" | "рокет" | "рокете" | "рокетэ" | "рокетбанк" => {
                Some(Self::Rocketbank)
            }
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sovcombank => "sovcombank",
            Self::Rocketbank => "rocketbank",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Sovcombank => "Совкомбанк",
            Self::Rocketbank => "Рокетбанк",
        }
    }

    pub fn source(self) -> &'static str {
        match self {
            Self::Sovcombank => "manual",
            Self::Rocketbank => "rocket",
        }
    }

                                                          
    pub fn search_terms(self) -> &'static [&'static str] {
        match self {
            Self::Sovcombank => &["Совкомбанк", "Sovcombank", "Халва"],
            Self::Rocketbank => &["Рокетбанк", "Rocketbank", "Rocket Bank"],
        }
    }
}

#[derive(Debug, Clone)]
struct HostTriple {
    send_sms: String,
    validate: String,
    credentials: String,
}

#[derive(Debug, Clone)]
struct BankConfig {
    key: BankKey,
    origin: String,
    referer_send: String,
    referer_validate: String,
    referer_creds: String,
    hosts: Vec<HostTriple>,
}

fn bank_config(key: BankKey) -> BankConfig {
    match key {
        BankKey::Sovcombank => BankConfig {
            key,
            origin: "https://manual.sovcombank.ru".into(),
            referer_send: "https://manual.sovcombank.ru/manual".into(),
            referer_validate: "https://manual.sovcombank.ru/manual/verify".into(),
            referer_creds: "https://manual.sovcombank.ru/manual/manual".into(),
            hosts: vec![HostTriple {
                send_sms: "https://manual.sovcombank.ru/rest/auth/phone/sendsmscode".into(),
                validate: "https://manual.sovcombank.ru/rest/auth/phone/validate".into(),
                credentials: "https://manual.sovcombank.ru/rest/api/v2/constants/auth/iphone/installation/credentials".into(),
            }],
        },
        BankKey::Rocketbank => BankConfig {
            key,
            origin: "https://rocketbank.ru".into(),
            referer_send: "https://rocketbank.ru/ios/manual".into(),
            referer_validate: "https://rocketbank.ru/ios/manual/verify".into(),
            referer_creds: "https://rocketbank.ru/ios/manual/verify".into(),
            hosts: vec![
                HostTriple {
                    send_sms: "https://api-app.sovcombank.ru/v1/phone/sendsmscode".into(),
                    validate: "https://api-app.sovcombank.ru/v1/phone/validate".into(),
                    credentials: "https://api-app.sovcombank.ru/v2/constants/auth/iphone/installation/credentials?soft=rocket".into(),
                },
                HostTriple {
                    send_sms: "https://rocketbank.ru/ios/manual/rest/auth/phone/sendsmscode".into(),
                    validate: "https://rocketbank.ru/ios/manual/rest/auth/phone/validate".into(),
                    credentials: "https://rocketbank.ru/ios/manual/rest/api/v2/constants/auth/iphone/installation/credentials?soft=rocket".into(),
                },
            ],
        },
    }
}

#[derive(Debug, Clone)]
pub struct BankOtpSession {
    pub bank: BankKey,
    pub phone: String,
    pub created: Instant,
    pub sms_token: Option<String>,
    pub auth_token: Option<String>,
    pub token_with_data: Option<String>,
    active_host: Option<HostTriple>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankCredentials {
    pub login: String,
    pub password: String,
    pub link: Option<String>,
    pub file_name: Option<String>,
    pub terminal_id: Option<String>,
                                                                            
    pub app_store_id: Option<String>,
    pub bundle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankSendSmsResult {
    pub session_id: String,
    pub phone_display: String,
    pub bank: String,
    pub bank_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankConfirmResult {
    pub credentials: BankCredentials,
    pub bank: String,
    pub bank_name: String,
}

                                                          
pub const BANK_CREDS_TTL_SECS: u64 = 30 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedBankCreds {
    pub bank: String,
    pub bank_name: String,
    pub login: String,
    pub password: String,
    pub link: String,
                                                    
    pub obtained_at: u64,
}

impl CachedBankCreds {
    pub fn remaining_secs(&self) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let expires = self.obtained_at.saturating_add(BANK_CREDS_TTL_SECS);
        expires as i64 - now as i64
    }

    pub fn is_active(&self) -> bool {
        self.remaining_secs() > 0
            && !self.login.trim().is_empty()
            && !self.password.trim().is_empty()
            && !self.link.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BankCredsCachePeek {
    pub active: bool,
    pub bank: String,
    pub bank_name: String,
    pub login: String,
    pub password: String,
    pub remaining_secs: i64,
    pub expires_in_label: String,
}

pub struct BankOtpStore {
    sessions: Mutex<HashMap<String, BankOtpSession>>,
                                                                         
    creds_cache: Mutex<HashMap<String, CachedBankCreds>>,
}

impl Default for BankOtpStore {
    fn default() -> Self {
        let store = Self {
            sessions: Mutex::new(HashMap::new()),
            creds_cache: Mutex::new(HashMap::new()),
        };
        store.load_disk_cache();
        store
    }
}

impl BankOtpStore {
    pub fn put(&self, id: String, session: BankOtpSession) {
        let mut map = self.sessions.lock().unwrap();
                                                  
        map.retain(|_, s| s.created.elapsed() < Duration::from_secs(15 * 60));
        map.insert(id, session);
    }

    pub fn get_mut_clone(&self, id: &str) -> Option<BankOtpSession> {
        self.sessions.lock().unwrap().get(id).cloned()
    }

    pub fn update(&self, id: &str, session: BankOtpSession) {
        self.sessions
            .lock()
            .unwrap()
            .insert(id.to_string(), session);
    }

    pub fn take(&self, id: &str) -> Option<BankOtpSession> {
        self.sessions.lock().unwrap().remove(id)
    }

    pub fn save_creds(&self, bank: BankKey, login: &str, password: &str, link: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = CachedBankCreds {
            bank: bank.as_str().into(),
            bank_name: bank.display_name().into(),
            login: login.trim().to_string(),
            password: password.to_string(),
            link: link.trim().to_string(),
            obtained_at: now,
        };
        {
            let mut map = self.creds_cache.lock().unwrap();
            map.retain(|_, c| c.is_active());
            map.insert(bank.as_str().to_string(), entry);
        }
        self.persist_disk_cache();
    }

    pub fn peek_creds(&self, bank: BankKey) -> Option<BankCredsCachePeek> {
        self.prune_creds();
        let map = self.creds_cache.lock().unwrap();
        let c = map.get(bank.as_str())?;
        if !c.is_active() {
            return None;
        }
        let rem = c.remaining_secs().max(0);
        Some(BankCredsCachePeek {
            active: true,
            bank: c.bank.clone(),
            bank_name: c.bank_name.clone(),
            login: c.login.clone(),
            password: c.password.clone(),
            remaining_secs: rem,
            expires_in_label: format_remaining_label(rem),
        })
    }

    pub fn get_active_creds(&self, bank: BankKey) -> Option<CachedBankCreds> {
        self.prune_creds();
        let map = self.creds_cache.lock().unwrap();
        map.get(bank.as_str()).filter(|c| c.is_active()).cloned()
    }

    pub fn clear_creds(&self, bank: BankKey) {
        {
            let mut map = self.creds_cache.lock().unwrap();
            map.remove(bank.as_str());
        }
        self.persist_disk_cache();
    }

    fn prune_creds(&self) {
        let mut map = self.creds_cache.lock().unwrap();
        let before = map.len();
        map.retain(|_, c| c.is_active());
        if map.len() != before {
            drop(map);
            self.persist_disk_cache();
        }
    }

    fn cache_path() -> Option<std::path::PathBuf> {
        let base = std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("HOME"))
            .ok()?;
        let dir = std::path::PathBuf::from(base).join("Signum");
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir.join("bank-alt-creds.json"))
    }

    fn load_disk_cache(&self) {
        let Some(path) = Self::cache_path() else {
            return;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(list) = serde_json::from_str::<Vec<CachedBankCreds>>(&raw) else {
            return;
        };
        let mut map = self.creds_cache.lock().unwrap();
        for c in list {
            if c.is_active() {
                map.insert(c.bank.clone(), c);
            }
        }
    }

    fn persist_disk_cache(&self) {
        let Some(path) = Self::cache_path() else {
            return;
        };
        let map = self.creds_cache.lock().unwrap();
        let list: Vec<_> = map.values().filter(|c| c.is_active()).cloned().collect();
        if list.is_empty() {
            let _ = std::fs::remove_file(&path);
            return;
        }
        if let Ok(raw) = serde_json::to_string_pretty(&list) {
            let _ = std::fs::write(path, raw);
        }
    }
}

fn format_remaining_label(secs: i64) -> String {
    if secs <= 0 {
        return "истекла".into();
    }
    let m = secs / 60;
    let s = secs % 60;
    if m >= 1 {
        format!("ещё {m} мин")
    } else {
        format!("ещё {s} сек")
    }
}

pub fn normalize_phone(raw: &str) -> Result<String, String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Err("Введите номер телефона".into());
    }
    let digits = if digits.len() == 10 && digits.starts_with('9') {
        format!("7{digits}")
    } else if digits.len() == 11 && digits.starts_with('8') {
        format!("7{}", &digits[1..])
    } else {
        digits
    };
    if digits.len() != 11 || !digits.starts_with('7') {
        return Err("Нужен номер РФ: +7 и 10 цифр (пример +79991234567)".into());
    }
    Ok(digits)
}

pub fn format_phone_display(phone_api: &str) -> String {
    let d: String = phone_api.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() == 11 && d.starts_with('7') {
                          
        format!("+7 {} {} {}", &d[1..4], &d[4..7], &d[7..11])
    } else {
        phone_api.to_string()
    }
}

fn ua(bank: BankKey) -> &'static str {
    match bank {
        BankKey::Rocketbank => UA_ROCKET,
        BankKey::Sovcombank => UA_SOVCOM,
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(35))
        .user_agent(UA_SOVCOM)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))
}

fn friendly_sms_error(status: u16, raw: &str, msg: &str) -> String {
    let low = format!("{msg} {raw}").to_lowercase();
    if low.contains("not verified") {
        return "API отклонил номер (not verified). Подождите 10–15 мин или попробуйте другой банк/номер.".into();
    }
    if low.contains("too many") || status == 429 {
        return "Слишком много SMS. Подождите 10–15 минут.".into();
    }
    if low.contains("qauth")
        || low.contains("qrator")
        || ((status == 401 || status == 403) && low.contains("<html"))
    {
        return format!(
            "HTTP {status}: сайт банка закрыт защитой с вашего IP. Попробуйте api-host (Рокет) или другую сеть."
        );
    }
    if low.contains("access blocked") {
        return "Access Blocked — IP временно заблокирован. Подождите.".into();
    }
    if !msg.is_empty() {
        return msg.to_string();
    }
    if !raw.is_empty() {
        return format!(
            "SMS HTTP {status}: {}",
            raw.chars().take(180).collect::<String>()
        );
    }
    format!("SMS HTTP {status}")
}

async fn request_json(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    body: Option<&JsonValue>,
    headers: &[(&str, &str)],
    user_agent: &str,
) -> Result<(u16, Option<JsonValue>, String), String> {
    let mut req = client
        .request(method.clone(), url)
        .header("Accept", "application/json")
        .header("Accept-Language", "ru,en;q=0.9")
        .header("User-Agent", user_agent);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    if method != reqwest::Method::GET {
        req = req.header("Content-Type", "application/json");
        if let Some(b) = body {
            req = req.json(b);
        }
    } else {
                                                                        
        req = req.header("Content-Type", "application/json");
    }

    let resp = req.send().await.map_err(|e| format!("Сеть: {e}"))?;
    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    let parsed = if raw.trim().is_empty() {
        None
    } else {
        serde_json::from_str(&raw).ok()
    };
    Ok((status, parsed, raw))
}

                                                                    
pub async fn send_sms(bank: BankKey, phone_raw: &str) -> Result<(BankOtpSession, String), String> {
    let phone = normalize_phone(phone_raw)?;
    let cfg = bank_config(bank);
    let client = http_client()?;
    let body = serde_json::json!({
        "phone": phone,
        "source": bank.source(),
    });
    let mut last_status = 0u16;
    let mut last_raw = String::new();
    let mut last_msg = String::new();

    for host in &cfg.hosts {
        let headers = [
            ("Origin", cfg.origin.as_str()),
            ("Referer", cfg.referer_send.as_str()),
        ];
        let (status, data, raw) = request_json(
            &client,
            reqwest::Method::POST,
            &host.send_sms,
            Some(&body),
            &headers,
            ua(bank),
        )
        .await?;
        last_status = status;
        last_raw = raw.clone();
        let msg = data
            .as_ref()
            .and_then(|d| {
                d.get("message")
                    .or_else(|| d.get("error"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        last_msg = msg.clone();

        if (200..300).contains(&status) {
            if let Some(token) = data
                .as_ref()
                .and_then(|d| d.get("token"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let session = BankOtpSession {
                    bank,
                    phone: phone.clone(),
                    created: Instant::now(),
                    sms_token: Some(token.to_string()),
                    auth_token: None,
                    token_with_data: None,
                    active_host: Some(host.clone()),
                };
                let display_msg = data
                    .as_ref()
                    .and_then(|d| d.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("ok")
                    .to_string();
                return Ok((session, display_msg));
            }
        }

        let soft = status == 401
            || status == 403
            || status == 429
            || msg.to_lowercase().contains("not verified")
            || raw.to_lowercase().contains("qauth")
            || raw.to_lowercase().contains("qrator")
            || msg.to_lowercase().contains("too many")
            || raw.to_lowercase().contains("access blocked");
        if soft {
            continue;
        }
        break;
    }

    Err(friendly_sms_error(last_status, &last_raw, &last_msg))
}

pub async fn validate_otp(session: &mut BankOtpSession, code_raw: &str) -> Result<String, String> {
    let token = session
        .sms_token
        .clone()
        .ok_or_else(|| "Нет SMS token — сначала отправьте код".to_string())?;
    let host = session
        .active_host
        .clone()
        .ok_or_else(|| "Сессия OTP повреждена".to_string())?;
    let cfg = bank_config(session.bank);
    let client = http_client()?;
    let code: String = code_raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if code.len() < 4 {
        return Err("Введите код из SMS (4+ цифр)".into());
    }

    let body = match session.bank {
        BankKey::Sovcombank => serde_json::json!({
            "code": code,
            "token": token,
            "phone": session.phone,
        }),
        BankKey::Rocketbank => serde_json::json!({
            "code": code,
            "token": token,
            "source": session.bank.source(),
        }),
    };

    let auth_owned = token.clone();
    let headers_owned: Vec<(String, String)> = match session.bank {
        BankKey::Sovcombank => vec![
            ("Origin".into(), cfg.origin.clone()),
            ("Referer".into(), cfg.referer_validate.clone()),
        ],
        BankKey::Rocketbank => vec![
            ("Origin".into(), cfg.origin.clone()),
            ("Referer".into(), cfg.referer_validate.clone()),
            ("authorization".into(), auth_owned),
        ],
    };
    let header_refs: Vec<(&str, &str)> = headers_owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let (status, data, raw) = request_json(
        &client,
        reqwest::Method::POST,
        &host.validate,
        Some(&body),
        &header_refs,
        ua(session.bank),
    )
    .await?;

    if !(200..300).contains(&status) {
        let msg = data
            .as_ref()
            .and_then(|d| {
                d.get("message")
                    .or_else(|| d.get("error"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("");
        return Err(if msg.is_empty() {
            format!(
                "Код не принят (HTTP {status}): {}",
                raw.chars().take(200).collect::<String>()
            )
        } else {
            msg.to_string()
        });
    }

    let mut twd: Option<String> = None;
    if let Some(d) = data.as_ref() {
        for k in ["tokenWithData", "token", "accessToken"] {
            if let Some(v) = d
                .get(k)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                twd = Some(v.to_string());
                if k == "tokenWithData" {
                    break;
                }
            }
        }
    }
    session.token_with_data = twd.clone();
    match session.bank {
        BankKey::Rocketbank => {
                                                         
            session.auth_token = Some(token.clone());
        }
        BankKey::Sovcombank => {
            session.auth_token = twd.or(Some(token));
        }
    }
    Ok(session.auth_token.clone().unwrap_or_default())
}

pub async fn get_credentials(session: &BankOtpSession) -> Result<BankCredentials, String> {
    let host = session
        .active_host
        .as_ref()
        .ok_or_else(|| "Сессия OTP повреждена".to_string())?;
    let cfg = bank_config(session.bank);
    let client = http_client()?;

    let mut candidates: Vec<String> = Vec::new();
    let push = |list: &mut Vec<String>, t: &Option<String>| {
        if let Some(s) = t.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if !list.iter().any(|x| x == s) {
                list.push(s.to_string());
            }
        }
    };
    match session.bank {
        BankKey::Rocketbank => {
            push(&mut candidates, &session.sms_token);
            push(&mut candidates, &session.auth_token);
            push(&mut candidates, &session.token_with_data);
        }
        BankKey::Sovcombank => {
            push(&mut candidates, &session.auth_token);
            push(&mut candidates, &session.token_with_data);
            push(&mut candidates, &session.sms_token);
        }
    }
    if candidates.is_empty() {
        return Err("Нет auth token после SMS".into());
    }

    let mut last_msg = String::new();
    for token in &candidates {
        for attempt in 0..3 {
            let headers = [
                ("authorization", token.as_str()),
                ("Origin", cfg.origin.as_str()),
                ("Referer", cfg.referer_creds.as_str()),
            ];
            let (status, data, raw) = request_json(
                &client,
                reqwest::Method::GET,
                &host.credentials,
                None,
                &headers,
                ua(session.bank),
            )
            .await?;
            if status == 200 {
                if let Some(d) = data.as_ref() {
                    let login = d.get("login").and_then(|v| v.as_str()).unwrap_or("").trim();
                    let password = d
                        .get("password")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    if !login.is_empty() && !password.is_empty() {
                        let link = d
                            .get("link")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let app_store_id = extract_adam_from_value(d)
                            .or_else(|| link.as_deref().and_then(adam_id_from_link));
                        let bundle_id = d
                            .get("bundleId")
                            .or_else(|| d.get("bundle_id"))
                            .or_else(|| d.get("bundleIdentifier"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty());
                        return Ok(BankCredentials {
                            login: login.to_string(),
                            password: password.to_string(),
                            link,
                            file_name: d
                                .get("fileName")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            terminal_id: d.get("terminalId").and_then(|v| {
                                v.as_str()
                                    .map(|s| s.to_string())
                                    .or_else(|| v.as_i64().map(|n| n.to_string()))
                            }),
                            app_store_id,
                            bundle_id,
                        });
                    }
                    last_msg = "login/password отсутствуют в ответе банка".into();
                }
            } else {
                let msg = data
                    .as_ref()
                    .and_then(|d| {
                        d.get("message")
                            .or_else(|| d.get("error"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("");
                last_msg = if msg.is_empty() {
                    format!(
                        "credentials HTTP {status}: {}",
                        raw.chars().take(180).collect::<String>()
                    )
                } else {
                    msg.to_string()
                };
                if status == 401 {
                    break;
                }
            }
            if attempt < 2 {
                tokio::time::sleep(Duration::from_millis(700)).await;
            }
        }
    }
    Err(last_msg.if_empty("Не удалось получить Apple ID от банка"))
}

trait IfEmpty {
    fn if_empty(self, alt: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, alt: &str) -> String {
        if self.trim().is_empty() {
            alt.to_string()
        } else {
            self
        }
    }
}

fn extract_adam_from_value(d: &JsonValue) -> Option<String> {
                                                                          
    for key in [
        "appId",
        "app_id",
        "adamId",
        "adam_id",
        "trackId",
        "track_id",
        "applicationId",
        "storeId",
        "itunesId",
        "appleId",
    ] {
        if let Some(v) = d.get(key) {
            if let Some(n) = v.as_u64() {
                let s = n.to_string();
                if s.len() >= 8 {
                    return Some(s);
                }
            }
            if let Some(s) = v
                .as_str()
                .map(str::trim)
                .filter(|s| s.len() >= 8 && s.chars().all(|c| c.is_ascii_digit()))
            {
                return Some(s.to_string());
            }
        }
    }
    None
}

                                                               
pub fn history_app_matches_bank(bank: BankKey, name: &str, artist: &str) -> bool {
    let hay = format!("{} {}", name, artist)
        .to_lowercase()
        .replace('ё', "е");
    match bank {
        BankKey::Sovcombank => {
            hay.contains("совком")
                || hay.contains("sovcom")
                || hay.contains("халва")
                || hay.contains("halva")
                || hay.contains("skb")
        }
        BankKey::Rocketbank => {
            hay.contains("рокет") || hay.contains("rocket") || hay.contains("roket")
        }
    }
}

                                                                                       
                                      
pub async fn resolve_bank_app_store_id(bank: BankKey) -> Result<(String, String, String), String> {
    let client = http_client()?;
    for country in ["ru", "us", "kz", "by"] {
        for term in bank.search_terms() {
            let url = format!(
                "https://itunes.apple.com/search?term={}&country={country}&entity=software&limit=12",
                urlencoding_lite(term)
            );
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("iTunes search: {e}"))?;
            let raw = resp.text().await.unwrap_or_default();
            let parsed: JsonValue = serde_json::from_str(&raw).unwrap_or(JsonValue::Null);
            let results = parsed
                .get("results")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for item in results {
                let name = item
                    .get("trackName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let seller = item
                    .get("sellerName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let bundle = item
                    .get("bundleId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let adam = item
                    .get("trackId")
                    .and_then(|v| {
                        v.as_u64()
                            .map(|n| n.to_string())
                            .or_else(|| v.as_str().map(|s| s.to_string()))
                    })
                    .unwrap_or_default();
                if adam.is_empty() {
                    continue;
                }
                if history_app_matches_bank(bank, &name, &seller)
                    || history_app_matches_bank(bank, &bundle, "")
                {
                    let title = item
                        .get("trackName")
                        .and_then(|v| v.as_str())
                        .unwrap_or(bank.display_name())
                        .to_string();
                    let bundle = if bundle.is_empty() {
                        format!("id.{adam}")
                    } else {
                        bundle
                    };
                    return Ok((adam, bundle, title));
                }
            }
        }
    }
    Err(format!(
        "{} сейчас нет в публичном App Store. Signum возьмёт приложение из покупок Apple ID банка.",
        bank.display_name()
    ))
}

                                                        
pub fn adam_id_from_link(link: &str) -> Option<String> {
    let patterns = [
        r"(?:id|/app/id|/id)(\d{8,})",
        r"[?&](?:id|adamId|appId|i)=(\d{8,})",
        r"\b(\d{9,12})\b",
    ];
    for p in patterns {
        if let Ok(re) = regex::Regex::new(p) {
            if let Some(c) = re.captures(link) {
                if let Some(m) = c.get(1) {
                    let s = m.as_str();
                    if s.len() >= 8 && s.chars().all(|ch| ch.is_ascii_digit()) {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }
    None
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

                                                                             
pub fn detect_bank_from_text(text: &str) -> Option<BankKey> {
    let n = text.to_lowercase().replace('ё', "е");
    if n.contains("рокет") || n.contains("rocket") {
        return Some(BankKey::Rocketbank);
    }
    if n.contains("совком") || n.contains("sovcom") || n.contains("халва") || n.contains("halva")
    {
        return Some(BankKey::Sovcombank);
    }
    None
}

                                                     
#[derive(Debug, Clone)]
pub struct BankManifestIpa {
    pub ipa_url: String,
    pub title: Option<String>,
    pub bundle_id: Option<String>,
    pub version: Option<String>,
    pub plist_url: String,
}

                                                                                  
pub fn resolve_plist_url(link: &str) -> Result<String, String> {
    let link = link.trim();
    if link.is_empty() {
        return Err("В ответе банка нет ссылки на установку".into());
    }
                                                                      
    if link.to_ascii_lowercase().starts_with("itms-services:") {
        if let Some(q) = link.split('?').nth(1) {
            for part in q.split('&') {
                let mut kv = part.splitn(2, '=');
                let k = kv.next().unwrap_or("");
                let v = kv.next().unwrap_or("");
                if k.eq_ignore_ascii_case("url") {
                    let decoded = urlencoding_decode(v);
                    if decoded.starts_with("http://") || decoded.starts_with("https://") {
                        return Ok(decoded);
                    }
                }
            }
        }
        return Err("Не удалось разобрать itms-services ссылку банка".into());
    }
    if link.starts_with("http://") || link.starts_with("https://") {
        return Ok(link.to_string());
    }
                              
    if link.starts_with("//") {
        return Ok(format!("https:{link}"));
    }
    Err("Ссылка банка не похожа на http(s) / itms-services".into())
}

fn urlencoding_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = |c: u8| -> Option<u8> {
                    match c {
                        b'0'..=b'9' => Some(c - b'0'),
                        b'a'..=b'f' => Some(c - b'a' + 10),
                        b'A'..=b'F' => Some(c - b'A' + 10),
                        _ => None,
                    }
                };
                if let (Some(a), Some(b)) = (h(bytes[i + 1]), h(bytes[i + 2])) {
                    out.push((a << 4) | b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

                                                                 
pub async fn fetch_manifest_ipa(link: &str) -> Result<BankManifestIpa, String> {
    let plist_url = resolve_plist_url(link)?;
    let client = http_client()?;
    let resp = client
        .get(&plist_url)
        .header(
            "Accept",
            "application/xml, text/xml, application/x-plist, */*",
        )
        .header("User-Agent", UA_SOVCOM)
        .send()
        .await
        .map_err(|e| format!("Не удалось скачать .plist: {e}"))?;
    let status = resp.status().as_u16();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Не удалось прочитать .plist: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "Банк вернул HTTP {status} при загрузке .plist ({}…)",
            plist_url.chars().take(80).collect::<String>()
        ));
    }
                               
    let ct_guess = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]).to_lowercase();
    if plist_url.to_ascii_lowercase().ends_with(".ipa")
        || ct_guess.starts_with("pk\x03\x04")
        || (bytes.len() > 4 && &bytes[0..2] == b"PK")
    {
        return Ok(BankManifestIpa {
            ipa_url: plist_url.clone(),
            title: None,
            bundle_id: None,
            version: None,
            plist_url,
        });
    }

    parse_install_plist(&bytes, &plist_url)
}

fn parse_install_plist(bytes: &[u8], plist_url: &str) -> Result<BankManifestIpa, String> {
    use std::io::Cursor;
    let value = plist::Value::from_reader(Cursor::new(bytes))
        .or_else(|_| {
                                                             
            let text = String::from_utf8_lossy(bytes);
            let start = text.find('<').unwrap_or(0);
            plist::Value::from_reader(Cursor::new(text[start..].as_bytes()))
        })
        .map_err(|e| format!("Не удалось разобрать .plist банка: {e}"))?;

    let mut ipa_url: Option<String> = None;
    let mut title: Option<String> = None;
    let mut bundle_id: Option<String> = None;
    let mut version: Option<String> = None;

                                                                                   
    if let Some(dict) = value.as_dictionary() {
        if let Some(items) = dict.get("items").and_then(|v| v.as_array()) {
            for item in items {
                let Some(item_dict) = item.as_dictionary() else {
                    continue;
                };
                if let Some(assets) = item_dict.get("assets").and_then(|v| v.as_array()) {
                    for asset in assets {
                        let Some(ad) = asset.as_dictionary() else {
                            continue;
                        };
                        let kind = ad
                            .get("kind")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        let url = ad.get("url").and_then(|v| v.as_string()).unwrap_or("");
                        if url.is_empty() {
                            continue;
                        }
                        if kind == "software-package"
                            || url.to_ascii_lowercase().contains(".ipa")
                            || kind.is_empty()
                        {
                            if kind == "software-package"
                                || url.to_ascii_lowercase().ends_with(".ipa")
                                || ipa_url.is_none()
                            {
                                ipa_url = Some(url.to_string());
                            }
                        }
                    }
                }
                if let Some(meta) = item_dict.get("metadata").and_then(|v| v.as_dictionary()) {
                    if title.is_none() {
                        title = meta
                            .get("title")
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string());
                    }
                    if bundle_id.is_none() {
                        bundle_id = meta
                            .get("bundle-identifier")
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string());
                    }
                    if version.is_none() {
                        version = meta
                            .get("bundle-version")
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string());
                    }
                }
            }
        }
    }

                                                               
    if ipa_url.is_none() {
        let text = String::from_utf8_lossy(bytes);
        if let Ok(re) = regex::Regex::new(r#"https?://[^\s"'<>]+\.ipa[^\s"'<>]*"#) {
            if let Some(m) = re.find(&text) {
                ipa_url = Some(m.as_str().replace("&amp;", "&"));
            }
        }
    }

    let ipa_url = ipa_url
        .ok_or_else(|| "В .plist банка нет ссылки на .ipa (software-package)".to_string())?;

                                          
    let ipa_url = if ipa_url.starts_with("http://") || ipa_url.starts_with("https://") {
        ipa_url
    } else if ipa_url.starts_with("//") {
        format!("https:{ipa_url}")
    } else if let Some(base) = plist_url.rsplit_once('/').map(|(b, _)| b) {
        format!("{base}/{ipa_url}")
    } else {
        ipa_url
    };

    Ok(BankManifestIpa {
        ipa_url,
        title,
        bundle_id,
        version,
        plist_url: plist_url.to_string(),
    })
}

                                                                                       
pub async fn download_ipa_file(
    url: &str,
    dest: &std::path::Path,
    mut progress: impl FnMut(f64),
) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::io::Write;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(900))
        .connect_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(12))
        .user_agent(UA_SOVCOM)
        .build()
        .map_err(|e| format!("HTTP: {e}"))?;
    let resp = client
        .get(url)
        .header("Accept", "*/*")
        .send()
        .await
        .map_err(|e| format!("Скачивание IPA: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Скачивание IPA: HTTP {}", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::File::create(dest).map_err(|e| format!("Создание IPA: {e}"))?;
    let mut done: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Сеть при скачивании IPA: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Запись IPA: {e}"))?;
        done += chunk.len() as u64;
        if total > 0 {
            progress(((done as f64 / total as f64) * 100.0).min(99.0));
        } else {
                                                      
            progress(((done as f64 / (50.0 * 1024.0 * 1024.0)) * 100.0).min(95.0));
        }
    }
    file.flush().ok();
    let meta = std::fs::metadata(dest).map_err(|e| e.to_string())?;
    if meta.len() < 1024 {
        let _ = std::fs::remove_file(dest);
        return Err("Скачанный файл слишком маленький — это не IPA".into());
    }
    progress(100.0);
    Ok(())
}
