use regex::Regex;
use serde::Serialize;
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

use crate::{api_download_urls, api_http_client, api_urls, APP_BUILD, APP_VERSION};

static DIAG_ACTIVE: AtomicBool = AtomicBool::new(false);
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagStatus {
    pub active: bool,
    pub has_log: bool,
}

fn diag_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("diag");
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось подготовить запись: {e}"))?;
    Ok(dir)
}

fn flag_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(diag_dir(app)?.join("active"))
}

fn log_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(diag_dir(app)?.join("session.log"))
}

fn redact(input: &str) -> String {
    let mut text = input.to_string();
    static EMAIL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static TOKEN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static PASS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static CODE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static PATH_WIN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static PATH_UNIX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static UDID: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let email = EMAIL.get_or_init(|| {
        Regex::new(r"(?i)[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}").unwrap()
    });
    let token = TOKEN.get_or_init(|| {
        Regex::new(r"(?i)(bearer\s+|token[=:]\s*|passwordtoken[=:]\s*|cookie[=:]\s*)[^\s,&]+")
            .unwrap()
    });
    let pass = PASS.get_or_init(|| {
        Regex::new(r"(?i)(--password|-p|--keychain-passphrase|--auth-code|-a)\s+\S+")
            .unwrap()
    });
    let code = CODE.get_or_init(|| Regex::new(r"\b\d{6}\b").unwrap());
    let path_win = PATH_WIN.get_or_init(|| {
        Regex::new(r"(?i)([A-Z]:\\Users\\)[^\\]+").unwrap()
    });
    let path_unix = PATH_UNIX.get_or_init(|| {
        Regex::new(r"(/(?:Users|home)/)[^/\s]+").unwrap()
    });
    let udid = UDID.get_or_init(|| Regex::new(r"\b[0-9a-fA-F]{40}\b").unwrap());
    text = email.replace_all(&text, "[email]").into_owned();
    text = token.replace_all(&text, "${1}[hidden]").into_owned();
    text = pass.replace_all(&text, "$1 [hidden]").into_owned();
    text = code.replace_all(&text, "[code]").into_owned();
    text = path_win.replace_all(&text, "${1}[user]").into_owned();
    text = path_unix.replace_all(&text, "${1}[user]").into_owned();
    text = udid.replace_all(&text, "[id]").into_owned();
    static JSON_SECRET: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static PLIST_SECRET: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static KV_SECRET: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let json_secret = JSON_SECRET.get_or_init(|| {
        Regex::new(r#"(?i)"(password|passwordToken|accessToken|sessionToken|dsPersonId|dsid|myacinfo|cookie|name|firstName|lastName|fullName|phoneNumber|numberWithDialCode)"\s*:\s*"[^"]*""#).unwrap()
    });
    let plist_secret = PLIST_SECRET.get_or_init(|| {
        Regex::new(r"(?is)<key>(password|passwordToken|accessToken|dsPersonId|dsid|myacinfo|name|firstName|lastName)</key>\s*<string>[^<]*</string>").unwrap()
    });
    let kv_secret = KV_SECRET.get_or_init(|| {
        Regex::new(r"(?i)\b(passwordToken|accessToken|sessionToken|dsPersonId|dsid|myacinfo|guid)\s*[:=]\s*\S+").unwrap()
    });
    text = json_secret.replace_all(&text, r#""$1":"[hidden]""#).into_owned();
    text = plist_secret
        .replace_all(&text, "<key>$1</key><string>[hidden]</string>")
        .into_owned();
    text = kv_secret.replace_all(&text, "$1=[hidden]").into_owned();
    text
}

fn short_url(url: &str) -> String {
    let cut = url.split('?').next().unwrap_or(url);
    redact(cut)
}

fn body_preview(body: &[u8], max: usize) -> String {
    if body.is_empty() {
        return "-".into();
    }
    if body.starts_with(b"PK") {
        return format!("[zip {} bytes]", body.len());
    }
    let text = String::from_utf8_lossy(body);
    let flat: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c })
        .collect();
    clip(&redact(&flat), max)
}

/// Request + status + redacted body. No-op unless «запись для поддержки» is on.
pub fn http(method: &str, url: &str, status: u16, extra: &str, body: &[u8]) {
    if !is_active() {
        return;
    }
    let extra = extra.trim();
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!(" {extra}")
    };
    let own_api = api_download_urls(url).len() > 1;
    if own_api {
        note(
            "http",
            &format!(
                "<< {method} {} -> {status}{extra} body({}b)",
                short_url(url),
                body.len()
            ),
        );
        return;
    }
    let preview = body_preview(body, 5000);
    if status == 0 {
        note(
            "http",
            &format!(
                ">> {method} {}{extra} body({}b) {preview}",
                short_url(url),
                body.len()
            ),
        );
        return;
    }
    note(
        "http",
        &format!(
            "<< {method} {} -> {status}{extra} body({}b) {preview}",
            short_url(url),
            body.len()
        ),
    );
}

pub fn http_err(method: &str, url: &str, error: &str) {
    if !is_active() {
        return;
    }
    note(
        "http",
        &format!("{method} {} FAIL {}", short_url(url), redact(error)),
    );
}

pub fn proc_out(program: &str, exit: Option<i32>, output: &str) {
    if !is_active() {
        return;
    }
    let name = std::path::Path::new(program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(program);
    note(
        "proc",
        &format!(
            "{name} exit={} out({}c) {}",
            exit.map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
            output.chars().count(),
            clip(&redact(output), 6000)
        ),
    );
}

pub fn is_technical_fault(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("discarding cookies")
        || lower.contains("cookiejar")
        || lower.contains("illegal base64")
        || lower.contains("cannot unmarshal")
        || lower.contains("failed to get account")
        || lower.contains("failed to get item")
        || lower.contains("cannot load cookies")
        || (lower.contains("panic") && lower.contains("cookie"))
}

fn is_expected_notice(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("не удалось сменить страну") {
        return true;
    }
    if lower.contains("нет доступных версий") {
        return true;
    }
    if lower.contains("нет подтвержд") && lower.contains("верси") {
        return true;
    }
    if lower.contains("не найдено в app store")
        || lower.contains("удалили из магазина")
        || (lower.contains("недоступно") && lower.contains("регион"))
    {
        return true;
    }
    false
}

fn skip_auto(message: &str) -> bool {
    let text = message.trim();
    if text.is_empty() {
        return true;
    }
    if is_technical_fault(text) {
        return false;
    }
    let lower = text.to_lowercase();
    if lower.contains("отменено") || lower.contains("cancelled") {
        return true;
    }
    if lower.contains("signum_2fa_required") {
        return true;
    }
    if lower.contains("введите код") || (lower.contains("нужен код") && lower.contains("iphone")) {
        return true;
    }
    is_expected_notice(text)
}

fn clip(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

pub fn is_active() -> bool {
    DIAG_ACTIVE.load(Ordering::Relaxed)
}

pub fn on_startup(app: &AppHandle) {
    let manual = flag_path(app).ok().map(|p| p.is_file()).unwrap_or(false);
    let auto = !manual;
    let active = true;
    DIAG_ACTIVE.store(active, Ordering::Relaxed);
    AUTO_SESSION.store(auto, Ordering::Relaxed);
    if let Ok(path) = log_file_path(app) {
        if let Ok(mut slot) = LOG_PATH.lock() {
            *slot = Some(path);
        }
        // Keep an unsent previous session: exit uploads run in the background
        // and the process may close before the request finishes.
    }
    if active {
        retry_pending_uploads(app);
        note(
            "session",
            &format!(
                "start version={} build={} os={} запись={}",
                crate::APP_VERSION,
                crate::APP_BUILD,
                std::env::consts::OS,
                if manual { "вручную" } else { "автоматически" },
            ),
        );
    }
}

/// Идёт ли запись, начатая приложением самостоятельно.
static AUTO_SESSION: AtomicBool = AtomicBool::new(false);

pub fn note(kind: &str, msg: &str) {
    if !DIAG_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let path = {
        let slot = LOG_PATH.lock().ok();
        slot.and_then(|g| g.clone())
    };
    let Some(path) = path else {
        return;
    };
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("{ts} [{kind}] {}\n", redact(msg));
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn export_to(app: &AppHandle, dest: &str) -> Result<(), String> {
    let dest = dest.trim();
    if dest.is_empty() {
        return Err("Укажите файл для отчёта.".into());
    }
    let log = read_log(app);
    if log.trim().is_empty() {
        return Err("Сначала повторите проблему, затем сохраните отчёт.".into());
    }
    fs::write(dest, redact(&log)).map_err(|_| "Не удалось сохранить отчёт.".to_string())
}

fn manual_recording(app: &AppHandle) -> bool {
    flag_path(app).ok().map(|path| path.is_file()).unwrap_or(false)
}

pub fn status(app: &AppHandle) -> DiagStatus {
    let path = log_file_path(app).ok();
    let has_log = path
        .as_ref()
        .and_then(|p| fs::metadata(p).ok())
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    // The window only cares about a recording the person started. The
    // automatic session keeps writing, but it must not look like that button
    // is already pressed.
    DiagStatus {
        active: manual_recording(app),
        has_log,
    }
}

pub fn enable(app: &AppHandle) -> Result<(), String> {
    let flag = flag_path(app)?;
    let log = log_file_path(app)?;
    let _ = fs::write(&flag, b"1");
    let _ = fs::write(&log, b"");
    if let Ok(mut slot) = LOG_PATH.lock() {
        *slot = Some(log);
    }
    AUTO_SESSION.store(false, Ordering::Relaxed);
    DIAG_ACTIVE.store(true, Ordering::Relaxed);
    note(
        "session",
        &format!(
            "start version={} build={} os={}",
            APP_VERSION,
            APP_BUILD,
            std::env::consts::OS
        ),
    );
    Ok(())
}

fn disable(app: &AppHandle) {
    DIAG_ACTIVE.store(false, Ordering::Relaxed);
    if let Ok(flag) = flag_path(app) {
        let _ = fs::remove_file(flag);
    }
}

/// Manual recording is over. The silent automatic session keeps going.
fn resume_auto(app: &AppHandle) {
    if let Ok(flag) = flag_path(app) {
        let _ = fs::remove_file(flag);
    }
    AUTO_SESSION.store(true, Ordering::Relaxed);
    DIAG_ACTIVE.store(true, Ordering::Relaxed);
    if let Ok(path) = log_file_path(app) {
        if let Ok(mut slot) = LOG_PATH.lock() {
            *slot = Some(path);
        }
    }
}

fn read_log(app: &AppHandle) -> String {
    log_file_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_default()
}

fn post_log_blocking(body: serde_json::Value) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())?;
    for url in api_urls("logs.php") {
        match client.post(url).json(&body).send() {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) if !resp.status().is_server_error() => {
                return Err("Не удалось отправить отчёт.".into());
            }
            _ => {}
        }
    }
    Err("Не удалось отправить отчёт.".into())
}

async fn post_log_async(body: serde_json::Value) -> Result<(), String> {
    let client = api_http_client()?;
    for url in api_urls("logs.php") {
        match client.post(url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) if !resp.status().is_server_error() => {
                return Err("Не удалось отправить отчёт.".into());
            }
            _ => {}
        }
    }
    Err("Не удалось отправить отчёт.".into())
}

fn payload(app: &AppHandle) -> serde_json::Value {
    support_payload(
        &read_log(app),
        if AUTO_SESSION.load(Ordering::Relaxed) {
            "auto-session"
        } else {
            "recording"
        },
    )
}

fn support_payload(log: &str, source: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "support",
        "source": source,
        "platform": std::env::consts::OS,
        "version": APP_VERSION,
        "build": APP_BUILD,
        "log": redact(log),
    })
}

fn pending_source(path: &std::path::Path) -> &'static str {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("session-manual-"))
    {
        "recording"
    } else {
        "auto-session"
    }
}

fn retry_pending_uploads(app: &AppHandle) {
    let Ok(dir) = diag_dir(app) else { return };
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("pending") {
            continue;
        }
        let Ok(log) = fs::read_to_string(&path) else { continue };
        if log.trim().is_empty() {
            let _ = fs::remove_file(path);
            continue;
        }
        launch_pending_upload(path);
    }
}

fn launch_pending_upload(path: PathBuf) {
    let Ok(exe) = std::env::current_exe() else { return };
    let _ = std::process::Command::new(exe)
        .arg("--signum-upload-support")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Run in a detached copy of Signum so the upload outlives the UI process.
pub fn run_pending_upload_mode() -> bool {
    let mut args = std::env::args_os();
    let Some(arg) = args.nth(1) else { return false };
    if arg != "--signum-upload-support" {
        return false;
    }
    let Some(path) = args.next().map(PathBuf::from) else {
        return true;
    };
    let Ok(log) = fs::read_to_string(&path) else {
        return true;
    };
    let source = pending_source(&path);
    if !log.trim().is_empty() && post_log_blocking(support_payload(&log, source)).is_ok() {
        let _ = fs::remove_file(path);
    }
    true
}

// Automatic error reports are independent of "Запись для поддержки".
// They never read or write diag/active or session.log.
const AUTO_DEDUP_SECS: u64 = 900;
const AUTO_MAX_PER_HOUR: usize = 30;
static AUTO_RECENT: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());

/// Raw tool output kept for a short while so an automatic report can carry the
/// real English error even when the user only ever sees a friendly Russian
/// sentence. Without this, half of the reports read "Не удалось выполнить
/// операцию" with no way to tell what actually failed.
const RAW_KEEP_SECS: u64 = 120;
static LAST_RAW: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Called from the error formatters with the untouched process/HTTP output.
pub fn remember_raw(raw: &str) {
    let text = raw.trim();
    if text.is_empty() {
        return;
    }
    let text = clip(&redact(text), 3000);
    let now = now_secs();
    if let Ok(mut slot) = LAST_RAW.lock() {
        slot.retain(|(at, _)| now.saturating_sub(*at) < RAW_KEEP_SECS);
        if slot.iter().any(|(_, kept)| kept == &text) {
            return;
        }
        slot.push((now, text));
        while slot.len() > 8 {
            slot.remove(0);
        }
    }
}

fn recent_raw() -> Option<String> {
    let now = now_secs();
    let slot = LAST_RAW.lock().ok()?;
    slot.iter()
        .rev()
        .find(|(at, _)| now.saturating_sub(*at) < RAW_KEEP_SECS)
        .map(|(_, text)| text.clone())
}

/// Stable English label for an error, matched on the raw tool output *and* on
/// the Russian sentence the user sees. Reports carry the label instead of the
/// localized text: it reads the same for everyone and — unlike ipatool's JSON,
/// which embeds a wall-clock `time` — it is identical across occurrences, so
/// duplicates actually collapse instead of flooding the feed.
pub fn classify_en(text: &str) -> (&'static str, &'static str) {
    let t = text.to_lowercase();
    let has = |needle: &str| t.contains(needle);

    // --- Apple ID session storage (ipatool keyring) -----------------------
    if has("could not be found in the keyring")
        || has("errkeynotfound")
        || (has("failed to get account") && has("item not found"))
    {
        return (
            "KEYRING_ITEM_MISSING",
            "ipatool keyring has no 'account' item: the saved session was not restored into ~/.ipatool, or it sits in the other engine's layout (v2 keyring/account vs v3 account)",
        );
    }
    if has("keyunwrap") || has("integrity check failed") || has("illegal base64") {
        return (
            "KEYRING_DECRYPT_FAILED",
            "the keychain passphrase does not match the stored account file (key rotated, or the file was written by another install)",
        );
    }
    if has("keychain passphrase is required") || has("use the --keychain-passphrase") {
        return (
            "KEYRING_PASSPHRASE_MISSING",
            "ipatool was called without --keychain-passphrase",
        );
    }
    if has("cookiejar") || has("discarding cookies") || has("cannot load cookies") {
        return (
            "COOKIE_JAR_CORRUPT",
            "the ipatool cookie jar is in the wrong format (Netscape/object where an array is expected)",
        );
    }
    if has("not logged in") || has("вход в apple id не сохранился") {
        return (
            "NOT_LOGGED_IN",
            "ipatool reports no active session for this Apple ID",
        );
    }

    // --- Apple auth -------------------------------------------------------
    if has("password token") || has("сессия apple id для app store устарела") {
        return (
            "PASSWORD_TOKEN_EXPIRED",
            "the App Store passwordToken has expired: Apple requires a fresh sign-in (2FA may be asked)",
        );
    }
    if has("sap helper") || has("sapsigner") || has("0xc0000409") {
        return (
            "SAP_SIGNER_FAILED",
            "the SAP signer helper failed to produce X-Apple-ActionSignature",
        );
    }
    if has("anisette") || has("x-apple-i-md") || (has("adi") && has("fail")) {
        return (
            "ANISETTE_UNAVAILABLE",
            "anisette/ADI headers could not be produced (iCloud/AOSKit missing or blocked on this machine)",
        );
    }
    if has("неверный пароль") || has("bad login") || has("incorrect password") {
        return ("BAD_PASSWORD", "Apple rejected the Apple ID password");
    }
    if has("пароль не найден") || has("sign_ipa_auth_required") || has("нет сохранённого пароля") {
        return (
            "SAVED_PASSWORD_MISSING",
            "no Apple ID password is stored for signing: the credential is gone from the OS keyring",
        );
    }
    if has("сессия устарела") || has("сессия истекла") || has("session expired") || has("unauthorized")
    {
        return (
            "SESSION_EXPIRED",
            "the stored Apple ID session is no longer accepted by Apple",
        );
    }
    if has("сначала войдите в apple id") {
        return (
            "NO_ACTIVE_ACCOUNT",
            "the action needs a signed-in Apple ID but none is active",
        );
    }
    if has("не удалось войти в apple id") {
        return (
            "LOGIN_FAILED",
            "Apple ID sign-in failed (credentials, anisette or network)",
        );
    }
    if has("слишком много попыток") || has("too many") || has("rate limit") {
        return ("RATE_LIMITED", "Apple is throttling sign-in attempts");
    }
    if has("не удалось подготовить вход в apple id") {
        return (
            "AUTH_PREPARE_FAILED",
            "the sign-in prerequisites could not be prepared (anisette/SAP helper); see the raw output",
        );
    }

    // --- Store / purchases ------------------------------------------------
    if has("empty songlist") || has("нет в покупках") || has("нет доступных версий") {
        return (
            "NOT_PURCHASED",
            "this build is not in the Apple ID purchase history (Apple answers an empty songList)",
        );
    }
    if has("account not in store") || has("accountnotinstore") || has("storefront ()") {
        return (
            "STOREFRONT_MISMATCH",
            "the Apple ID is not attached to the storefront being queried",
        );
    }
    if has("не найдено в app store") || has("app not found") || has("no longer available") {
        return (
            "APP_NOT_IN_STORE",
            "the listing is gone from the App Store or hidden in this region",
        );
    }
    if has("исчерпал лимит приложений") || has("maximum number of apps") {
        return (
            "SIGNING_LIMIT_REACHED",
            "the free Apple ID hit its signed-app limit (usually 10)",
        );
    }

    // --- Device -----------------------------------------------------------
    if has("iphone не найден") || has("device not found") || has("no device found") {
        return (
            "DEVICE_NOT_FOUND",
            "no iPhone/iPad is reachable over USB (not connected, locked, or not trusted)",
        );
    }
    if has("не удалось установить доверие") || has("pairing") || has("trust") {
        return (
            "DEVICE_TRUST_FAILED",
            "pairing failed: the device did not confirm the Trust prompt",
        );
    }
    if has("applicationverificationfailed") || has("отклонил приложение при проверке") {
        return (
            "APP_VERIFICATION_FAILED",
            "the device refused the signed app (bad signature, entitlements or provisioning profile)",
        );
    }

    if has("is not part of the main app bundle identifier")
        || has("invalid bundle")
        || has("sign_ipa_bundle_invalid")
        || has("расширение с чужим идентификатором")
    {
        return (
            "BUNDLE_EXTENSION_MISMATCH",
            "the IPA carries an app extension whose bundle id is not a child of the main app id; Apple refuses to sign it unless the plugins are stripped",
        );
    }

    // --- Transport / misc --------------------------------------------------
    if has("timed out") || has("timeout") || has("не ответил вовремя") {
        return ("NETWORK_TIMEOUT", "the request timed out");
    }
    if has("connection refused")
        || has("connection reset")
        || has("could not resolve")
        || has("не удалось подключиться")
        || has("network error")
    {
        return ("NETWORK_UNREACHABLE", "the service could not be reached");
    }
    if has("не удалось выполнить поиск") {
        return ("SEARCH_FAILED", "the App Store search request failed");
    }
    if has("не удалось обработать ответ") || has("cannot unmarshal") || has("invalid json") {
        return (
            "RESPONSE_PARSE_FAILED",
            "a service reply could not be parsed",
        );
    }
    if has("no space left") || has("disk full") {
        return ("DISK_FULL", "not enough free disk space");
    }
    if has("permission denied") || has("access is denied") {
        return ("PERMISSION_DENIED", "the file or device access was denied");
    }
    if has("не удалось выполнить операцию") {
        return (
            "UNCLASSIFIED_OPERATION_FAILURE",
            "a tool exited with an error Signum has no rule for yet — see the raw output below",
        );
    }
    (
        "UNCLASSIFIED",
        "no matching rule — see the raw output below",
    )
}

/// Fold volatile parts (timestamps, ids, sizes) so repeats of the same fault
/// share one fingerprint.
fn dedup_key(source: &str, code: &str, message: &str) -> String {
    static VOLATILE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let volatile = VOLATILE.get_or_init(|| {
        Regex::new(r"(?i)\d{4}-\d{2}-\d{2}t[\d:.+\-z]+|\b[0-9a-f]{8,}\b|\d+").unwrap()
    });
    format!("{source}|{code}|{}", volatile.replace_all(message, "#"))
}

fn auto_fingerprint(source: &str, message: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    message.hash(&mut hasher);
    hasher.finish()
}

fn should_send_auto(fingerprint: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let Ok(mut recent) = AUTO_RECENT.lock() else {
        return false;
    };
    recent.retain(|(t, _)| now.saturating_sub(*t) < 3600);
    if recent
        .iter()
        .any(|(t, fp)| *fp == fingerprint && now.saturating_sub(*t) < AUTO_DEDUP_SECS)
    {
        return false;
    }
    if recent.len() >= AUTO_MAX_PER_HOUR {
        return false;
    }
    recent.push((now, fingerprint));
    true
}

pub fn report_error(source: &str, message: &str, detail: Option<&str>) {
    // Automatic reports are intentionally disabled: only automatic support
    // session logs are queued and sent in the server's 12-hour batch.
    let _ = (source, message, detail);
}

#[tauri::command]
pub fn report_auto_error(source: String, message: String, detail: Option<String>) {
    report_error(&source, &message, detail.as_deref());
}

pub async fn send_and_disable(app: &AppHandle) -> Result<(), String> {
    if !is_active() && read_log(app).trim().is_empty() {
        return Err("Запись не включена.".into());
    }
    let body = payload(app);
    post_log_async(body).await?;
    if let Ok(path) = log_file_path(app) {
        let _ = fs::remove_file(path);
    }
    resume_auto(app);
    Ok(())
}

pub fn on_exit(app: &AppHandle) {
    if !is_active() {
        return;
    }
    // Move this run out of the active file so the next launch can start a new
    // recording immediately. Failed uploads stay queued for the next launch.
    let Some(path) = log_file_path(app).ok() else {
        disable(app);
        return;
    };
    let source = if AUTO_SESSION.load(Ordering::Relaxed) {
        "auto"
    } else {
        "manual"
    };
    let pending = path.with_file_name(format!(
        "session-{source}-{}-{}.pending",
        now_secs(),
        std::process::id()
    ));
    if fs::rename(&path, &pending).is_err() {
        disable(app);
        return;
    }
    launch_pending_upload(pending);
    disable(app);
}

/// Что человек нажал в окне.
///
/// Без этого лог читался как поток запросов без причины: непонятно, сам ли
/// человек полез за версиями или это фоновая работа. Теперь у каждой цепочки
/// событий видно начало.
#[tauri::command]
pub fn log_ui_action(action: String, detail: Option<String>) {
    let detail = detail.unwrap_or_default();
    if detail.trim().is_empty() {
        note("ui", &action);
    } else {
        note("ui", &format!("{action} — {detail}"));
    }
}

#[tauri::command]
pub fn get_diag_status(app: AppHandle) -> DiagStatus {
    status(&app)
}

#[tauri::command]
pub fn start_diag_recording(app: AppHandle) -> Result<(), String> {
    enable(&app)?;
    tauri::process::restart(&app.env());
    #[allow(unreachable_code)]
    Ok(())
}

#[tauri::command]
pub async fn send_diag_log(app: AppHandle) -> Result<(), String> {
    send_and_disable(&app).await
}

/// «Остановить запись»: флаг/активность выключаются, файл остаётся —
/// пользователь ещё может отправить его или скачать позже.
#[tauri::command]
pub fn stop_diag_recording(app: AppHandle) -> Result<(), String> {
    resume_auto(&app);
    Ok(())
}

#[tauri::command]
pub fn export_diag_log(app: AppHandle, path: String) -> Result<(), String> {
    export_to(&app, &path)
}

#[cfg(test)]
mod tests {
    use super::{
        classify_en, dedup_key, is_technical_fault, pending_source, redact, skip_auto,
        support_payload,
    };

    #[test]
    fn automatic_recording_is_a_support_report() {
        let report = support_payload("session started", "auto-session");
        assert_eq!(report["kind"], "support");
        assert_eq!(report["source"], "auto-session");
        assert_eq!(report["log"], "session started");
        assert!(report.get("message").is_none());
    }

    #[test]
    fn pending_upload_preserves_manual_or_automatic_source() {
        assert_eq!(
            pending_source(std::path::Path::new("session-manual-123-4.pending")),
            "recording"
        );
        assert_eq!(
            pending_source(std::path::Path::new("session-auto-123-4.pending")),
            "auto-session"
        );
    }

    #[test]
    fn classifies_extension_bundle_mismatch_apart_from_login() {
        // Real report: this used to be filed as a sign-in failure, so users were
        // told to sign out and back in for a problem inside the IPA.
        let (code, text) = classify_en(
            "SIGN_IPA_DEV_LOGIN_FAILED: Invalid bundle: Extension AudioCatalogResources is not part of the main app bundle identifier: com.vk.AudioCatalogResources",
        );
        assert_eq!(code, "BUNDLE_EXTENSION_MISMATCH");
        assert!(text.contains("extension"), "{text}");
    }

    #[test]
    fn classifies_keyring_faults_in_english() {
        let (code, text) = classify_en(
            r#"{"level":"error","error":"failed to get account: failed to get item: The specified item could not be found in the keyring","success":false,"time":"2026-08-31T21:30:04+03:00"}"#,
        );
        assert_eq!(code, "KEYRING_ITEM_MISSING");
        assert!(text.contains("keyring"), "{text}");
        assert_eq!(
            classify_en("failed to get item: illegal base64 data at input byte 0").0,
            "KEYRING_DECRYPT_FAILED"
        );
        assert_eq!(
            classify_en("aes.KeyUnwrap(): integrity check failed.").0,
            "KEYRING_DECRYPT_FAILED"
        );
    }

    #[test]
    fn personal_values_go_but_the_fields_stay() {
        // Поле остаётся на месте — уходит только значение. Иначе журнал терял
        // бы структуру, и стало бы непонятно, что именно ответила Apple.
        let raw = r#"{"email":"ivan@mail.ru","name":"Иван Петров","dsid":"1340784009","success":true}"#;
        let out = redact(raw);
        assert!(out.contains(r#""name":"[hidden]""#), "{out}");
        assert!(out.contains(r#""dsid":"[hidden]""#), "{out}");
        assert!(out.contains("[email]"), "{out}");
        // Структура цела: ключи и полезные значения на месте.
        assert!(out.contains(r#""success":true"#), "{out}");
        // Личного не осталось.
        assert!(!out.contains("Иван"), "{out}");
        assert!(!out.contains("Петров"), "{out}");
        assert!(!out.contains("1340784009"), "{out}");
        assert!(!out.contains("ivan@mail.ru"), "{out}");

        // То же в plist-ответах Apple.
        let plist = "<key>name</key><string>Иван Петров</string>";
        let out = redact(plist);
        assert!(out.contains("<key>name</key>"), "{out}");
        assert!(!out.contains("Иван"), "{out}");
    }

    #[test]
    fn classifies_russian_ui_text_too() {
        assert_eq!(
            classify_en("iPhone не найден. Подключите устройство по USB.").0,
            "DEVICE_NOT_FOUND"
        );
        assert_eq!(
            classify_en(
                "Не удалось подготовить вход в Apple ID. Полностью закройте Signum и войдите снова."
            )
            .0,
            "AUTH_PREPARE_FAILED"
        );
        assert_eq!(
            classify_en("Этой версии нет в покупках вашего Apple ID.").0,
            "NOT_PURCHASED"
        );
        assert_eq!(
            classify_en("Не удалось выполнить операцию. Проверьте Apple ID, подключение iPhone.").0,
            "UNCLASSIFIED_OPERATION_FAILURE"
        );
    }

    #[test]
    fn dedup_key_ignores_embedded_timestamps() {
        // ipatool stamps every error with the wall clock; without folding it the
        // 15-minute dedupe never fired and the same fault was sent over and over.
        let a = r#"{"error":"not found in the keyring","time":"2026-08-31T21:30:04+03:00"}"#;
        let b = r#"{"error":"not found in the keyring","time":"2026-08-31T21:31:24+03:00"}"#;
        assert_eq!(
            dedup_key("session", "KEYRING_ITEM_MISSING", a),
            dedup_key("session", "KEYRING_ITEM_MISSING", b)
        );
    }

    #[test]
    fn redact_strips_email_code_and_user_path() {
        let out = redact(r"user@mail.com code 123456 C:\Users\Ivan\AppData panic");
        assert!(!out.contains("user@mail.com"), "{out}");
        assert!(!out.contains("123456"), "{out}");
        assert!(!out.contains(r"\Ivan\"), "{out}");
        assert!(out.contains("[email]"), "{out}");
        assert!(out.contains("[code]"), "{out}");
        let secrets = redact(r#"{"passwordToken":"abc","dsPersonId":"99"} passwordToken=xyz"#);
        assert!(!secrets.contains("abc"), "{secrets}");
        assert!(!secrets.contains("xyz"), "{secrets}");
        assert!(secrets.contains("[hidden]"), "{secrets}");
    }

    #[test]
    fn skip_expected_prompts_not_bugs() {
        assert!(skip_auto("Отменено"));
        assert!(skip_auto("SIGNUM_2FA_REQUIRED:code"));
        assert!(skip_auto("Введите код с iPhone"));
        assert!(skip_auto(
            "Не удалось сменить страну. Подождите минуту и попробуйте снова."
        ));
        assert!(skip_auto(
            "На этом Apple ID нет доступных версий Т-Бизнес в покупках. Войдите в Apple ID, на котором приложение уже приобреталось, и повторите попытку."
        ));
        assert!(skip_auto(
            "Приложение не найдено в App Store — его удалили из магазина или оно недоступно в вашем регионе."
        ));
        assert!(!skip_auto("Не удалось проверить вход в Apple ID."));
        assert!(!skip_auto("Сохранённый вход повреждён. Нажмите «Войти снова»."));
        assert!(!skip_auto(
            r#"discarding cookies in invalid format (error: json: cannot unmarshal object into Go value of type []cookiejar.entry)"#
        ));
        assert!(is_technical_fault(
            r#"failed to get account: failed to get item: illegal base64 data at input byte 0"#
        ));
    }
}
