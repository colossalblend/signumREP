                                                                     
   
                                                                                  
                                                                                        
                                                                                      
                                             
                                                                      
                                                                  
   
                                                                   
                                                                     

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use num_bigint::{BigUint, RandBigInt};
use num_traits::{One, Zero};
use pbkdf2::pbkdf2_hmac;
use rand::rngs::OsRng;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, COOKIE, SET_COOKIE, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::Sha256;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const RAP_ORIGIN: &str = "https://reportaproblem.apple.com";
const IDMSA: &str = "https://idmsa.apple.com";
const AUTH: &str = "https://idmsa.apple.com/appleauth/auth";
                                                              
const WIDGET_KEY: &str = "20379f32034f8867d352666ff2904d2152d5ff6843ee2db5ab5df863c14b1aef";
const ACCOUNT_WIDGET_KEY: &str = "af1139274f266b22b68c2a3e7ad932cb3c0bbe854e13a79af78dcc73136882c3";
const APP_ID: &str = "4199";
const RAP_API: &str = "3.0.0";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
                            
const PENDING_TTL: Duration = Duration::from_secs(10 * 60);

                                           
const N_HEX: &str = "\
AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050A37329CBB4\
A099ED8193E0757767A13DD52312AB4B03310DCD7F48A9DA04FD50E8083969EDB767B0CF60\
95179A163AB3661A05FBD5FAAAE82918A9962F0B93B855F97993EC975EEAA80D740ADBF4FF\
747359D041D5C33EA71D281E446B14773BCA97B43A23FB801676BD207A436C6481F1D2B907\
8717461A5B9D32E688F87748544523B524B0D57D5EA77A2775D2ECFA032CFBDBF52FB37861\
60279004E57AE6AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DB\
FBB694B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73";

                                                                              
               
                                                                              

#[derive(Debug, Clone)]
pub struct HistoryApp {
    pub adam_id: String,
    pub name: String,
    pub artist: String,
    pub artwork: String,
    pub price: String,
    pub date: String,
}

                                                                    
                                                                                           
#[derive(Debug, Clone)]
pub struct PendingHistoryAuth {
    pub email: String,
    cookies: HashMap<String, String>,
    scnt: Option<String>,
    session_id: Option<String>,
    session_token: Option<String>,
    repair_session_token: Option<String>,
                                                                                  
    auth_attributes: Option<String>,
                                                   
    phone_id: Option<i64>,
                                                               
    prefer_phone: bool,
    frame_id: String,
    hc_bits: u32,
    hc_challenge: Option<String>,
    target: AuthTarget,
    created: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthTarget {
    PurchaseHistory,
    AccountRegion,
}

impl AuthTarget {
    fn widget_key(self) -> &'static str {
        match self {
            Self::PurchaseHistory => WIDGET_KEY,
            Self::AccountRegion => ACCOUNT_WIDGET_KEY,
        }
    }

    fn redirect_uri(self) -> &'static str {
        match self {
            Self::PurchaseHistory => IDMSA,
            Self::AccountRegion => "https://account.apple.com",
        }
    }
}

impl PendingHistoryAuth {
    pub fn is_expired(&self) -> bool {
        self.created.elapsed() > PENDING_TTL
    }

    pub fn matches_email(&self, email: &str) -> bool {
        self.email.eq_ignore_ascii_case(email.trim())
    }
}

                                                                        
                                                                                
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountAuthSession {
    pub cookies: HashMap<String, String>,
    pub scnt: Option<String>,
    pub session_id: Option<String>,
    pub session_token: Option<String>,
    pub repair_session_token: Option<String>,
    pub auth_attributes: Option<String>,
}

                                                                                 
#[derive(Debug, Clone)]
pub struct HistorySuccess {
    pub apps: Vec<HistoryApp>,
                                                                           
    pub cookies: HashMap<String, String>,
                                                                    
    pub next_batch_id: Option<String>,
    pub has_more: bool,
                                                                                   
    pub rap_dsid: Option<String>,
                                                                       
    pub rap_xsrf: Option<String>,
}

                                                                                      
pub const HISTORY_PAGE_BATCHES: usize = 1;

#[derive(Debug)]
pub enum HistoryOutcome {
    Apps(HistorySuccess),
                                                                               
    Needs2fa {
        pending: PendingHistoryAuth,
    },
}

                                                                               
static LAST_DEBUG_LOG: Mutex<Option<String>> = Mutex::new(None);

                                                                        
pub fn last_debug_log_path() -> Option<String> {
    LAST_DEBUG_LOG.lock().ok().and_then(|g| g.clone())
}

                                                                              
                                          
                                                                  
                                                                              

#[derive(Clone)]
struct DebugLog {
    path: PathBuf,
    inner: Arc<Mutex<()>>,
}

impl DebugLog {
    fn create() -> Self {
        let base = std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".into());
        let dir = PathBuf::from(base).join("Signum").join("history-debug");
        let _ = fs::create_dir_all(&dir);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
                                        
        let path = dir.join(format!("history-{stamp}.log"));
        let log = Self {
            path: path.clone(),
            inner: Arc::new(Mutex::new(())),
        };
        log.write_raw(&format!(
            "=== Signum purchase-history HTTP debug ===\nstart_unix={stamp}\npath={}\n\n",
            path.display()
        ));
        if let Ok(mut g) = LAST_DEBUG_LOG.lock() {
            *g = Some(path.to_string_lossy().into_owned());
        }
        log
    }

    fn path_str(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    fn write_raw(&self, text: &str) {
        let _g = self.inner.lock();
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = f.write_all(text.as_bytes());
            let _ = f.flush();
        }
    }

    fn note(&self, msg: &str) {
        self.write_raw(&format!("[NOTE] {msg}\n"));
    }

    fn phase(&self, msg: &str) {
        self.write_raw(&format!("\n======== {msg} ========\n"));
    }

    fn request(&self, method: &str, url: &str, headers: &HeaderMap, body: Option<&[u8]>) {
        let mut s = format!("\n>> {method} {url}\n");
        for (k, v) in headers.iter() {
            let key = k.as_str();
            let val = v.to_str().unwrap_or("<bin>");
            let redacted = redact_header(key, val);
            s.push_str(&format!("   H {key}: {redacted}\n"));
        }
        if let Some(b) = body {
            let snippet = body_snippet(b, 1200);
            s.push_str(&format!("   BODY ({}) {}\n", b.len(), snippet));
        }
        self.write_raw(&s);
    }

    fn response(&self, status: u16, headers: &HeaderMap, body: &[u8], location: Option<&str>) {
        let mut s = format!("<< status={status}\n");
        if let Some(loc) = location {
            s.push_str(&format!("   Location: {loc}\n"));
        }
        for name in [
            "scnt",
            "x-apple-id-session-id",
            "x-apple-session-token",
            "x-apple-repair-session-token",
            "x-apple-auth-attributes",
            "x-apple-hc-bits",
            "x-apple-hc-challenge",
            "set-cookie",
            "content-type",
            "location",
        ] {
            for v in headers.get_all(name) {
                let val = v.to_str().unwrap_or("<bin>");
                s.push_str(&format!("   RH {name}: {}\n", redact_header(name, val)));
            }
        }
        s.push_str(&format!(
            "   BODY ({}) {}\n",
            body.len(),
            body_snippet(body, 2000)
        ));
        self.write_raw(&s);
    }
}

fn redact_header(key: &str, val: &str) -> String {
    let k = key.to_ascii_lowercase();
    if k == "cookie" || k == "set-cookie" {
                                 
        let names: Vec<&str> = val
            .split(';')
            .filter_map(|p| p.split('=').next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        return format!("[cookies names={:?} len={}]", names, val.len());
    }
    if k.contains("token") || k == "scnt" || k.contains("session") || k.contains("password") {
        if val.len() <= 12 {
            return "***".into();
        }
        return format!(
            "{}…{} (len={})",
            &val[..6],
            &val[val.len() - 4..],
            val.len()
        );
    }
    if val.len() > 300 {
        return format!("{}… (len={})", &val[..200], val.len());
    }
    val.to_string()
}

fn body_snippet(body: &[u8], max: usize) -> String {
    let text = String::from_utf8_lossy(body);
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
                                                                                
                                                                
    if flat.chars().count() > max {
        let truncated: String = flat.chars().take(max).collect();
        format!("{truncated}…")
    } else {
        flat
    }
}

#[derive(Debug)]
pub enum HistoryError {
    WrongPassword,
    AccountLocked,
    Invalid2fa,
                                                                                           
    Temporary(String),
                                                                         
    SessionExpired,
    Message(String),
}

impl HistoryError {
    pub fn user_message(&self) -> String {
        match self {
            Self::WrongPassword => {
                "Неверный пароль Apple ID. Проверьте пароль и попробуйте снова.".into()
            }
            Self::AccountLocked => {
                "Apple ID временно заблокирован. Подождите немного и повторите вход.".into()
            }
            Self::Invalid2fa => {
                "Код не принят. Введите актуальный 6-значный код с iPhone (новый код не запрашивался).".into()
            }
            Self::Temporary(message) => message.clone(),
            Self::SessionExpired => {
                "Сессия истекла. Нажмите «Загрузить историю» ещё раз.".into()
            }
            Self::Message(m) => m.clone(),
        }
    }

                                                                       
    pub fn keep_pending(&self) -> bool {
        matches!(self, Self::Invalid2fa | Self::Temporary(_))
    }

    pub fn clear_pending(&self) -> bool {
        matches!(
            self,
            Self::WrongPassword | Self::AccountLocked | Self::SessionExpired | Self::Message(_)
        )
    }
}

                                                                              
             
                                                                              

#[derive(Default, Clone)]
struct CookieJar {
    map: HashMap<String, String>,
}

impl CookieJar {
    fn from_map(map: HashMap<String, String>) -> Self {
        Self { map }
    }

    fn store_from_response(&mut self, headers: &HeaderMap) {
        for value in headers.get_all(SET_COOKIE) {
            let Ok(raw) = value.to_str() else { continue };
                                                                                         
                                                                                       
            for part in split_set_cookie_header(raw) {
                let first = part.split(';').next().unwrap_or("").trim();
                if let Some((k, v)) = first.split_once('=') {
                    let k = k.trim();
                    if k.is_empty() {
                        continue;
                    }
                    if v.trim().is_empty() {
                        self.map.remove(k);
                    } else {
                        self.map.insert(k.to_string(), v.trim().to_string());
                    }
                }
            }
        }
    }

    fn header_value(&self) -> Option<String> {
        if self.map.is_empty() {
            return None;
        }
        Some(
            self.map
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

                                                                              
                     
                                                                              

struct AppleSrp {
    n: BigUint,
    g: BigUint,
    k: BigUint,
    a: BigUint,
    a_pub: BigUint,
    n_len: usize,
}

impl AppleSrp {
    fn new() -> Self {
        let n = BigUint::parse_bytes(N_HEX.as_bytes(), 16).expect("N");
        let g = BigUint::from(2u32);
        let n_len = (n.bits() as usize + 7) / 8;
        let k = bytes_to_int(&sha256_bytes(&[
            &int_to_bytes(&n),
            &int_to_bytes_width(&g, n_len),
        ]));
        let mut a = OsRng.gen_biguint(256 * 8);
        a |= BigUint::one() << (256 * 8 - 1);
        let a_pub = g.modpow(&a, &n);
        Self {
            n,
            g,
            k,
            a,
            a_pub,
            n_len,
        }
    }

    fn public_a_bytes(&self) -> Vec<u8> {
        int_to_bytes(&self.a_pub)
    }

    fn process_challenge(
        &self,
        username: &str,
        password_key: &[u8],
        salt: &[u8],
        b_bytes: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), HistoryError> {
        let b = bytes_to_int(b_bytes);
        if (&b % &self.n).is_zero() {
            return Err(HistoryError::Message(
                "Не удалось войти в Apple ID. Повторите попытку.".into(),
            ));
        }
        let u = bytes_to_int(&sha256_bytes(&[
            &int_to_bytes_width(&self.a_pub, self.n_len),
            &int_to_bytes_width(&b, self.n_len),
        ]));
        if u.is_zero() {
            return Err(HistoryError::Message(
                "Не удалось войти в Apple ID. Повторите попытку.".into(),
            ));
        }

                                                                         
        let inner = sha256_bytes(&[b":", password_key]);
        let x = bytes_to_int(&sha256_bytes(&[salt, &inner]));

        let v = self.g.modpow(&x, &self.n);
        let kv = (&self.k * &v) % &self.n;
        let base = if b >= kv {
            (&b - &kv) % &self.n
        } else {
            (&self.n + &b - &kv) % &self.n
        };
        let exp = &self.a + &u * &x;
        let s = base.modpow(&exp, &self.n);
        let k_sess = sha256_bytes(&[&int_to_bytes(&s)]);

        let h_n_xor_g = {
            let hn = sha256_bytes(&[&int_to_bytes(&self.n)]);
            let hg = sha256_bytes(&[&int_to_bytes_width(&self.g, self.n_len)]);
            hn.iter()
                .zip(hg.iter())
                .map(|(a, b)| a ^ b)
                .collect::<Vec<u8>>()
        };
        let h_i = sha256_bytes(&[username.as_bytes()]);
        let m1 = sha256_bytes(&[
            &h_n_xor_g,
            &h_i,
            salt,
            &int_to_bytes(&self.a_pub),
            &int_to_bytes(&b),
            &k_sess,
        ]);
        let m2 = sha256_bytes(&[&int_to_bytes(&self.a_pub), &m1, &k_sess]);
        Ok((m1, m2))
    }
}

fn derive_s2k_password(password: &str, protocol: &str, salt: &[u8], iterations: u32) -> Vec<u8> {
    let digest = sha256_bytes(&[password.as_bytes()]);
    let material: Vec<u8> = if protocol == "s2k_fo" {
        hex::encode(&digest).into_bytes()
    } else {
        digest
    };
    let mut out = [0u8; 32];
    pbkdf2_hmac::<Sha256>(&material, salt, iterations, &mut out);
    out.to_vec()
}

fn generate_hashcash(bits: u32, challenge: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let date = unix_to_yyyymmddhhmmss(now);
    let mut counter: u64 = 0;
    loop {
        let token = format!("1:{bits}:{date}:{challenge}::{counter}");
        let hash = Sha1::digest(token.as_bytes());
        if leading_zero_bits(&hash) >= bits {
            return token;
        }
        counter += 1;
        if counter > 50_000_000 {
            return token;
        }
    }
}

fn leading_zero_bits(hash: &[u8]) -> u32 {
    let mut bits = 0u32;
    for b in hash {
        if *b == 0 {
            bits += 8;
        } else {
            bits += b.leading_zeros();
            break;
        }
    }
    bits
}

fn unix_to_yyyymmddhhmmss(secs: u64) -> String {
    let z = (secs / 86400) as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let tod = secs % 86400;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{:04}{:02}{:02}{:02}{:02}{:02}", y, m, d, hh, mm, ss)
}

fn sha256_bytes(parts: &[&[u8]]) -> Vec<u8> {
    use sha2::Digest;
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().to_vec()
}

fn int_to_bytes(n: &BigUint) -> Vec<u8> {
    let mut v = n.to_bytes_be();
    if v.is_empty() {
        v.push(0);
    }
    v
}

fn int_to_bytes_width(n: &BigUint, width: usize) -> Vec<u8> {
    let mut v = n.to_bytes_be();
    if v.len() < width {
        let mut out = vec![0u8; width - v.len()];
        out.append(&mut v);
        out
    } else if v.len() > width {
        v[v.len() - width..].to_vec()
    } else {
        v
    }
}

fn bytes_to_int(b: &[u8]) -> BigUint {
    BigUint::from_bytes_be(b)
}

                                                                              
                                                                    
                                                                              

                                                                       
                                                          
fn split_set_cookie_header(raw: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = raw.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b',' {
                                                                               
            let rest = raw[i + 1..].trim_start();
            if let Some(eq) = rest.find('=') {
                let name = rest[..eq].trim();
                if !name.is_empty()
                    && name.is_ascii()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    && !name.eq_ignore_ascii_case("expires")
                    && !name.eq_ignore_ascii_case("max-age")
                    && !name.eq_ignore_ascii_case("path")
                    && !name.eq_ignore_ascii_case("domain")
                    && !name.eq_ignore_ascii_case("secure")
                    && !name.eq_ignore_ascii_case("httponly")
                    && !name.eq_ignore_ascii_case("samesite")
                {
                    out.push(raw[start..i].trim());
                    start = i + 1;
                }
            }
        }
        i += 1;
    }
    let last = raw[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    if out.is_empty() {
        out.push(raw);
    }
    out
}

struct Session {
    client: reqwest::Client,
    cookies: CookieJar,
    scnt: Option<String>,
    session_id: Option<String>,
    session_token: Option<String>,
    repair_session_token: Option<String>,
    auth_attributes: Option<String>,
    frame_id: String,
    hc_bits: u32,
    hc_challenge: Option<String>,
    target: AuthTarget,
    debug: DebugLog,
}

impl Session {
    fn new(debug: DebugLog) -> Result<Self, HistoryError> {
        Self::new_for_target(debug, AuthTarget::PurchaseHistory)
    }

    fn new_for_target(debug: DebugLog, target: AuthTarget) -> Result<Self, HistoryError> {
                                                                               
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(UA)
            .build()
            .map_err(|_| {
                HistoryError::Message("Не удалось подключиться к Apple. Проверьте интернет.".into())
            })?;
        Ok(Self {
            client,
            cookies: CookieJar::default(),
            scnt: None,
            session_id: None,
            session_token: None,
            repair_session_token: None,
            auth_attributes: None,
            frame_id: if target == AuthTarget::AccountRegion {
                format!("auth-{}", uuid::Uuid::new_v4())
            } else {
                format!("daw-{}", uuid::Uuid::new_v4())
            },
            hc_bits: 11,
            hc_challenge: None,
            target,
            debug,
        })
    }

    fn from_pending(p: &PendingHistoryAuth, debug: DebugLog) -> Result<Self, HistoryError> {
        if p.is_expired() {
            debug.note("pending session EXPIRED");
            return Err(HistoryError::SessionExpired);
        }
        let mut s = Self::new_for_target(debug, p.target)?;
        s.cookies = CookieJar::from_map(p.cookies.clone());
        s.scnt = p.scnt.clone();
        s.session_id = p.session_id.clone();
        s.session_token = p.session_token.clone();
        s.repair_session_token = p.repair_session_token.clone();
        s.auth_attributes = p.auth_attributes.clone();
        s.frame_id = p.frame_id.clone();
        s.hc_bits = p.hc_bits;
        s.hc_challenge = p.hc_challenge.clone();
        s.debug.note(&format!(
            "restored pending email={} cookies={} scnt_len={} sid_len={} phone={:?} prefer_phone={} attrs={} frame={}",
            p.email,
            s.cookies.map.len(),
            s.scnt.as_ref().map(|x| x.len()).unwrap_or(0),
            s.session_id.as_ref().map(|x| x.len()).unwrap_or(0),
            p.phone_id,
            p.prefer_phone,
            s.auth_attributes.is_some(),
            s.frame_id
        ));
        Ok(s)
    }

    fn to_pending(&self, email: &str) -> PendingHistoryAuth {
        PendingHistoryAuth {
            email: email.trim().to_string(),
            cookies: self.cookies.map.clone(),
            scnt: self.scnt.clone(),
            session_id: self.session_id.clone(),
            session_token: self.session_token.clone(),
            repair_session_token: self.repair_session_token.clone(),
            auth_attributes: self.auth_attributes.clone(),
            phone_id: None,
            prefer_phone: false,
            frame_id: self.frame_id.clone(),
            hc_bits: self.hc_bits,
            hc_challenge: self.hc_challenge.clone(),
            target: self.target,
            created: Instant::now(),
        }
    }

    fn account_auth_session(&self) -> AccountAuthSession {
        AccountAuthSession {
            cookies: self.cookies.map.clone(),
            scnt: self.scnt.clone(),
            session_id: self.session_id.clone(),
            session_token: self.session_token.clone(),
            repair_session_token: self.repair_session_token.clone(),
            auth_attributes: self.auth_attributes.clone(),
        }
    }

    fn to_pending_with_phone(
        &self,
        email: &str,
        phone_id: Option<i64>,
        prefer_phone: bool,
    ) -> PendingHistoryAuth {
        let mut p = self.to_pending(email);
        p.phone_id = phone_id;
        p.prefer_phone = prefer_phone;
        p
    }

                                                                                                
    fn ingest_headers(&mut self, headers: &HeaderMap, update_session: bool) {
        self.cookies.store_from_response(headers);
        if !update_session {
            return;
        }
        if let Some(v) = header_str(headers, "scnt") {
            if !v.is_empty() {
                self.scnt = Some(v);
            }
        }
        if let Some(v) = header_str(headers, "X-Apple-ID-Session-Id")
            .or_else(|| header_str(headers, "x-apple-id-session-id"))
            .or_else(|| header_str(headers, "X-Apple-Id-Session-Id"))
        {
            if !v.is_empty() {
                self.session_id = Some(v);
            }
        }
        if let Some(v) = header_str(headers, "X-Apple-Session-Token")
            .or_else(|| header_str(headers, "x-apple-session-token"))
        {
            if !v.is_empty() {
                self.session_token = Some(v);
            }
        }
        if let Some(v) = header_str(headers, "X-Apple-Repair-Session-Token")
            .or_else(|| header_str(headers, "x-apple-repair-session-token"))
        {
            if !v.is_empty() {
                self.repair_session_token = Some(v);
            }
        }
        if let Some(v) = header_str(headers, "X-Apple-Auth-Attributes")
            .or_else(|| header_str(headers, "x-apple-auth-attributes"))
        {
            if !v.is_empty() {
                self.auth_attributes = Some(v);
            }
        }
        if let Some(v) = header_str(headers, "X-Apple-HC-Bits")
            .or_else(|| header_str(headers, "x-apple-hc-bits"))
            .and_then(|s| s.parse().ok())
        {
            if (1..=20).contains(&v) {
                self.hc_bits = v;
            }
        }
        if let Some(v) = header_str(headers, "X-Apple-HC-Challenge")
            .or_else(|| header_str(headers, "x-apple-hc-challenge"))
        {
            if !v.is_empty() {
                self.hc_challenge = Some(v);
            }
        }
    }

                                                                                      
                                                                
    fn auth_headers_minimal(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static(UA));
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert("Accept", HeaderValue::from_static("application/json"));
        h.insert(
            "X-Apple-Widget-Key",
            HeaderValue::from_static(self.target.widget_key()),
        );
        h.insert(
            "X-Requested-With",
            HeaderValue::from_static("XMLHttpRequest"),
        );
        if self.target == AuthTarget::PurchaseHistory {
            h.insert("Origin", HeaderValue::from_static(IDMSA));
        }
        h.insert(
            "Referer",
            HeaderValue::from_static("https://idmsa.apple.com/"),
        );
        if let Some(scnt) = &self.scnt {
            if let Ok(v) = HeaderValue::from_str(scnt) {
                h.insert("scnt", v);
            }
        }
        if let Some(sid) = &self.session_id {
            if let Ok(v) = HeaderValue::from_str(sid) {
                h.insert("X-Apple-Id-Session-Id", v);
            }
        }
                                                                                            
        if let Some(attrs) = &self.auth_attributes {
            if let Ok(v) = HeaderValue::from_str(attrs) {
                h.insert("X-Apple-Auth-Attributes", v);
            }
        }
        h
    }

                                                                                       
    fn auth_headers_minimal_get(&self) -> HeaderMap {
        let mut h = self.auth_headers_minimal();
        h.remove(CONTENT_TYPE);
        h
    }

    fn two_factor_headers(&self) -> HeaderMap {
        if self.target == AuthTarget::AccountRegion {
            let mut headers = self.auth_headers(false);
            headers.insert(
                "Accept",
                HeaderValue::from_static("application/json, text/plain, */*"),
            );
            headers.insert("Origin", HeaderValue::from_static(IDMSA));
            headers.insert(
                "X-Apple-App-Id",
                HeaderValue::from_static(ACCOUNT_WIDGET_KEY),
            );
            headers.remove("X-Requested-With");
            headers
        } else {
            self.auth_headers_minimal()
        }
    }

    fn arm_headers(&self) -> HeaderMap {
        if self.target == AuthTarget::AccountRegion {
            let mut headers = self.auth_headers(false);
            headers.insert("Accept", HeaderValue::from_static("text/html"));
            headers.remove("Origin");
            headers
        } else {
            self.auth_headers(false)
        }
    }

    fn auth_headers(&self, with_hc: bool) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static(UA));
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                                         
        h.insert(
            "Accept",
            HeaderValue::from_static("application/json, text/javascript, */*; q=0.01"),
        );
        h.insert(
            "Accept-Language",
            HeaderValue::from_static("ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7"),
        );
        h.insert(
            "X-Apple-Widget-Key",
            HeaderValue::from_static(self.target.widget_key()),
        );
        h.insert(
            "X-Apple-OAuth-Client-Id",
            HeaderValue::from_static(self.target.widget_key()),
        );
        h.insert(
            "X-Apple-OAuth-Client-Type",
            HeaderValue::from_static("firstPartyAuth"),
        );
        h.insert(
            "X-Apple-OAuth-Redirect-URI",
            HeaderValue::from_static(self.target.redirect_uri()),
        );
        h.insert(
            "X-Apple-OAuth-Response-Type",
            HeaderValue::from_static("code"),
        );
        h.insert(
            "X-Apple-OAuth-Response-Mode",
            HeaderValue::from_static("web_message"),
        );
        if self.target == AuthTarget::PurchaseHistory {
            h.insert(
                "X-Apple-OAuth-Require-Grant-Code",
                HeaderValue::from_static("true"),
            );
        }
        if let Ok(v) = HeaderValue::from_str(&self.frame_id) {
            h.insert("X-Apple-OAuth-State", v.clone());
            h.insert("X-Apple-Frame-Id", v);
        }
        if self.target == AuthTarget::PurchaseHistory {
            h.insert("X-Apple-App-Id", HeaderValue::from_static(APP_ID));
            h.insert("X-Apple-Locale", HeaderValue::from_static("RU-RU"));
            h.insert("X-Apple-Trusted-Domain", HeaderValue::from_static(IDMSA));
        } else {
            h.insert("X-Apple-Domain-Id", HeaderValue::from_static("11"));
            h.insert("X-Apple-Privacy-Consent", HeaderValue::from_static("true"));
        }
        h.insert(
            "X-Apple-Privacy-Consent-Accepted",
            HeaderValue::from_static("true"),
        );
        h.insert(
            "X-Requested-With",
            HeaderValue::from_static("XMLHttpRequest"),
        );
        if self.target == AuthTarget::PurchaseHistory {
            h.insert("Origin", HeaderValue::from_static(IDMSA));
        }
        h.insert(
            "Referer",
            HeaderValue::from_static("https://idmsa.apple.com/"),
        );
                                                       
        let fd = format!(
            r#"{{"U":"{UA}","L":"ru-RU","Z":"GMT+03:00","V":"1.1","F":"Nba44j1e3NlY5BNlY5BSs5uQ084akLMeN.DmJa"}}"#
        );
        if let Ok(v) = HeaderValue::from_str(&fd) {
            h.insert("X-Apple-I-FD-Client-Info", v);
        }
        if let Some(scnt) = &self.scnt {
            if let Ok(v) = HeaderValue::from_str(scnt) {
                h.insert("scnt", v);
            }
        }
        if let Some(sid) = &self.session_id {
            if let Ok(v) = HeaderValue::from_str(sid) {
                h.insert("X-Apple-ID-Session-Id", v);
            }
        }
        if let Some(tok) = &self.session_token {
            if let Ok(v) = HeaderValue::from_str(tok) {
                h.insert("X-Apple-Session-Token", v);
            }
        }
        if let Some(attrs) = &self.auth_attributes {
            if let Ok(v) = HeaderValue::from_str(attrs) {
                h.insert("X-Apple-Auth-Attributes", v);
            }
        }
        if with_hc {
            let challenge = self
                .hc_challenge
                .clone()
                .unwrap_or_else(|| hex::encode(OsRng.gen_biguint(128).to_bytes_be()));
            let hc = generate_hashcash(self.hc_bits, &challenge);
            if let Ok(v) = HeaderValue::from_str(&hc) {
                h.insert("X-Apple-HC", v);
            }
        }
                                                                                     
        h
    }

                                                                                   
                                                                       
    async fn request_once(
        &mut self,
        method: &str,
        url: &str,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
        update_session: bool,
    ) -> Result<(u16, HeaderMap, Vec<u8>, Option<String>), HistoryError> {
                                         
        let mut log_headers = headers.clone();
        if let Some(c) = self.cookies.header_value() {
            if let Ok(v) = HeaderValue::from_str(&c) {
                log_headers.insert(COOKIE, v);
            }
        }
        self.debug
            .request(method, url, &log_headers, body.as_deref());
        crate::diag_log::http(
            method,
            url,
            0,
            "request",
            body.as_deref().unwrap_or(&[]),
        );

        let mut builder = match method {
            "GET" => self.client.get(url),
            "POST" => self.client.post(url),
            "PUT" => self.client.put(url),
            _ => self.client.get(url),
        };
        builder = builder.headers(headers);
        if let Some(c) = self.cookies.header_value() {
            builder = builder.header(COOKIE, c);
        }
        if let Some(b) = body {
            builder = builder.body(b);
        }
        let resp = builder.send().await.map_err(|e| {
            self.debug.note(&format!("NETWORK ERROR: {e}"));
            HistoryError::Message("Нет связи с Apple. Проверьте интернет.".into())
        })?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
                                                                                               
        self.ingest_headers(&headers, update_session);
        let location =
            header_str(&headers, "location").or_else(|| header_str(&headers, "Location"));
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| {
                self.debug.note(&format!("BODY READ ERROR: {e}"));
                HistoryError::Message("Ответ Apple оборвался. Повторите попытку.".into())
            })?
            .to_vec();
        self.debug
            .response(status, &headers, &bytes, location.as_deref());
        let loc = location.as_deref().unwrap_or("");
        crate::diag_log::http(
            method,
            url,
            status,
            &format!("loc={}", loc.chars().take(180).collect::<String>()),
            &bytes,
        );
        Ok((status, headers, bytes, location))
    }

                                                                   
                                                                                   
                                                                                                   
    async fn request_follow(
        &mut self,
        mut method: &str,
        mut url: String,
        headers: HeaderMap,
        mut body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>), HistoryError> {
        for _ in 0..12 {
            let (status, _hdrs, bytes, location) = self
                .request_once(method, &url, headers.clone(), body.take(), true)
                .await?;
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let loc = location.ok_or_else(|| {
                    HistoryError::Message("Не удалось завершить вход в Apple ID.".into())
                })?;
                let next = resolve_url(&url, &loc);
                if !is_apple_auth_host(&next) {
                    self.debug.note(&format!(
                        "stop redirect off-auth: {status} -> {next} (keep idmsa session)"
                    ));
                    return Ok((status, bytes));
                }
                url = next;
                                                                                         
                if status != 307 && status != 308 {
                    method = "GET";
                    body = None;
                }
                continue;
            }
            return Ok((status, bytes));
        }
        Err(HistoryError::Message(
            "Не удалось завершить вход в Apple ID.".into(),
        ))
    }

                                                                                      
    async fn request_frozen(
        &mut self,
        method: &str,
        url: &str,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>), HistoryError> {
        let (status, _h, bytes, _loc) =
            self.request_once(method, url, headers, body, false).await?;
        Ok((status, bytes))
    }

    async fn get_follow(&mut self, url: &str) -> Result<(u16, Vec<u8>), HistoryError> {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static(UA));
        h.insert(
            "Accept",
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8",
            ),
        );
        self.request_follow("GET", url.to_string(), h, None).await
    }

    async fn post_json(
        &mut self,
        url: &str,
        with_hc: bool,
        body: &JsonValue,
    ) -> Result<(u16, Vec<u8>), HistoryError> {
                                                                               
        self.request_follow(
            "POST",
            url.to_string(),
            self.auth_headers(with_hc),
            Some(body.to_string().into_bytes()),
        )
        .await
    }

    async fn post_form_follow(
        &mut self,
        url: &str,
        form: &str,
    ) -> Result<(u16, Vec<u8>), HistoryError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        headers.insert(
            "Accept",
            HeaderValue::from_static("text/html,application/xhtml+xml"),
        );
        headers.insert("Origin", HeaderValue::from_static(IDMSA));
        headers.insert(
            "Referer",
            HeaderValue::from_static("https://idmsa.apple.com/"),
        );
        self.request_follow(
            "POST",
            url.to_string(),
            headers,
            Some(form.as_bytes().to_vec()),
        )
        .await
    }
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn resolve_url(base: &str, loc: &str) -> String {
    if loc.starts_with("http://") || loc.starts_with("https://") {
        return loc.to_string();
    }
    if loc.starts_with("//") {
        return format!("https:{loc}");
    }
                                   
    if loc.starts_with('/') {
        let parts: Vec<&str> = base.splitn(4, '/').collect();
                                            
        if parts.len() >= 3 {
            return format!("{}//{}{}", parts[0], parts[2], loc);
        }
    }
                    
    if let Some(idx) = base.rfind('/') {
        return format!("{}{}", &base[..=idx], loc);
    }
    loc.to_string()
}

                                                                              
               
                                                                              

enum AccountAuthOutcome {
    Authenticated(Session),
    Needs2fa(PendingHistoryAuth),
}

                                         
   
                                                                                       
                                                                      
pub async fn fetch_purchase_history(
    email: &str,
    password: &str,
    two_factor_code: Option<&str>,
    pending: Option<PendingHistoryAuth>,
) -> Result<HistoryOutcome, HistoryError> {
    match authenticate_idmsa(
        email,
        password,
        two_factor_code,
        pending,
        AuthTarget::PurchaseHistory,
    )
    .await?
    {
        AccountAuthOutcome::Authenticated(mut session) => {
            complete_after_auth(&mut session, two_factor_code.is_some()).await
        }
        AccountAuthOutcome::Needs2fa(pending) => Ok(HistoryOutcome::Needs2fa { pending }),
    }
}

                                                                                        
                                                                                  
pub async fn authenticate_account(
    email: &str,
    password: &str,
    two_factor_code: Option<&str>,
    pending: Option<PendingHistoryAuth>,
) -> Result<Result<AccountAuthSession, PendingHistoryAuth>, HistoryError> {
    match authenticate_idmsa(
        email,
        password,
        two_factor_code,
        pending,
        AuthTarget::PurchaseHistory,
    )
    .await?
    {
        AccountAuthOutcome::Authenticated(session) => Ok(Ok(session.account_auth_session())),
        AccountAuthOutcome::Needs2fa(pending) => Ok(Err(pending)),
    }
}

pub async fn authenticate_region_account(
    email: &str,
    password: &str,
    two_factor_code: Option<&str>,
    pending: Option<PendingHistoryAuth>,
) -> Result<Result<AccountAuthSession, PendingHistoryAuth>, HistoryError> {
    match authenticate_idmsa(
        email,
        password,
        two_factor_code,
        pending,
        AuthTarget::AccountRegion,
    )
    .await?
    {
        AccountAuthOutcome::Authenticated(session) => Ok(Ok(session.account_auth_session())),
        AccountAuthOutcome::Needs2fa(pending) => Ok(Err(pending)),
    }
}

async fn authenticate_idmsa(
    email: &str,
    password: &str,
    two_factor_code: Option<&str>,
    pending: Option<PendingHistoryAuth>,
    target: AuthTarget,
) -> Result<AccountAuthOutcome, HistoryError> {
    let debug = DebugLog::create();
    let email = email.trim().to_ascii_lowercase();
    let password = password.trim();
    let log_path = debug.path_str();
    debug.phase("START");
    debug.note(&format!(
        "email={email} password_len={} code_len={} pending={}",
        password.len(),
        two_factor_code.map(|c| c.len()).unwrap_or(0),
        pending
            .as_ref()
            .map(|p| format!(
                "yes email={} expired={} match={}",
                p.email,
                p.is_expired(),
                p.matches_email(&email)
            ))
            .unwrap_or_else(|| "no".into())
    ));

    if email.is_empty() {
        return Err(HistoryError::Message("Сначала войдите в Apple ID.".into()));
    }

    let code = two_factor_code
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(6)
        .collect::<String>();
    let code = if code.len() >= 6 { Some(code) } else { None };

                                                                                
    if let Some(code) = code.as_ref() {
        debug.phase("PATH A — resume 2FA session");
        if let Some(p) = pending.as_ref() {
            if p.matches_email(&email) && !p.is_expired() {
                let mut session = Session::from_pending(p, debug.clone())?;
                match verify_2fa(&mut session, code, p.phone_id, p.prefer_phone).await {
                    Ok(()) => {
                        session.debug.note("verify_2fa OK");
                        return Ok(AccountAuthOutcome::Authenticated(session));
                    }
                    Err(e) => {
                        session.debug.note(&format!("verify_2fa FAILED: {e:?}"));
                        session
                            .debug
                            .note(&format!("LOG FILE: {}", session.debug.path_str()));
                                                                           
                        return Err(e);
                    }
                }
            } else {
                debug.note(&format!(
                    "pending mismatch/expired: pending_email={} expired={}",
                    p.email,
                    p.is_expired()
                ));
            }
        } else {
            debug.note("no pending session in memory — cannot use code");
        }
        debug.note(&format!("LOG FILE: {log_path}"));
        return Err(HistoryError::SessionExpired);
    }

    if password.is_empty() {
        return Err(HistoryError::Message(
            "Пароль не найден. Выйдите и войдите в Apple ID снова.".into(),
        ));
    }

                                       
    debug.phase("PATH B — full SRP login");
    let mut session = Session::new_for_target(debug, target)?;
    bootstrap_idmsa(&mut session).await?;

    let (st, body) = session
        .post_json(
            &format!("{AUTH}/federate?isRememberMeEnabled=true"),
            false,
            &json!({ "accountName": email, "rememberMe": true }),
        )
        .await?;
    if st == 401 || st == 403 {
        return Err(classify_auth_error(&body));
    }

    let srp = AppleSrp::new();
    let init_body = json!({
        "a": B64.encode(srp.public_a_bytes()),
        "accountName": email,
        "protocols": ["s2k", "s2k_fo"]
    });
    let (st, body) = session
        .post_json(&format!("{AUTH}/signin/init"), false, &init_body)
        .await?;
    if st >= 400 {
        return Err(classify_auth_error(&body));
    }
    let init: JsonValue = serde_json::from_slice(&body).map_err(|_| {
        HistoryError::Message("Не удалось начать вход в Apple ID. Повторите.".into())
    })?;
    let salt = B64
        .decode(
            init.get("salt").and_then(|v| v.as_str()).ok_or_else(|| {
                HistoryError::Message("Не удалось начать вход в Apple ID.".into())
            })?,
        )
        .map_err(|_| HistoryError::Message("Не удалось начать вход в Apple ID.".into()))?;
    let b_bytes = B64
        .decode(
            init.get("b").and_then(|v| v.as_str()).ok_or_else(|| {
                HistoryError::Message("Не удалось начать вход в Apple ID.".into())
            })?,
        )
        .map_err(|_| HistoryError::Message("Не удалось начать вход в Apple ID.".into()))?;
    let c = init
        .get("c")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let iterations = init
        .get("iteration")
        .and_then(|v| v.as_u64())
        .unwrap_or(20000) as u32;
    let protocol = init
        .get("protocol")
        .and_then(|v| v.as_str())
        .unwrap_or("s2k")
        .to_string();

    let password_key = derive_s2k_password(password, &protocol, &salt, iterations);
    let (m1, m2) = srp.process_challenge(&email, &password_key, &salt, &b_bytes)?;

    let complete_body = json!({
        "accountName": email,
        "rememberMe": true,
        "m1": B64.encode(&m1),
        "c": c,
        "m2": B64.encode(&m2),
        "trustTokens": []
    });
    let (st, body) = session
        .post_json(
            &format!("{AUTH}/signin/complete?isRememberMeEnabled=true"),
            true,
            &complete_body,
        )
        .await?;

    let auth_type = serde_json::from_slice::<JsonValue>(&body)
        .ok()
        .and_then(|v| {
            v.get("authType")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();

    session.debug.note(&format!(
        "signin/complete status={st} authType={auth_type} scnt={} sid={} token={}",
        session.scnt.is_some(),
        session.session_id.is_some(),
        session.session_token.is_some()
    ));

                                                                             
                                                           
                                                        
    match st {
        200 => {
            session.debug.phase("complete 200 — no secondary auth");
        }
        409 => {
            session.debug.phase("complete 409 — TRUE 2FA needed");
                                                                                      
            let pending = arm_2fa_pending(&mut session, &email).await;
            session
                .debug
                .note(&format!("LOG FILE: {}", session.debug.path_str()));
            return Ok(AccountAuthOutcome::Needs2fa(pending));
        }
        412 => {
            if auth_type == "hsa2" || auth_type == "2sa" {
                session.debug.phase("complete 412 authType hsa2 — TRUE 2FA");
                let pending = arm_2fa_pending(&mut session, &email).await;
                session
                    .debug
                    .note(&format!("LOG FILE: {}", session.debug.path_str()));
                return Ok(AccountAuthOutcome::Needs2fa(pending));
            }
            session
                .debug
                .phase("complete 412 sa — repair/setuplater (no 2FA code)");
            run_sa_repair_flow(&mut session).await?;
        }
        401 | 403 => {
            session.debug.phase("complete 401/403 — auth error");
            session
                .debug
                .note(&format!("LOG FILE: {}", session.debug.path_str()));
            return Err(with_log_path(
                classify_auth_error(&body),
                &session.debug.path_str(),
            ));
        }
        s if s >= 400 => {
            session.debug.note(&format!(
                "complete unexpected status {s} authType={auth_type}"
            ));
            if auth_type == "hsa2" || auth_type == "2sa" || looks_like_2fa_body(&body) {
                let pending = arm_2fa_pending(&mut session, &email).await;
                session
                    .debug
                    .note(&format!("LOG FILE: {}", session.debug.path_str()));
                return Ok(AccountAuthOutcome::Needs2fa(pending));
            }
            if auth_type == "sa" || looks_like_repair_body(&body) {
                run_sa_repair_flow(&mut session).await?;
            } else {
                session
                    .debug
                    .note(&format!("LOG FILE: {}", session.debug.path_str()));
                return Err(with_log_path(
                    classify_auth_error(&body),
                    &session.debug.path_str(),
                ));
            }
        }
        _ => {}
    }

    Ok(AccountAuthOutcome::Authenticated(session))
}

                                                                                               
async fn bootstrap_idmsa(session: &mut Session) -> Result<(), HistoryError> {
    let widget_key = session.target.widget_key();
    let redirect_uri = session.target.redirect_uri();
    if session.target == AuthTarget::AccountRegion {
        let _ = session
            .get_follow("https://account.apple.com/account/manage/section/information")
            .await?;
        let authorize_url = format!(
            "{AUTH}/authorize/signin?frame_id={fid}&skVersion=7&iframeId={fid}&client_id={widget_key}&redirect_uri={redirect_uri}&response_type=code&response_mode=web_message&state={fid}&authVersion=8.0.2",
            fid = session.frame_id
        );
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(
            "Accept",
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(
            "Referer",
            HeaderValue::from_static("https://account.apple.com/"),
        );
        let (status, _) = session
            .request_follow("GET", authorize_url, headers, None)
            .await?;
        session.debug.note(&format!(
            "account bootstrap authorize/signin -> {status} attrs={} sid={}",
            session.auth_attributes.as_ref().map_or(0, String::len),
            session.session_id.is_some()
        ));
        return Ok(());
    }
    let signin_path = match session.target {
        AuthTarget::PurchaseHistory => "%2F__logged_in%2Freportaproblem.apple.com",
        AuthTarget::AccountRegion => "%2Faccount%2Fmanage",
    };
                           
    let signin_url =
        format!("{IDMSA}/IDMSWebAuth/signin?appIdKey={widget_key}&path={signin_path}&rv=1");
    let (st, _) = session.get_follow(&signin_url).await?;
    session
        .debug
        .note(&format!("bootstrap IDMSWebAuth -> {st}"));

                                                                          
    let authorize_url = format!(
        "{AUTH}/authorize/signin?frame_id={fid}&language=RU-RU&skVersion=7&iframeId={fid}&client_id={widget_key}&redirect_uri={redirect_uri}&response_type=code&response_mode=web_message&state={fid}&authVersion=latest",
        fid = session.frame_id
    );
    let mut h = HeaderMap::new();
    h.insert(USER_AGENT, HeaderValue::from_static(UA));
    h.insert(
        "Accept",
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8",
        ),
    );
    h.insert("Referer", HeaderValue::from_static(IDMSA));
    let (st, _) = session
        .request_follow("GET", authorize_url, h, None)
        .await?;
    session.debug.note(&format!(
        "bootstrap authorize/signin -> {st} sid={} scnt={} aasp={}",
        session.session_id.is_some(),
        session.scnt.is_some(),
        session.cookies.map.contains_key("aasp")
    ));

                                                                               
    let (st, _) = session
        .get_follow(&format!("{AUTH}/signin?widgetKey={widget_key}"))
        .await?;
    session.debug.note(&format!(
        "bootstrap auth/signin -> {st} hc_bits={} challenge={}",
        session.hc_bits,
        session.hc_challenge.is_some()
    ));
    Ok(())
}

fn is_apple_auth_host(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("idmsa.apple.com")
        || lower.contains("appleid.apple.com")
        || lower.contains("gsa.apple.com")
        || lower.contains("account.apple.com")
        || lower.contains("reportaproblem.apple.com")
        || lower.contains("daiquiri")
        || lower.contains("itunes.apple.com")
}

                                                                                           
                                                                                
async fn arm_2fa_pending(session: &mut Session, email: &str) -> PendingHistoryAuth {
                                       
    let frozen_scnt = session.scnt.clone();
    let frozen_sid = session.session_id.clone();
    let frozen_attrs = session.auth_attributes.clone();
    session.debug.note(&format!(
        "arm_2fa freeze scnt={} sid={} attrs={} cookies={}",
        frozen_scnt.is_some(),
        frozen_sid.is_some(),
        frozen_attrs.is_some(),
        session.cookies.map.len()
    ));

    let attempts: Vec<(&str, HeaderMap)> = if session.target == AuthTarget::AccountRegion {
        vec![("account-full", session.arm_headers())]
    } else {
        vec![
            ("minimal-get", session.auth_headers_minimal_get()),
            ("minimal", session.auth_headers_minimal()),
            ("full", session.auth_headers(false)),
        ]
    };

    let mut body = Vec::new();
    let mut st_ok = 0u16;
    for (label, h) in attempts {
                                                     
        session.scnt = frozen_scnt.clone();
        session.session_id = frozen_sid.clone();
        session.auth_attributes = frozen_attrs.clone();

                                                                                        
        let (st, hdrs, raw, _) = session
            .request_once("GET", &format!("{AUTH}"), h, None, false)
            .await
            .unwrap_or((0, HeaderMap::new(), Vec::new(), None));
        session.debug.note(&format!(
            "GET /auth ({label}) -> {st} body={}",
            body_snippet(&raw, 200)
        ));
        if st == 200 || st == 204 {
            session.ingest_headers(&hdrs, true);
            st_ok = st;
            body = raw;
            break;
        }
        body = raw;
        st_ok = st;
    }

                                                                                           
    if st_ok != 200 && st_ok != 204 {
        session.scnt = frozen_scnt.clone();
        session.session_id = frozen_sid.clone();
        session.auth_attributes = frozen_attrs.clone();
        session
            .debug
            .note("arm_2fa: GET /auth failed — session kept for resend/retry");
    }

    let info: JsonValue = serde_json::from_slice(&body).unwrap_or(json!({}));
    let phone_id = info
        .get("trustedPhoneNumbers")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.first())
        .and_then(|p| p.get("id"))
        .and_then(|id| id.as_i64().or_else(|| id.as_u64().map(|u| u as i64)));
                                                                                    
                                                                                       
                                                                         
    let no_trusted_devices = info
        .get("noTrustedDevices")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut prefer_phone = false;
    if no_trusted_devices {
        if let Some(pid) = phone_id {
            prefer_phone = trigger_phone_code(session, pid).await;
        }
    }
    session.debug.note(&format!(
        "arm_2fa phone_id={phone_id:?} no_trusted_devices={no_trusted_devices} prefer_phone={prefer_phone}"
    ));

                                                                          
    session.scnt = frozen_scnt.or(session.scnt.clone());
    session.session_id = frozen_sid.or(session.session_id.clone());
    if session.auth_attributes.is_none() {
        session.auth_attributes = frozen_attrs;
    }

    session.to_pending_with_phone(email, phone_id, prefer_phone)
}

                                             
async fn trigger_phone_code(session: &mut Session, phone_id: i64) -> bool {
    let body = json!({
        "phoneNumber": { "id": phone_id },
        "mode": "sms"
    });
    let bytes = body.to_string().into_bytes();
                                               
    for method in ["PUT", "POST"] {
        let (st, raw) = session
            .request_once(
                method,
                &format!("{AUTH}/verify/phone"),
                session.two_factor_headers(),
                Some(bytes.clone()),
                true,
            )
            .await
            .map(|(s, _h, b, _)| (s, b))
            .unwrap_or((0, Vec::new()));
        session.debug.note(&format!(
            "{method} verify/phone (sms id={phone_id}) -> {st} body={}",
            body_snippet(&raw, 160)
        ));
        if st == 200 || st == 204 {
            return true;
        }
    }
    false
}

                                                                                           
                        
pub async fn resend_history_2fa(
    pending: PendingHistoryAuth,
) -> Result<PendingHistoryAuth, HistoryError> {
    let debug = DebugLog::create();
    debug.phase("RESEND 2FA (pending session)");
    if pending.is_expired() {
        return Err(HistoryError::SessionExpired);
    }
    let mut session = Session::from_pending(&pending, debug)?;
    let email = pending.email.clone();
                                                        
    let new_pending = arm_2fa_pending(&mut session, &email).await;
    session
        .debug
        .note(&format!("LOG FILE: {}", session.debug.path_str()));
    Ok(new_pending)
}

pub async fn resend_region_2fa(
    pending: PendingHistoryAuth,
) -> Result<PendingHistoryAuth, HistoryError> {
    resend_history_2fa(pending).await
}

fn with_log_path(err: HistoryError, _log: &str) -> HistoryError {
                                                             
    err
}

                                                                                
                                                                  
                                                           
                          
                                                                                
                                                                  
async fn run_sa_repair_flow(session: &mut Session) -> Result<(), HistoryError> {
                                                                                      
    let widget_q = format!(
        "trustedWidgetDomain={}&widgetKey={WIDGET_KEY}&language=ru_RU",
        urlencoding_lite(IDMSA)
    );
    for host in ["https://appleid.apple.com", "https://account.apple.com"] {
        let url = format!("{host}/widget/account/repair?{widget_q}");
        let (st, _) = session.get_follow(&url).await.unwrap_or((0, Vec::new()));
        session.debug.note(&format!("repair widget {host} -> {st}"));
    }

                                                                  
    let (st, body) = appleid_json_get(
        session,
        "https://appleid.apple.com/account/manage/repair/options",
    )
    .await
    .unwrap_or((0, Vec::new()));
    let opts: JsonValue = serde_json::from_slice(&body).unwrap_or(json!({}));
    let attr = opts
        .get("repairAttribute")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    session
        .debug
        .note(&format!("repair/options st={st} attr={attr}"));

                                                                             
                                                                                  
    for path in [
        "https://appleid.apple.com/account/security/upgrade",
        "https://appleid.apple.com/account/security/upgrade/setuplater",
        "https://account.apple.com/account/security/upgrade",
        "https://account.apple.com/account/security/upgrade/setuplater",
    ] {
        let (pst, _) = appleid_json_get(session, path)
            .await
            .unwrap_or((0, Vec::new()));
        session.debug.note(&format!("SA path {path} -> {pst}"));
    }

                                  
    let (st2, body2) = appleid_json_get(
        session,
        "https://appleid.apple.com/account/manage/repair/options",
    )
    .await
    .unwrap_or((0, Vec::new()));
    let attr2 = serde_json::from_slice::<JsonValue>(&body2)
        .ok()
        .and_then(|v| {
            v.get("repairAttribute")
                .and_then(|a| a.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    session.debug.note(&format!(
        "repair/options after setuplater st={st2} attr={attr2}"
    ));

    post_repair_complete(session).await?;

                                                                                
                                                                                
    if session.session_token.is_none() {
        if let Some(rst) = session.repair_session_token.clone() {
            session
                .debug
                .note("SA repair: no X-Apple-Session-Token — using repair_session_token as grant");
            session.session_token = Some(rst);
        }
    }
    session.debug.note(&format!(
        "after SA repair token={} repair_tok={} myacinfo={}",
        session.session_token.is_some(),
        session.repair_session_token.is_some(),
        session.cookies.map.contains_key("myacinfo")
    ));
    Ok(())
}

fn repair_headers(session: &Session) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(USER_AGENT, HeaderValue::from_static(UA));
    h.insert(
        "Accept",
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(
        "Origin",
        HeaderValue::from_static("https://appleid.apple.com"),
    );
    h.insert(
        "Referer",
        HeaderValue::from_static("https://appleid.apple.com/"),
    );
    h.insert("X-Apple-Widget-Key", HeaderValue::from_static(WIDGET_KEY));
    h.insert("X-Apple-Repair-App-Id", HeaderValue::from_static(APP_ID));
    if let Some(scnt) = &session.scnt {
        if let Ok(v) = HeaderValue::from_str(scnt) {
            h.insert("scnt", v);
        }
    }
    if let Some(sid) = &session.session_id {
        if let Ok(v) = HeaderValue::from_str(sid) {
            h.insert("X-Apple-ID-Session-Id", v);
        }
    }
    if let Some(tok) = &session.session_token {
        if let Ok(v) = HeaderValue::from_str(tok) {
            h.insert("X-Apple-Session-Token", v);
        }
    }
    if let Some(c) = session.cookies.header_value() {
        if let Ok(v) = HeaderValue::from_str(&c) {
            h.insert(COOKIE, v);
        }
    }
    h
}

async fn appleid_json_get(
    session: &mut Session,
    url: &str,
) -> Result<(u16, Vec<u8>), HistoryError> {
    session
        .request_follow("GET", url.to_string(), repair_headers(session), None)
        .await
}

async fn post_repair_complete(session: &mut Session) -> Result<(), HistoryError> {
    let mut h = session.auth_headers(false);
    h.insert(
        "Accept",
        HeaderValue::from_static("application/json;charset=utf-8"),
    );
    if let Some(rst) = &session.repair_session_token {
        if let Ok(v) = HeaderValue::from_str(rst) {
            h.insert("X-Apple-Repair-Session-Token", v);
        }
    } else if let Some(tok) = &session.session_token {
                                                                        
        if let Ok(v) = HeaderValue::from_str(tok) {
            h.insert("X-Apple-Repair-Session-Token", v);
        }
    }
                                                                             
    let (st, body) = session
        .request_follow(
            "POST",
            format!("{AUTH}/repair/complete"),
            h,
            Some(Vec::new()),
        )
        .await?;
    session.debug.note(&format!(
        "repair/complete -> {st} token={} body_len={}",
        session.session_token.is_some(),
        body.len()
    ));
                                                       
    if session.session_token.is_none() {
        if let Ok(v) = serde_json::from_slice::<JsonValue>(&body) {
            for key in [
                "sessionToken",
                "session_token",
                "X-Apple-Session-Token",
                "grantCode",
            ] {
                if let Some(t) = v
                    .get(key)
                    .and_then(|x| x.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    session.session_token = Some(t.to_string());
                    session
                        .debug
                        .note(&format!("repair/complete body token from {key}"));
                    break;
                }
            }
        }
    }
    Ok(())
}

fn looks_like_repair_body(body: &[u8]) -> bool {
    let t = String::from_utf8_lossy(body).to_ascii_lowercase();
    t.contains("\"authtype\"") && t.contains("\"sa\"")
        || t.contains("hsa2_enrollment")
        || t.contains("repairattribute")
}

                                                                              
                                                            
async fn complete_after_auth(
    session: &mut Session,
    after_2fa: bool,
) -> Result<HistoryOutcome, HistoryError> {
    session.debug.phase(if after_2fa {
        "HANDOFF after 2FA"
    } else {
        "HANDOFF after password/repair"
    });
                                                                                           
    if session.session_token.is_none() {
        if let Some(rst) = session.repair_session_token.clone() {
            session.session_token = Some(rst);
        }
    }
    session.debug.note(&format!(
        "cookies={} scnt={} sid={} token={} repair_tok={} myacinfo={}",
        session.cookies.map.len(),
        session.scnt.is_some(),
        session.session_id.is_some(),
        session.session_token.is_some(),
        session.repair_session_token.is_some(),
        session.cookies.map.contains_key("myacinfo")
    ));

                                                                             
    let scnt_q = session.scnt.clone().unwrap_or_default();
    let grant = session
        .session_token
        .clone()
        .or_else(|| session.repair_session_token.clone())
        .unwrap_or_default();
    if grant.is_empty() {
        if !session.cookies.map.contains_key("myacinfo") {
            session
                .debug
                .note(&format!("LOG FILE: {}", session.debug.path_str()));
            return Err(HistoryError::SessionExpired);
        }
        session
            .debug
            .note("trust returned myacinfo without grant token; continue cookie-first handoff");
    } else {
        let form = format!(
            "rememberMe=true&grantCode={grant}&iframeId={fid}&requestUri=%2Fsignin&appIdKey={WIDGET_KEY}&language=RU-RU&path=%2F__logged_in%2Freportaproblem.apple.com&scnt={scnt}",
            grant = urlencoding_lite(&grant),
            fid = urlencoding_lite(&session.frame_id),
            scnt = urlencoding_lite(&scnt_q),
        );
        let (grant_st, _) = session
            .post_form_follow(&format!("{IDMSA}/IDMSWebAuth/signin"), &form)
            .await
            .unwrap_or((0, Vec::new()));
        session.debug.note(&format!(
            "IDMSWebAuth grant handoff -> {grant_st} myacinfo={}",
            session.cookies.map.contains_key("myacinfo")
        ));
    }

                                                                           
    let _ = session
        .get_follow("https://daiquiri-ext.itunes.apple.com/__logged_in/reportaproblem.apple.com")
        .await;
    let _ = session.get_follow(&format!("{RAP_ORIGIN}/")).await;

    match fetch_purchases_from_rap(session, HISTORY_PAGE_BATCHES, None, None).await {
        Ok(success) => Ok(HistoryOutcome::Apps(success)),
        Err(e) => {
            session.debug.note(&format!(
                "fetch_purchases FAILED after_2fa={after_2fa}: {e:?}"
            ));
            session
                .debug
                .note(&format!("LOG FILE: {}", session.debug.path_str()));
            Err(if after_2fa {
                HistoryError::Message(
                    "Код принят, но историю не удалось открыть. Нажмите «Загрузить историю» ещё раз."
                        .into(),
                )
            } else {
                                                                                          
                HistoryError::Message(
                    "Нажмите «Обновить» в истории. Если Apple попросит код — введите его с iPhone."
                        .into(),
                )
            })
        }
    }
}

                                                                                
                                                                                        
                                                                                         
fn is_rap_ephemeral_cookie(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "dqsid" || n == "user-context" || n == "jsessionid" || n.starts_with("selfserv_")
}

                                                                                     
fn strip_rap_ephemeral_cookies(cookies: &HashMap<String, String>) -> HashMap<String, String> {
    cookies
        .iter()
        .filter(|(k, _)| !is_rap_ephemeral_cookie(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

                                                                             
   
                                                                                          
                                                                             
async fn fetch_purchases_from_rap(
    session: &mut Session,
    max_batches: usize,
    start_batch: Option<String>,
    reuse_creds: Option<(String, String)>,
) -> Result<HistorySuccess, HistoryError> {
    let max_batches = max_batches.max(1).min(50);
    let start_batch = start_batch.filter(|s| !s.trim().is_empty());

    let (dsid, xsrf) = if let Some((d, x)) = reuse_creds
        .filter(|(d, x)| !d.trim().is_empty() && !x.trim().is_empty())
        .filter(|_| start_batch.is_some())
    {
        session.debug.note(&format!(
            "reuse RAP creds (skip api/login) dsid_len={} xsrf_len={} start_batch={}",
            d.len(),
            x.len(),
            start_batch.as_deref().unwrap_or("-")
        ));
        (d, x)
    } else {
                                                                                                
                                                                                           
        let (st, body) = {
            let mut login_headers = HeaderMap::new();
            login_headers.insert(USER_AGENT, HeaderValue::from_static(UA));
            login_headers.insert(
                "Accept",
                HeaderValue::from_static("application/json, text/plain, */*"),
            );
            login_headers.insert(
                "Accept-Language",
                HeaderValue::from_static("ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7"),
            );
            login_headers.insert("X-Apple-Rap2-Api", HeaderValue::from_static(RAP_API));
            login_headers.insert(
                "Referer",
                HeaderValue::from_static("https://reportaproblem.apple.com/"),
            );
            let (st1, body1) = session
                .request_follow(
                    "GET",
                    format!("{RAP_ORIGIN}/api/login"),
                    login_headers,
                    None,
                )
                .await?;
            if st1 == 200 {
                (st1, body1)
            } else {
                session.debug.note(&format!(
                    "api/login first try st={st1} body={}",
                    body_snippet(&body1, 180)
                ));
                                                                
                let mut h2 = HeaderMap::new();
                h2.insert("Accept", HeaderValue::from_static("application/json"));
                h2.insert("X-Apple-Rap2-Api", HeaderValue::from_static(RAP_API));
                session
                    .request_follow("GET", format!("{RAP_ORIGIN}/api/login"), h2, None)
                    .await?
            }
        };

        let looks_html = body.starts_with(b"<")
            || body
                .windows(9)
                .any(|w| w.eq_ignore_ascii_case(b"<!doctype"))
            || body.windows(5).any(|w| w.eq_ignore_ascii_case(b"<html"));
        if st == 401 || st == 403 || st == 400 || st >= 500 || looks_html {
            session.debug.note(&format!(
                "api/login FAILED st={st} html={looks_html} body={}",
                body_snippet(&body, 200)
            ));
            return Err(HistoryError::SessionExpired);
        }

        let login: JsonValue = serde_json::from_slice(&body).map_err(|_| {
            HistoryError::Message("Не удалось загрузить историю. Повторите попытку.".into())
        })?;

        let dsid = login
            .get("dsid")
            .and_then(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .filter(|s| !s.is_empty())
            .ok_or(HistoryError::SessionExpired)?;
        let xsrf = login
            .get("token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if xsrf.is_empty() {
            return Err(HistoryError::SessionExpired);
        }
        session.debug.note(&format!(
            "api/login OK dsid={dsid} max_batches={max_batches} start_batch={}",
            start_batch.as_deref().unwrap_or("-")
        ));
        (dsid, xsrf)
    };

    match purchase_search_pages(session, max_batches, start_batch, &dsid, &xsrf).await {
        Ok(mut success) => {
            success.rap_dsid = Some(dsid);
            success.rap_xsrf = Some(xsrf);
            Ok(success)
        }
        Err(e) => {
                                                                                
            session
                .debug
                .note(&format!("purchase_search_pages failed: {e:?}"));
            Err(e)
        }
    }
}

async fn purchase_search_pages(
    session: &mut Session,
    max_batches: usize,
    start_batch: Option<String>,
    dsid: &str,
    xsrf: &str,
) -> Result<HistorySuccess, HistoryError> {
    let mut apps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut batch_id: Option<String> = start_batch.filter(|s| !s.trim().is_empty());
    let mut next_batch_id: Option<String> = None;
    let mut has_more = false;

    for page in 0..max_batches {
        let body = if let Some(bid) = &batch_id {
            json!({ "batchId": bid, "dsid": dsid })
        } else {
            json!({ "dsid": dsid })
        };
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        headers.insert("X-Apple-Rap2-Api", HeaderValue::from_static(RAP_API));
        if let Ok(v) = HeaderValue::from_str(xsrf) {
            headers.insert("X-Apple-Xsrf-Token", v);
        }
        headers.insert("Origin", HeaderValue::from_static(RAP_ORIGIN));
        headers.insert(
            "Referer",
            HeaderValue::from_static("https://reportaproblem.apple.com/"),
        );
        let (st, raw) = session
            .request_follow(
                "POST",
                format!("{RAP_ORIGIN}/api/purchase/search"),
                headers,
                Some(body.to_string().into_bytes()),
            )
            .await?;
        if st >= 400 {
            session.debug.note(&format!(
                "purchase/search HTTP {st} page={page} body={}",
                body_snippet(&raw, 160)
            ));
            if apps.is_empty() {
                                                                    
                if st == 401 || st == 403 || st == 400 {
                    return Err(HistoryError::SessionExpired);
                }
                return Err(HistoryError::Message(
                    "Не удалось получить историю покупок. Повторите попытку.".into(),
                ));
            }
            break;
        }
        let data: JsonValue = serde_json::from_slice(&raw).unwrap_or(json!({}));
        let before = apps.len();
        extract_purchases(&data, &mut apps, &mut seen);
        session.debug.note(&format!(
            "purchase/search page={} +{} total={}",
            page + 1,
            apps.len() - before,
            apps.len()
        ));
        let next = data
            .get("nextBatchId")
            .or_else(|| data.get("nextBatchID"))
            .or_else(|| data.get("next_batch_id"))
            .and_then(|v| match v {
                JsonValue::String(s) => Some(s.clone()),
                JsonValue::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        session.debug.note(&format!(
            "page={} nextBatchId_len={} keys={:?}",
            page + 1,
            next.len(),
            data.as_object()
                .map(|o| o.keys().take(12).cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        ));
        if next.is_empty() {
            next_batch_id = None;
            has_more = false;
            break;
        }
        next_batch_id = Some(next.clone());
        has_more = true;
        batch_id = Some(next);
    }

    session.debug.note(&format!(
        "DONE apps={} has_more={has_more} next={}",
        apps.len(),
        next_batch_id.as_deref().unwrap_or("-")
    ));
    Ok(HistorySuccess {
        apps,
        cookies: session.cookies.map.clone(),
        next_batch_id,
        has_more,
        rap_dsid: None,
        rap_xsrf: None,
    })
}

                                                                     
   
                                                                                          
                                                                               
pub async fn fetch_history_with_cookies(
    cookies: &HashMap<String, String>,
    max_batches: usize,
    start_batch: Option<String>,
    rap_creds: Option<(String, String)>,
) -> Result<HistorySuccess, HistoryError> {
    if cookies.is_empty() {
        return Err(HistoryError::SessionExpired);
    }
    let debug = DebugLog::create();
    debug.phase("COOKIE SESSION");
    let start_batch_owned = start_batch.filter(|s| !s.trim().is_empty());
    let has_reuse = rap_creds
        .as_ref()
        .map(|(d, x)| !d.trim().is_empty() && !x.trim().is_empty())
        .unwrap_or(false)
        && start_batch_owned.is_some();

                                                                                        
    if has_reuse {
        let mut session = Session::new(debug.clone())?;
                                                                         
        session.cookies = CookieJar::from_map(cookies.clone());
        let _ = session.get_follow(&format!("{RAP_ORIGIN}/")).await;
        match fetch_purchases_from_rap(
            &mut session,
            max_batches,
            start_batch_owned.clone(),
            rap_creds.clone(),
        )
        .await
        {
            Ok(mut success) => {
                success.cookies = {
                    let mut merged = cookies.clone();
                    for (k, v) in success.cookies {
                        merged.insert(k, v);
                    }
                    merged
                };
                return Ok(success);
            }
            Err(e) => {
                debug.note(&format!(
                    "reuse RAP creds failed ({e:?}) — fall back to stripped cookie re-login"
                ));
            }
        }
    }

                                                                                           
    let auth_cookies = strip_rap_ephemeral_cookies(cookies);
    if auth_cookies.is_empty() || !auth_cookies.contains_key("myacinfo") {
        return Err(HistoryError::SessionExpired);
    }
    debug.note(&format!(
        "cookie re-login: kept={} stripped_ephemeral={}",
        auth_cookies.len(),
        cookies.len().saturating_sub(auth_cookies.len())
    ));
    let mut session = Session::new(debug)?;
    session.cookies = CookieJar::from_map(auth_cookies.clone());
                                                                     
    let _ = session
        .get_follow("https://daiquiri-ext.itunes.apple.com/__logged_in/reportaproblem.apple.com")
        .await;
    let _ = session.get_follow(&format!("{RAP_ORIGIN}/")).await;
    let mut success =
        fetch_purchases_from_rap(&mut session, max_batches, start_batch_owned, None).await?;
                                                                 
    success.cookies = {
        let mut merged = auth_cookies;
                                                                                   
        for (k, v) in success.cookies {
            merged.insert(k, v);
        }
                                                                                          
        for (k, v) in cookies {
            if !is_rap_ephemeral_cookie(k) && !merged.contains_key(k) {
                merged.insert(k.clone(), v.clone());
            }
        }
        merged
    };
    Ok(success)
}

                                                                                       
#[derive(Debug, Clone)]
pub struct KeepaliveResult {
    pub cookies: HashMap<String, String>,
    pub dsid: Option<String>,
    pub xsrf: Option<String>,
}

                                                                    
                                                                                        
pub async fn refresh_history_session(cookies: &HashMap<String, String>) -> Option<KeepaliveResult> {
    if cookies.is_empty() {
        return None;
    }
    let debug = DebugLog::create();
    debug.phase("KEEPALIVE");
                                                                        
    let auth_cookies = strip_rap_ephemeral_cookies(cookies);
    if !auth_cookies.contains_key("myacinfo") {
        return None;
    }
    let mut session = Session::new(debug).ok()?;
    session.cookies = CookieJar::from_map(auth_cookies.clone());
    let _ = session
        .get_follow("https://daiquiri-ext.itunes.apple.com/__logged_in/reportaproblem.apple.com")
        .await;
    let _ = session.get_follow(&format!("{RAP_ORIGIN}/")).await;
    let mut login_headers = HeaderMap::new();
    login_headers.insert(USER_AGENT, HeaderValue::from_static(UA));
    login_headers.insert(
        "Accept",
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    login_headers.insert("X-Apple-Rap2-Api", HeaderValue::from_static(RAP_API));
    login_headers.insert("Origin", HeaderValue::from_static(RAP_ORIGIN));
    login_headers.insert(
        "Referer",
        HeaderValue::from_static("https://reportaproblem.apple.com/"),
    );
    let (st, body) = session
        .request_follow(
            "GET",
            format!("{RAP_ORIGIN}/api/login"),
            login_headers,
            None,
        )
        .await
        .ok()?;
    if st != 200 {
        return None;
    }
    let login: JsonValue = serde_json::from_slice(&body).ok()?;
    let dsid = login
        .get("dsid")
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        })
        .filter(|s| !s.is_empty());
    if dsid.is_none() {
        return None;
    }
    let xsrf = login
        .get("token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
                                                                                   
    let mut merged = auth_cookies;
    for (k, v) in session.cookies.map {
        merged.insert(k, v);
    }
    Some(KeepaliveResult {
        cookies: merged,
        dsid,
        xsrf,
    })
}

                                                                                
   
                                                                      
                                                              
                                                                       
                                                                                    
async fn verify_2fa(
    session: &mut Session,
    code: &str,
    phone_id: Option<i64>,
    prefer_phone: bool,
) -> Result<(), HistoryError> {
    session.debug.phase("VERIFY 2FA");
    let frozen_scnt = session.scnt.clone();
    let frozen_sid = session.session_id.clone();
    let frozen_attrs = session.auth_attributes.clone();
    let phone_id = phone_id.or(Some(1));                            
    session.debug.note(&format!(
        "code_len={} digits_ok={} scnt_len={} sid_len={} phone_id={:?} prefer_phone={prefer_phone} attrs={} (no pre-verify GET /auth)",
        code.len(),
        code.chars().all(|c| c.is_ascii_digit()),
        frozen_scnt.as_ref().map(|s| s.len()).unwrap_or(0),
        frozen_sid.as_ref().map(|s| s.len()).unwrap_or(0),
        phone_id,
        frozen_attrs.is_some(),
    ));

    let code_body = json!({ "securityCode": { "code": code } });
    let code_bytes = code_body.to_string().into_bytes();

                                                                                  
                                                                                     
    enum Attempt {
        PhoneSms,
        Device,
    }
    let attempt = if prefer_phone {
        Attempt::PhoneSms
    } else {
        Attempt::Device
    };

    session.scnt = frozen_scnt.clone();
    session.session_id = frozen_sid.clone();
    session.auth_attributes = frozen_attrs.clone();

    let (label, url, body) = match attempt {
        Attempt::Device => (
            "trusteddevice",
            format!("{AUTH}/verify/trusteddevice/securitycode"),
            code_bytes,
        ),
        Attempt::PhoneSms => {
            let pid = phone_id.unwrap_or(1);
            (
                "phone-sms",
                format!("{AUTH}/verify/phone/securitycode"),
                json!({
                    "phoneNumber": { "id": pid },
                    "securityCode": { "code": code },
                    "mode": "sms"
                })
                .to_string()
                .into_bytes(),
            )
        }
    };

    let (st, headers, raw, _) = session
        .request_once(
            "POST",
            &url,
            session.two_factor_headers(),
            Some(body),
            false,
        )
        .await?;
    session.debug.note(&format!(
        "POST verify/{label} -> {st} body={}",
        body_snippet(&raw, 200)
    ));

    if st == 200 || st == 204 {
        session.ingest_headers(&headers, true);
        session.debug.note(&format!(
            "verify OK via {label} token={} scnt={} sid={}",
            session.session_token.is_some(),
            session.scnt.is_some(),
            session.session_id.is_some()
        ));
        return trust_after_2fa(session).await;
    }

    session.scnt = frozen_scnt;
    session.session_id = frozen_sid;
    session.auth_attributes = frozen_attrs;
    session
        .debug
        .note("verify_2fa failed — freeze restored, pending kept");
    if st == 0 || st == 429 || st >= 500 {
        Err(HistoryError::Temporary(
            "Apple временно не отвечает на проверку кода. Подождите минуту и отправьте тот же код ещё раз.".into(),
        ))
    } else {
        Err(HistoryError::Invalid2fa)
    }
}

                                                                                  
async fn trust_after_2fa(session: &mut Session) -> Result<(), HistoryError> {
    let (st_t, _, _, _) = session
        .request_once(
            "GET",
            &format!("{AUTH}/2sv/trust"),
            session.two_factor_headers(),
            None,
            true,
        )
        .await
        .unwrap_or((0, HeaderMap::new(), Vec::new(), None));
    session.debug.note(&format!(
        "GET 2sv/trust -> {st_t} token={}",
        session.session_token.is_some()
    ));
    let (st_t2, _, _, _) = session
        .request_once(
            "POST",
            &format!("{AUTH}/2sv/trust"),
            session.two_factor_headers(),
            Some(b"{}".to_vec()),
            true,
        )
        .await
        .unwrap_or((0, HeaderMap::new(), Vec::new(), None));
    session.debug.note(&format!(
        "POST 2sv/trust -> {st_t2} token={}",
        session.session_token.is_some()
    ));
    Ok(())
}

fn looks_like_2fa_body(body: &[u8]) -> bool {
    let t = String::from_utf8_lossy(body).to_ascii_lowercase();
    t.contains("hsa2")
        || t.contains("two-factor")
        || t.contains("twofactor")
        || t.contains("second factor")
        || t.contains("securitycode")
        || t.contains("authtype")
}

#[cfg(test)]
mod tests {
    use super::{AccountAuthSession, AuthTarget, HistoryError, ACCOUNT_WIDGET_KEY};
    use std::collections::HashMap;

    #[test]
    fn retryable_2fa_errors_keep_the_frozen_session() {
        assert!(HistoryError::Invalid2fa.keep_pending());
        assert!(HistoryError::Temporary("Apple недоступен".into()).keep_pending());
        assert!(!HistoryError::SessionExpired.keep_pending());
        assert!(HistoryError::SessionExpired.clear_pending());
    }

    #[test]
    fn account_auth_snapshot_round_trips_all_store_metadata() {
        let snapshot = AccountAuthSession {
            cookies: HashMap::from([("myacinfo".into(), "cookie".into())]),
            scnt: Some("scnt".into()),
            session_id: Some("session-id".into()),
            session_token: Some("session-token".into()),
            repair_session_token: Some("repair-token".into()),
            auth_attributes: Some("attributes".into()),
        };

        let encoded = serde_json::to_vec(&snapshot).unwrap();
        let decoded: AccountAuthSession = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn region_auth_uses_account_apple_oauth_context() {
        assert_eq!(AuthTarget::AccountRegion.widget_key(), ACCOUNT_WIDGET_KEY);
        assert_eq!(
            AuthTarget::AccountRegion.redirect_uri(),
            "https://account.apple.com"
        );
        assert_ne!(
            AuthTarget::AccountRegion.widget_key(),
            AuthTarget::PurchaseHistory.widget_key()
        );
    }
}

fn classify_auth_error(body: &[u8]) -> HistoryError {
    let text = String::from_utf8_lossy(body);
    let lower = text.to_ascii_lowercase();
    if lower.contains("locked") || lower.contains("заблок") {
        return HistoryError::AccountLocked;
    }
    if looks_like_2fa_body(body) {
                                                                                                
                                                               
        if !lower.contains("password") && !lower.contains("-20101") {
                                                                               
            return HistoryError::Message(
                "Нужен код двухфакторной аутентификации. Введите 6 цифр с iPhone/Mac.".into(),
            );
        }
    }
    if lower.contains("incorrect")
        || lower.contains("invalid")
        || lower.contains("password")
        || lower.contains("-20101")
        || lower.contains("-20209")
        || lower.contains("неверн")
    {
        if lower.contains("20209") && lower.contains("lock") {
            return HistoryError::AccountLocked;
        }
        return HistoryError::WrongPassword;
    }
    if let Ok(v) = serde_json::from_slice::<JsonValue>(body) {
        if let Some(svc) = v.get("serviceErrors").and_then(|a| a.as_array()) {
            for e in svc {
                let code = e.get("code").and_then(|c| c.as_str()).unwrap_or("");
                let msg = e.get("message").and_then(|c| c.as_str()).unwrap_or("");
                let ml = msg.to_ascii_lowercase();
                if code == "-20209" || ml.contains("locked") {
                    return HistoryError::AccountLocked;
                }
                if code == "-20101" || ml.contains("password") || ml.contains("incorrect") {
                    return HistoryError::WrongPassword;
                }
            }
        }
    }
    HistoryError::WrongPassword
}

fn extract_purchases(
    data: &JsonValue,
    apps: &mut Vec<HistoryApp>,
    seen: &mut std::collections::HashSet<String>,
) {
    let purchases = data
        .get("purchases")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| {
            for k in ["data", "result", "body"] {
                if let Some(arr) = data
                    .get(k)
                    .and_then(|inner| inner.get("purchases"))
                    .and_then(|v| v.as_array())
                {
                    return Some(arr.clone());
                }
            }
            None
        })
        .unwrap_or_default();

    for pur in purchases {
        let pdate = pur
            .get("purchaseDate")
            .or_else(|| pur.get("invoiceDate"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let total = pur
            .get("estimatedTotalAmount")
            .or_else(|| pur.get("invoiceAmount"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let plis = pur
            .get("plis")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for pli in plis {
            let adam = pli
                .get("adamId")
                .or_else(|| pli.get("adamID"))
                .map(|v| match v {
                    JsonValue::String(s) => s.clone(),
                    JsonValue::Number(n) => n.to_string(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            if adam.is_empty() || !seen.insert(adam.clone()) {
                continue;
            }
            let lc = pli.get("localizedContent").cloned().unwrap_or(json!({}));
            let name = lc
                .get("nameForDisplay")
                .and_then(|v| v.as_str())
                .or_else(|| pli.get("title").and_then(|v| v.as_str()))
                .unwrap_or("")
                .trim()
                .to_string();
            let artist = lc
                .get("detailForDisplay")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let artwork = lc
                .get("artworkURL")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let date = pli
                .get("pliDate")
                .and_then(|v| v.as_str())
                .unwrap_or(&pdate)
                .to_string();
            let free = pli
                .get("isFreePurchase")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let price = pli
                .get("amountPaid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    if !total.is_empty() {
                        total.clone()
                    } else if free {
                        "Бесплатно".into()
                    } else {
                        String::new()
                    }
                });
            apps.push(HistoryApp {
                adam_id: adam,
                name: if name.is_empty() {
                    "Приложение".into()
                } else {
                    name
                },
                artist,
                artwork,
                price,
                date,
            });
        }
    }
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ─── Второй метод истории: ipatool list-purchases (DAAP) ─────────────────────
//
// Использует живую ipatool-сессию (Store login через Signum) и `list-purchases`
// из ipatool ≥2.4.1: покупочная история через приватный DAAP-протокол Apple,
// SAP-подписанный, без отдельной 2FA. Старый idmsa-метод остаётся фолбэком.

/// Максимальное число страниц по 100 приложений (Apple ограничивает страницу).
const IPATOOL_HISTORY_MAX_PAGES: usize = 20;

#[derive(Debug, Deserialize)]
struct IpatoolPurchasesOutput {
    #[serde(default)]
    apps: Vec<IpatoolPurchasesApp>,
    #[serde(default)]
    total_count: i64,
    #[serde(default)]
    page: i64,
    #[serde(default)]
    count: i64,
}

#[derive(Debug, Deserialize)]
struct IpatoolPurchasesApp {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    #[serde(rename = "bundleID")]
    bundle_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    #[serde(rename = "purchaseDate")]
    purchase_date: String,
}

fn parse_ipatool_purchases_output(raw: &str) -> Option<Vec<HistoryApp>> {
    // zerolog пишет одну JSON-строку на событие; интересующая нас содержит
    // массив "apps" c объектами {"id":..,"bundleID":..,"name":..}.
    let mut apps = Vec::new();
    let mut saw_any = false;
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(value) = serde_json::from_str::<JsonValue>(line) else {
            continue;
        };
        let Some(list) = value.get("apps").and_then(|v| v.as_array()) else {
            continue;
        };
        saw_any = true;
        for item in list {
            let Some(object) = item.as_object() else {
                continue;
            };
            let id = object
                .get("id")
                .and_then(|v| v.as_i64())
                .unwrap_or_default();
            if id <= 0 {
                continue;
            }
            // Дата приходит как RFC3339 или unix-число.
            let raw_date = object
                .get("purchaseDate")
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .or_else(|| v.as_i64().map(|n| n.to_string()))
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let name = object
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let bundle = object
                .get("bundleID")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if apps.iter().any(|a: &HistoryApp| a.adam_id == id.to_string()) {
                continue;
            }
            apps.push(HistoryApp {
                adam_id: id.to_string(),
                name: if name.is_empty() {
                    if bundle.is_empty() {
                        "Приложение".into()
                    } else {
                        bundle
                    }
                } else {
                    name
                },
                artist: String::new(),
                artwork: String::new(),
                price: "Бесплатно".into(),
                date: raw_date,
            });
        }
    }
    if saw_any {
        Some(apps)
    } else {
        None
    }
}

/// История покупок через `ipatool list-purchases` (v4, DAAP).
/// `program` — путь к ipatool, `keychain` — ключ шифрования сессии.
/// Возвращает `None`, когда метод неприменим (нет сессии/нет команды у бинаря),
/// чтобы вызывающий код ушёл в старый idmsa-фолбэк.
pub async fn fetch_history_via_ipatool(
    program: &str,
    keychain: &crate::IpatoolKeychain,
    debug: &(dyn Fn(&str) + Send + Sync),
) -> Option<Vec<HistoryApp>> {
    let mut all: Vec<HistoryApp> = Vec::new();
    for page in 1..=IPATOOL_HISTORY_MAX_PAGES {
        let mut args = crate::ipatool_args(keychain);
        args.extend([
            "--format".into(),
            "json".into(),
            "list-purchases".into(),
            "--page".into(),
            page.to_string(),
            "--max-results".into(),
            "100".into(),
        ]);
        let raw = crate::run_process_raw_error(program, &args, Duration::from_secs(120), None)
            .await
            .ok()?;
        if let Some(apps) = parse_ipatool_purchases_output(&raw) {
            let before = all.len();
            for app in apps {
                if !all
                    .iter()
                    .any(|a: &HistoryApp| a.adam_id == app.adam_id)
                {
                    all.push(app);
                }
            }
            let added = all.len() - before;
            debug(&format!(
                "history-ipatool page={page} added={added} total={}",
                all.len()
            ));
            // Полная страница = возможно есть ещё; короткая — конец истории.
            if added < 100 {
                break;
            }
        } else {
            // Ответ без массива apps: либо ошибка, либо команда не поддерживается.
            debug(&format!(
                "history-ipatool page={page} unreadable: {}",
                raw.lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect::<String>()
            ));
            // Первая страница не удалась — метод в целом не сработал.
            if page == 1 {
                return None;
            }
            break;
        }
    }
    if all.is_empty() {
        None
    } else {
        Some(all)
    }
}

#[cfg(test)]
mod ipatool_history_tests {
    use super::parse_ipatool_purchases_output;

    #[test]
    fn parses_zerolog_apps_lines() {
        // ipatool >= 2.4.1 печатает zerolog-строку на событие; интересная
        // содержит "apps":[{...}] с полями id/bundleID/name.
        let raw = concat!(
            r#"{"level":"info","count":2,"totalCount":2,"page":1,"apps":[{"id":6739530834,"bundleID":"com.example.app","name":"Example","price":0,"purchaseDate":"2024-05-01T10:00:00Z"}]}"#,
            "\n",
            r#"{"level":"warn","message":"unrelated"}"#,
        );
        let apps = parse_ipatool_purchases_output(raw).expect("apps line must parse");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].adam_id, "6739530834");
        assert_eq!(apps[0].name, "Example");
        assert!(!apps[0].date.is_empty());
    }

    #[test]
    fn no_apps_array_means_none() {
        // Ошибки (нет сессии, bit bad keychain) не содержат "apps".
        assert!(parse_ipatool_purchases_output(
            r#"{"level":"error","error":"failed to get account"}"#
        )
        .is_none());
        assert!(parse_ipatool_purchases_output("plain text").is_none());
    }

    #[test]
    fn empty_name_falls_back_to_bundle() {
        let raw = r#"{"apps":[{"id":42,"bundleID":"com.foo.bar"}]}"#;
        let apps = parse_ipatool_purchases_output(raw).expect("apps");
        assert_eq!(apps[0].name, "com.foo.bar");
    }
}
