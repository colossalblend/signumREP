                                                                                                                         
   
                     
                                                               
                                                             
                                         
   
                                                                       

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

                                                                              
                                            
                                                                              

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CertMode {
    #[default]
    Auto,
    Free,
    Paid,
    /// Свой сертификат (.p12 + .mobileprovision) — без Developer-учётки Apple.
    Custom,
}

                                                                   
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignOptions {
                                                     
    #[serde(default)]
    pub lower_min_ios: bool,
                                                                                
    #[serde(default)]
    pub min_ios_version: Option<String>,
                                                              
    #[serde(default)]
    pub file_sharing: bool,
                                                                               
    #[serde(default)]
    pub remove_device_limits: bool,
                                               
    #[serde(default)]
    pub change_app_name: bool,
    #[serde(default)]
    pub app_name: Option<String>,
                                                         
    #[serde(default)]
    pub change_version: bool,
    #[serde(default)]
    pub app_version: Option<String>,
                                                                                 
                                                                
    #[serde(default = "default_true")]
    pub auto_bundle_id: bool,
    #[serde(default)]
    pub custom_bundle_id: Option<String>,
                                                                                         
    #[serde(default)]
    pub force_unique_bundle: bool,
                                                                                           
    #[serde(default)]
    pub strip_plugins: bool,
                                           
    #[serde(default)]
    pub strip_watch: bool,
                                                             
    #[serde(default)]
    pub clear_url_schemes: bool,
                                                                                          
    #[serde(default)]
    pub documents_in_place: bool,
                                                                                             
    #[serde(default)]
    pub supports_opening_docs: bool,
                                                                            
    #[serde(default)]
    pub allow_http: bool,
                                                                              
    #[serde(default)]
    pub remove_orientation_lock: bool,
                                                                                            
    #[serde(default)]
    pub strip_frameworks: bool,
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            lower_min_ios: false,
            min_ios_version: None,
            file_sharing: false,
            remove_device_limits: false,
            change_app_name: false,
            app_name: None,
            change_version: false,
            app_version: None,
            auto_bundle_id: true,
            custom_bundle_id: None,
            force_unique_bundle: false,
            strip_plugins: false,
            strip_watch: false,
            clear_url_schemes: false,
            documents_in_place: false,
            supports_opening_docs: false,
            allow_http: false,
            remove_orientation_lock: false,
            strip_frameworks: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignAndInstallInput {
    pub ipa_path: String,
    #[serde(default)]
    pub udid: Option<String>,
    #[serde(default = "default_true")]
    pub install: bool,
    #[serde(default)]
    pub save_signed: bool,
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub cert_mode: CertMode,
    /// Имя сохранённого сертификата (custom): берём p12/provision из хранилища.
    #[serde(default)]
    pub cert_name: Option<String>,
    #[serde(default)]
    pub custom_p12_path: Option<String>,
    #[serde(default)]
    pub custom_prov_path: Option<String>,
    #[serde(default)]
    pub custom_cert_password: Option<String>,
    #[serde(default)]
    pub options: SignOptions,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct IpaMetadata {
    pub bundle_id: String,
    pub name: String,
    pub version: Option<String>,
    pub min_ios: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SigningStatus {
    pub authenticated: bool,
    pub email: Option<String>,
    pub team_id: Option<String>,
    pub team_name: Option<String>,
    pub team_kind: Option<String>,
    pub cert_ready: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignedIpaResult {
    pub signed_ipa: PathBuf,
    pub metadata: IpaMetadata,
    pub team_id: String,
    pub team_name: String,
    pub team_kind: String,
}

                                                                              
                 
                                                                              

pub fn parse_ipa_metadata(ipa_path: &Path) -> Result<IpaMetadata, String> {
    let file = fs::File::open(ipa_path).map_err(|e| format!("SIGN_IPA_FILE_REQUIRED: {e}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Не удалось прочитать IPA: {e}"))?;
    let mut plist_bytes: Option<Vec<u8>> = None;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Не удалось прочитать IPA: {e}"))?;
        let name = entry.name().replace('\\', "/");
        if name.starts_with("Payload/")
            && name.ends_with(".app/Info.plist")
            && name.matches('/').count() == 2
        {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("Не удалось прочитать Info.plist: {e}"))?;
            plist_bytes = Some(buf);
            break;
        }
    }
    let bytes = plist_bytes.ok_or_else(|| "В IPA нет Payload/*.app/Info.plist".to_string())?;
    let value = plist::Value::from_reader(Cursor::new(bytes))
        .map_err(|e| format!("Некорректный Info.plist: {e}"))?;
    let dict = value
        .as_dictionary()
        .ok_or_else(|| "Info.plist: ожидался словарь".to_string())?;
    let bundle_id = dict
        .get("CFBundleIdentifier")
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string();
    if bundle_id.is_empty() {
        return Err("В IPA нет CFBundleIdentifier".into());
    }
    let name = dict
        .get("CFBundleDisplayName")
        .or_else(|| dict.get("CFBundleName"))
        .and_then(|v| v.as_string())
        .unwrap_or("App")
        .to_string();
    let version = dict
        .get("CFBundleShortVersionString")
        .or_else(|| dict.get("CFBundleVersion"))
        .and_then(|v| v.as_string())
        .map(str::to_string);
    let min_ios = dict
        .get("MinimumOSVersion")
        .and_then(|v| v.as_string())
        .map(str::to_string);
    Ok(IpaMetadata {
        bundle_id,
        name,
        version,
        min_ios,
    })
}

pub fn signing_cache_dir(app_data: &Path) -> PathBuf {
    app_data.join("signing")
}

pub fn default_signed_output_dir(app_data: &Path) -> PathBuf {
    app_data.join("signed-ipas")
}

// ─── Сохранение пользовательских сертификатов (p12 + .mobileprovision) ──────

/// Папка с сертификатами, сохранёнными между запусками приложения.
pub fn saved_certs_dir(app_data: &Path) -> PathBuf {
    app_data.join("certs")
}

fn is_safe_cert_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b' '))
}

/// Параметры одного сохранённого сертификата (для UI: просто показываем).
#[derive(Debug, Clone, Serialize)]
pub struct SavedCertInfo {
    pub name: String,
    pub p12_path: String,
    pub prov_path: String,
    /// Когда профиль перестанет работать, из самого .mobileprovision.
    pub expires_at: Option<String>,
    /// Важно для диагностики — пароль не отдаём наружу, только факт наличия.
    pub has_password: bool,
}

/// Сохранить сертификат: копирует файлы в нашу папку, пароль приводится к строке
/// и кладётся в Windows Credential Manager (не на диск).
pub fn save_custom_cert(
    app_data: &Path,
    name: &str,
    p12_src: &Path,
    prov_src: &Path,
    password: &str,
) -> Result<PathBuf, String> {
    if !p12_src.is_file() {
        return Err("Файл .p12 не найден.".into());
    }
    if !prov_src.is_file() {
        return Err("Файл .mobileprovision не найден.".into());
    }
    if !is_safe_cert_file_name(name) {
        return Err("Недопустимое имя сертификата.".into());
    }
    let dir = saved_certs_dir(app_data).join(name);
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку сертификата: {e}"))?;
    fs::copy(p12_src, dir.join("cert.p12")).map_err(|e| format!("Не удалось сохранить .p12: {e}"))?;
    fs::copy(prov_src, dir.join("profile.mobileprovision"))
        .map_err(|e| format!("Не удалось сохранить профиль: {e}"))?;
    if password.trim().is_empty() {
        let _ = fs::remove_file(dir.join("password.txt"));
        delete_custom_cert_password(name);
    } else {
        // Пароль в Credential Manager, не в файл.
        set_custom_cert_password(name, password)?;
    }
    Ok(dir)
}

/// Пути к файлам сохранённого сертификата + пароль из Credential Manager.
pub fn load_custom_cert(
    app_data: &Path,
    name: &str,
) -> Result<(PathBuf, PathBuf, Option<String>), String> {
    if !is_safe_cert_file_name(name) {
        return Err("Недопустимое имя сертификата.".into());
    }
    let dir = saved_certs_dir(app_data).join(name);
    let p12 = dir.join("cert.p12");
    let prov = dir.join("profile.mobileprovision");
    if !p12.is_file() || !prov.is_file() {
        return Err("Сертификат не найден. Импортируйте его заново.".into());
    }
    Ok((p12, prov, get_custom_cert_password(name)))
}

pub fn delete_custom_cert(app_data: &Path, name: &str) -> Result<(), String> {
    if !is_safe_cert_file_name(name) {
        return Err("Недопустимое имя сертификата.".into());
    }
    delete_custom_cert_password(name);
    let dir = saved_certs_dir(app_data).join(name);
    if dir.is_dir() {
        fs::remove_dir_all(&dir).map_err(|e| format!("Не удалось удалить сертификат: {e}"))?;
    }
    Ok(())
}

pub fn list_custom_certs(app_data: &Path) -> Vec<SavedCertInfo> {
    let root = saved_certs_dir(app_data);
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let p12 = path.join("cert.p12");
        let prov = path.join("profile.mobileprovision");
        if !p12.is_file() || !prov.is_file() {
            continue;
        }
        out.push(SavedCertInfo {
            name: name.to_string(),
            p12_path: p12.display().to_string(),
            expires_at: provision_expires_at(&prov),
            prov_path: prov.display().to_string(),
            has_password: get_custom_cert_password(name).is_some(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Срок действия профиля.
///
/// Файл — подписанный контейнер, внутри которого лежит обычный XML-plist.
/// Полноценный разбор контейнера ради одной даты не нужен: достаточно взять
/// значение ключа прямо из текста.
fn provision_expires_at(path: &Path) -> Option<String> {
    let raw = fs::read(path).ok()?;
    provision_expiry_from_bytes(&raw)
}

pub(crate) fn provision_expiry_from_bytes(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(&raw);
    let tail = &text[text.find("<key>ExpirationDate</key>")?..];
    let start = tail.find("<date>")? + "<date>".len();
    let end = tail.find("</date>")?;
    if end <= start {
        return None;
    }
    let value = tail[start..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn custom_cert_keyring_entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("Signum", &format!("custom_cert_password:{name}"))
        .map_err(|e| e.to_string())
}

fn get_custom_cert_password(name: &str) -> Option<String> {
    custom_cert_keyring_entry(name)
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .filter(|value| !value.trim().is_empty())
}

fn set_custom_cert_password(name: &str, password: &str) -> Result<(), String> {
    custom_cert_keyring_entry(name)?
        .set_password(password)
        .map_err(|e| format!("Не удалось сохранить пароль сертификата: {e}"))
}

fn delete_custom_cert_password(name: &str) {
    if let Ok(entry) = custom_cert_keyring_entry(name) {
        let _ = entry.delete_credential();
    }
}

                                                                                  
fn wait_for_signing_2fa_code(
    app: &tauri::AppHandle,
    sms: bool,
) -> isideload::auth::apple_account::TwoFactorCallbackResponse {
    use isideload::auth::apple_account::TwoFactorCallbackResponse;
    use tauri::{Emitter, Manager};

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Some(state) = app.try_state::<crate::AppState>() {
        if let Ok(mut slot) = state.signing_2fa_reply.lock() {
            *slot = Some(tx);
        }
    } else {
        return TwoFactorCallbackResponse::Abort;
    }

    let message = if sms {
        "Apple отправил SMS. Введите 6-значный код."
    } else {
        "Apple отправил код на iPhone. Введите 6 цифр."
    };
    let _ = app.emit(
        "signing-2fa-required",
        serde_json::json!({
            "message": message,
            "sms": sms,
        }),
    );

    match rx.recv_timeout(Duration::from_secs(180)) {
        Ok(code) if code == "__resend__" => TwoFactorCallbackResponse::ResendCode,
        Ok(code) => {
            let digits: String = code
                .chars()
                .filter(|c| c.is_ascii_digit())
                .take(6)
                .collect();
            if digits.len() >= 6 {
                TwoFactorCallbackResponse::SubmitCode(digits)
            } else {
                TwoFactorCallbackResponse::Abort
            }
        }
        Err(_) => TwoFactorCallbackResponse::Abort,
    }
}

fn report_text(err: impl std::fmt::Display) -> String {
    err.to_string()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !lower.contains(".rs:")
                && !lower.contains("src-tauri")
                && !lower.contains("vendor\\isideload")
                && !lower.contains("vendor/isideload")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn map_sign_app_err(err: impl std::fmt::Display) -> String {
    let s = report_text(err);
    let lower = s.to_ascii_lowercase();
    if lower.contains("not enough available app id")
        || (lower.contains("app id") && lower.contains("required") && lower.contains("available"))
    {
        return "SIGN_IPA_APP_ID_LIMIT: На этом Apple ID кончился лимит подписей — у бесплатного их 10. Включите «Удалить плагины» или войдите другим Apple ID.".into();
    }
    if lower.contains("8220")
        || lower.contains("no devices from which to generate")
        || lower.contains("team has no devices")
        || (lower.contains("provisioning profile") && lower.contains("no device"))
    {
        return "SIGN_IPA_NO_TEAM_DEVICE: Для подписи Apple нужна хотя бы одна регистрация iPhone на этом Apple ID. Подключите iPhone кабелем, нажмите «Доверять» и повторите. Потом файл можно сохранять и без телефона.".into();
    }
    if s.trim().is_empty() {
        return "SIGN_IPA_CERT_FAILED: Не удалось подписать файл. Повторите попытку или войдите другим Apple ID.".into();
    }
    map_isideload_err(s)
}

/// Похоже ли на временный отказ Apple, который имеет смысл просто переждать.
fn is_apple_outage(lower: &str) -> bool {
    lower.contains("service temporarily unavailable")
        || lower.contains("service unavailable")
        || (lower.contains("server error") && lower.contains("gsa.apple.com"))
        || has_status_code(lower, "503")
        || has_status_code(lower, "502")
}

/// Код состояния, а не кусок другого числа.
///
/// Проверка на голое вхождение «503» принимала за отказ сервиса любой текст,
/// где это сочетание встретилось внутри длинного числа — например в
/// идентификаторе приложения 6503087901.
fn has_status_code(text: &str, code: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices(code).any(|(start, _)| {
        let end = start + code.len();
        let left_clean = start == 0 || !bytes[start - 1].is_ascii_digit();
        let right_clean = end >= bytes.len() || !bytes[end].is_ascii_digit();
        left_clean && right_clean
    })
}

fn map_isideload_err(err: impl std::fmt::Display) -> String {
    let s = report_text(err);
    let lower = s.to_ascii_lowercase();
    if lower.contains("not enough available app id")
        || (lower.contains("app id") && lower.contains("required") && lower.contains("available"))
    {
        return "SIGN_IPA_APP_ID_LIMIT: На этом Apple ID кончился лимит подписей — у бесплатного их 10. Включите «Удалить плагины» или войдите другим Apple ID.".into();
    }
    if lower.contains("8220")
        || lower.contains("no devices from which to generate")
        || lower.contains("team has no devices")
        || (lower.contains("provisioning profile") && lower.contains("no device"))
    {
        return "SIGN_IPA_NO_TEAM_DEVICE: Для подписи Apple нужна хотя бы одна регистрация iPhone на этом Apple ID. Подключите iPhone кабелем, нажмите «Доверять» и повторите. Потом файл можно сохранять и без телефона.".into();
    }
    if lower.contains("2fa")
        || lower.contains("two-factor")
        || lower.contains("two factor")
        || lower.contains("no 2fa code")
        || lower.contains("abort")
    {
        return "SIGN_IPA_DEV_LOGIN_FAILED: нужен код 2FA для Developer. Введите код в окне Signum (не только вход в «Аккаунт»).".into();
    }
    if lower.contains("password") || lower.contains("credential") || lower.contains("-22406") {
        return "SIGN_IPA_DEV_LOGIN_FAILED: Apple не принял пароль. Выйдите и войдите снова в «Аккаунт».".into();
    }
    if lower.contains("anisette") {
        return "SIGN_IPA_DEV_LOGIN_FAILED: Apple не подтвердила этот компьютер. Проверьте интернет и повторите.".into();
    }
    // 503/502 от gsa.apple.com — это отказ на стороне Apple, а не наша ошибка и
    // не проблема пароля. Без этой ветки текст падал в общий случай и
    // показывался пользователю сырым дампом.
    if is_apple_outage(&lower) {
        return "SIGN_IPA_DEV_LOGIN_FAILED: Apple временно не отвечает (сервис входа перегружен).                 Это не ошибка пароля — подождите несколько минут и повторите."
            .into();
    }
    if lower.contains("team")
        && (lower.contains("empty") || lower.contains("none") || lower.contains("no "))
    {
        return "SIGN_IPA_TEAM_NONE".into();
    }
    if lower.contains("appidname")
        || lower.contains("app id name")
        || (lower.contains("invalid value")
            && (lower.contains("appid")
                || lower.contains("parameter 'name'")
                || lower.contains("parameter \"name\"")))
        || lower.contains("failed to add developer app id")
        || lower.contains("failed to add developer app group")
    {
        return "SIGN_IPA_CERT_FAILED: Apple не приняла имя приложения — обычно из-за кириллицы. Обновите Signum и повторите: имена подчищаются сами.".into();
    }
    if lower.contains("certificate")
        || lower.contains("7252")
        || lower.contains("7460")
        || lower.contains("csr")
        || lower.contains("cert identity")
    {
        return format!("SIGN_IPA_CERT_FAILED: не удалось выпустить сертификат разработки. {s}");
    }
    // A malformed IPA, not a login problem: an extension inside the app carries
    // a bundle id that is not a child of the main one, so Apple refuses the
    // whole bundle. Users were told to sign out and back in, which never helped.
    if lower.contains("invalid bundle")
        || lower.contains("is not part of the main app bundle identifier")
        || lower.contains("mismatched extension bundle")
    {
        return format!("SIGN_IPA_BUNDLE_INVALID: В приложении есть расширение с чужим идентификатором, Apple такой пакет не подписывает. Включите «Удалить плагины / extensions» в опциях подписи и повторите. {s}");
    }
    // Apple was simply unreachable — the retry advice is about the network,
    // not about the Apple ID.
    if lower.contains("failed to fetch url bag")
        || lower.contains("error sending request")
        || lower.contains("connectionreset")
        || lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("dns error")
        || lower.contains("timed out")
    {
        return "SIGN_IPA_NETWORK: Не удалось связаться с серверами Apple. Проверьте интернет или VPN и повторите попытку.".into();
    }
    format!("SIGN_IPA_DEV_LOGIN_FAILED: {s}")
}

                                                                
                                                         
   
                                                                           
                                                                        
pub async fn sign_ipa_for_device(
    app: &tauri::AppHandle,
    ipa_path: &Path,
    udid: Option<&str>,
    device_name: &str,
    email: &str,
    password: &str,
    cert_mode: CertMode,
    options: &SignOptions,
    _anisette_exe: Option<&Path>,
    _zsign_exe: &Path,
    cache_dir: &Path,
    output_ipa: &Path,
    mut progress: impl FnMut(f64, &str),
) -> Result<SignedIpaResult, String> {
    use isideload::{
        anisette::remote_v3::RemoteV3AnisetteProvider,
        auth::apple_account::AppleAccount,
        dev::{developer_session::DeveloperSession, devices::DevicesApi},
        sideload::{builder::MaxCertsBehavior, SideloaderBuilder, TeamSelection},
        util::fs_storage::FsStorage,
    };
    if !ipa_path.is_file() {
        return Err("SIGN_IPA_FILE_REQUIRED".into());
    }
    let udid = udid.map(str::trim).filter(|v| !v.is_empty());
    if email.trim().is_empty() || password.is_empty() {
        return Err("SIGN_IPA_AUTH_REQUIRED".into());
    }

    let _ = isideload::init();

    progress(3.0, "Читаем файл…");
    let mut metadata = parse_ipa_metadata(ipa_path)?;

    fs::create_dir_all(cache_dir).map_err(|e| format!("Не удалось создать кэш подписи: {e}"))?;
    let anisette_store = cache_dir.join("isideload-anisette");
    let cert_store = cache_dir.join("isideload-certs");
    let work_dir = cache_dir.join("work");
    fs::create_dir_all(&anisette_store).ok();
    fs::create_dir_all(&cert_store).ok();
    fs::create_dir_all(&work_dir).ok();

    progress(5.0, "Готовим приложение…");
    let prepared_ipa = prepare_ipa_with_options(ipa_path, &work_dir, &mut metadata, options)?;

    progress(8.0, "Проверяем устройство…");
    progress(12.0, "Входим в Apple ID…");

    // Сервис входа Apple регулярно отвечает 503 на несколько секунд. Раньше
    // один такой ответ обрывал всю подпись, и человек начинал заново со своим
    // паролем и кодом 2FA. Пережидаем сами: отказ сервиса — единственное, что
    // повторяем, всё остальное (пароль, 2FA, лимиты) от повтора не исправится.
    const LOGIN_ATTEMPTS: u32 = 3;
    let mut account = None;
    let mut last_error = String::new();
    for attempt in 0..LOGIN_ATTEMPTS {
        if attempt > 0 {
            let pause = Duration::from_secs(5 * u64::from(attempt));
            progress(12.0, "Apple занят, пробуем ещё раз…");
            tokio::time::sleep(pause).await;
        }
        let anisette = RemoteV3AnisetteProvider::default()
            .map_err(map_isideload_err)?
            .set_storage(Box::new(FsStorage::new(anisette_store.clone())))
            .set_serial_number(crate::apple_store::v6_serial().unwrap_or_else(|| "SIGNUM-1".into()));
        let app_for_2fa = app.clone();
        let two_factor = Box::new(
            move |params: isideload::auth::apple_account::TwoFactorCallbackParams| {
                wait_for_signing_2fa_code(&app_for_2fa, params.sms)
            },
        );
        match AppleAccount::builder(email.trim())
            .danger_debug(true)
            .anisette_provider(anisette)
            .login(password, two_factor)
            .await
        {
            Ok(logged_in) => {
                account = Some(logged_in);
                break;
            }
            Err(error) => {
                let mapped = map_isideload_err(error);
                if !is_apple_outage(&mapped.to_ascii_lowercase()) {
                    return Err(mapped);
                }
                last_error = mapped;
            }
        }
    }
    let account = match account {
        Some(account) => account,
        None => return Err(last_error),
    };

    progress(22.0, "Получаем разрешение Apple…");
    let mut account = account;
    let dev_session = DeveloperSession::from_account(&mut account)
        .await
        .map_err(map_isideload_err)?;

    progress(30.0, "Выбираем аккаунт разработчика…");
    let mut sideloader = SideloaderBuilder::new(dev_session, email.trim().to_string())
        .team_selection(TeamSelection::First)
        .max_certs_behavior(MaxCertsBehavior::Revoke)
        .storage(Box::new(FsStorage::new(cert_store)))
        .machine_name("Signum".into())
        .delete_app_after_install(false)
        .build();

    let team = select_team_isideload(&mut sideloader, cert_mode).await?;

    if let Some(udid) = udid {
        progress(38.0, "Регистрируем устройство…");
        sideloader
            .get_dev_session()
            .ensure_device_registered(&team, device_name, udid, None)
            .await
            .map_err(map_sign_app_err)?;
    } else {
        progress(38.0, "Проверяем iPhone в аккаунте…");
        let known = sideloader
            .get_dev_session()
            .list_devices(&team, None)
            .await
            .map_err(map_sign_app_err)?;
        if known.is_empty() {
            return Err(
                "SIGN_IPA_NO_TEAM_DEVICE: Для подписи Apple нужна хотя бы одна регистрация iPhone на этом Apple ID. Подключите iPhone кабелем, нажмите «Доверять» и повторите. Потом файл можно сохранять и без телефона.".into(),
            );
        }
    }

    progress(48.0, "Выпускаем сертификат…");
    let sign_fut = sideloader.sign_app(prepared_ipa.clone(), Some(team.clone()), false);
    let (bundle_dir, _special) = tokio::time::timeout(Duration::from_secs(240), sign_fut)
        .await
        .map_err(|_| {
            "SIGN_IPA_DEV_LOGIN_FAILED: таймаут подписи (Apple Developer / сеть). Проверьте VPN/интернет и повторите."
                .to_string()
        })?
        .map_err(map_sign_app_err)?;

    progress(75.0, "Собираем приложение…");
    if let Some(parent) = output_ipa.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Не удалось создать папку: {e}"))?;
    }
    pack_app_dir_to_ipa(&bundle_dir, output_ipa)?;

    let _ = fs::remove_dir_all(
        bundle_dir
            .parent()
            .and_then(|p| {
                p.parent().and_then(|pp| {
                    if pp
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with("_extracted"))
                    {
                        Some(pp)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(bundle_dir.as_path()),
    );
    let _ = fs::remove_file(&prepared_ipa);

    let team_kind = team_kind_label(&team, cert_mode);
    let team_name = team.name.clone().unwrap_or_else(|| "Team".into());

    progress(90.0, "Подпись готова");
    Ok(SignedIpaResult {
        signed_ipa: output_ipa.to_path_buf(),
        metadata,
        team_id: team.team_id.clone(),
        team_name,
        team_kind,
    })
}

/// На что похож выбранный файл сертификата.
///
/// Настоящий .p12 — это DER, то есть начинается с тега SEQUENCE (0x30).
/// Проверка нарочно поверхностная: она не подтверждает валидность, но надёжно
/// отличает «не тот файл» от «не тот пароль», а именно это и путали.
enum P12Shape {
    LooksLikeP12,
    Pem,
    NotDer,
    Empty,
}

fn p12_shape(path: &Path) -> P12Shape {
    let Ok(bytes) = fs::read(path) else {
        return P12Shape::Empty;
    };
    if bytes.is_empty() {
        return P12Shape::Empty;
    }
    if bytes.starts_with(b"-----BEGIN") {
        return P12Shape::Pem;
    }
    if bytes.first() != Some(&0x30) {
        return P12Shape::NotDer;
    }
    // Проверки одного байта мало: с 0x30 начинается любой DER — и сертификат
    // .cer, и .mobileprovision. Раньше все они считались «похоже на p12», и
    // человеку говорили, что он ошибся паролем, хотя файл был не тот.
    //
    // У настоящего PKCS#12 сразу за внешней последовательностью идёт версия —
    // целое число 3, то есть байты 02 01 03.
    match der_payload_start(&bytes) {
        Some(at) if bytes[at..].starts_with(&[0x02, 0x01, 0x03]) => P12Shape::LooksLikeP12,
        _ => P12Shape::NotDer,
    }
}

/// Смещение содержимого DER-последовательности: пропускаем тег и длину.
fn der_payload_start(bytes: &[u8]) -> Option<usize> {
    let length_byte = *bytes.get(1)?;
    if length_byte < 0x80 {
        return Some(2); // короткая форма длины
    }
    // Длинная форма: младшие биты — сколько байт занимает сама длина.
    let count = usize::from(length_byte & 0x7f);
    if count == 0 || count > 4 {
        return None;
    }
    Some(2 + count)
}

/// Подпись через zsign с пользовательским сертификатом (.p12 + .mobileprovision).
/// Не трогает Apple ID: подходит для годовых/enterprise сертификатов.
pub async fn sign_ipa_with_custom_cert(
    _app: &tauri::AppHandle,
    ipa_path: &Path,
    p12_path: &Path,
    mprov_path: &Path,
    password: &str,
    options: &SignOptions,
    zsign_exe: &Path,
    cache_dir: &Path,
    output_ipa: &Path,
    mut progress: impl FnMut(f64, &str),
) -> Result<SignedIpaResult, String> {
    if !ipa_path.is_file() {
        return Err("SIGN_IPA_FILE_REQUIRED".into());
    }
    if !p12_path.is_file() {
        return Err("Файл .p12 не найден. Выберите его заново.".into());
    }
    if !mprov_path.is_file() {
        return Err("Файл .mobileprovision не найден. Выберите его заново.".into());
    }

    progress(3.0, "Читаем файл…");
    let mut metadata = parse_ipa_metadata(ipa_path)?;

    fs::create_dir_all(cache_dir).map_err(|e| format!("Не удалось создать кэш подписи: {e}"))?;
    let work_dir = cache_dir.join("work-custom");
    fs::create_dir_all(&work_dir).map_err(|e| e.to_string())?;

    progress(12.0, "Готовим приложение…");
    let prepared_ipa = prepare_ipa_with_options(ipa_path, &work_dir, &mut metadata, options)?;

    progress(30.0, "Подписываем сертификатом…");
    let mut args: Vec<String> = vec![
        "-k".into(),
        p12_path.to_string_lossy().into_owned(),
        "-m".into(),
        mprov_path.to_string_lossy().into_owned(),
        "-z".into(),
        "9".into(),
    ];
    if let Some(bundle_id) = options
        .custom_bundle_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .filter(|_| !options.auto_bundle_id)
    {
        args.push("-b".into());
        args.push(bundle_id.to_string());
    }
    if options.change_app_name {
        if let Some(name) = options
            .app_name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            args.push("-n".into());
            args.push(name.to_string());
        }
    }
    if options.change_version {
        if let Some(version) = options
            .app_version
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            args.push("-r".into());
            args.push(version.to_string());
        }
    }
    args.push("-o".into());
    args.push(output_ipa.to_string_lossy().into_owned());
    args.push(prepared_ipa.to_string_lossy().into_owned());

    // -p только если пароль непустой — иначе zsign падает на пустом.
    if !password.trim().is_empty() {
        let mut with_pass = vec!["-p".to_string(), password.to_string()];
        with_pass.extend(std::mem::take(&mut args));
        args = with_pass;
    }

    let exe = zsign_exe.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        let mut command = std::process::Command::new(&exe);
        command
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        // Signing runs behind the progress dialog; without this zsign opens its
        // own console window over the app for the whole run.
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command.output()
    })
    .await
    .map_err(|e| format!("Не удалось запустить подпись: {e}"))?
    .map_err(|e| format!("Не удалось запустить подпись: {e}"))?;

    if !output.status.success() || !output_ipa.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = format!("{stderr}\n{stdout}")
            .trim()
            .chars()
            .take(600)
            .collect::<String>();
        let lowered = detail.to_ascii_lowercase();
        // zsign на любую неудачу с сертификатом отвечает «input the correct file
        // and password», поэтому слово password в тексте не значит, что дело в
        // пароле. Смотрим на сам файл, иначе человек будет перебирать верный
        // пароль к файлу, который в принципе не .p12.
        let cert_shape = p12_shape(p12_path);
        let friendly = if lowered.contains("password") || lowered.contains("pkcs12") {
            match cert_shape {
                P12Shape::Pem => {
                    "Это не .p12, а текстовый PEM-файл. Экспортируйте сертификат из «Связки ключей» как «Личный обмен информацией (.p12)»."
                }
                P12Shape::NotDer => {
                    "Это не файл .p12. Похоже, выбран сертификат .cer или профиль .mobileprovision — а нужен .p12, в котором лежит и ключ."
                }
                P12Shape::Empty => "Файл сертификата пустой. Выберите .p12 заново.",
                P12Shape::LooksLikeP12 => {
                    "Неверный пароль от сертификата .p12. Проверьте его и повторите."
                }
            }
        } else if lowered.contains("provision") {
            "Файл .mobileprovision не подошёл к сертификату. Проверьте оба файла."
        } else {
            "Не удалось подписать IPA этим сертификатом. Проверьте, что файлы не повреждены."
        };
        return Err(format!("{friendly}\n\nПодробности: {detail}"));
    }

    let _ = fs::remove_dir_all(&work_dir);

    progress(90.0, "Подпись готова");
    SignedIpaResult::from_paths(output_ipa, metadata, "custom")
}

impl SignedIpaResult {
    fn from_paths(
        output_ipa: &Path,
        metadata: IpaMetadata,
        team_kind: &str,
    ) -> Result<SignedIpaResult, String> {
        Ok(SignedIpaResult {
            signed_ipa: output_ipa.to_path_buf(),
            metadata,
            team_id: String::new(),
            team_name: "Свой сертификат".into(),
            team_kind: team_kind.to_string(),
        })
    }
}

                                                                                            
fn prepare_ipa_with_options(
    ipa_path: &Path,
    work_dir: &Path,
    metadata: &mut IpaMetadata,
    options: &SignOptions,
) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let extract_dir = work_dir.join(format!("extract-{stamp}"));
    let out_ipa = work_dir.join(format!("prepared-{stamp}.ipa"));
    if extract_dir.exists() {
        let _ = fs::remove_dir_all(&extract_dir);
    }
    fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;

              
    {
        let file = fs::File::open(ipa_path).map_err(|e| e.to_string())?;
        let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
        archive.extract(&extract_dir).map_err(|e| e.to_string())?;
    }

    let payload = extract_dir.join("Payload");
    let app_dir = find_app_bundle(&payload)?;

                                                                  
    if options.strip_plugins {
        let _ = fs::remove_dir_all(app_dir.join("PlugIns"));
        let _ = fs::remove_dir_all(app_dir.join("Extensions"));
        let _ = fs::remove_dir_all(app_dir.join("AppClips"));
    }
    if options.strip_watch {
        let _ = fs::remove_dir_all(app_dir.join("Watch"));
        let _ = fs::remove_dir_all(app_dir.join("WatchKit"));
        let _ = fs::remove_dir_all(app_dir.join("WatchKitSupport"));
        let _ = fs::remove_dir_all(app_dir.join("WatchKitSupport2"));
        let _ = fs::remove_dir_all(app_dir.join("com.apple.WatchPlaceholder"));
    }
    if options.strip_frameworks {
                                                                                      
        let fw = app_dir.join("Frameworks");
        if fw.is_dir() {
            if let Ok(entries) = fs::read_dir(&fw) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                                                                                              
                    let drop = name.ends_with(".framework")
                        && (name.contains("Widget")
                            || name.contains("Extension")
                            || name.contains("Watch")
                            || name.contains("Notification")
                            || name.contains("Intent")
                            || name.contains("Share"));
                    if drop {
                        let _ = fs::remove_dir_all(&p);
                    }
                }
            }
        }
    }

    patch_info_plist(&app_dir.join("Info.plist"), metadata, options)?;

             
    pack_dir_to_ipa(&extract_dir, &out_ipa)?;
    let _ = fs::remove_dir_all(&extract_dir);
    Ok(out_ipa)
}

fn find_app_bundle(payload: &Path) -> Result<PathBuf, String> {
    if !payload.is_dir() {
        return Err("В IPA нет Payload/".into());
    }
    let mut apps = Vec::new();
    for entry in fs::read_dir(payload).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("app") {
            apps.push(p);
        }
    }
    match apps.len() {
        1 => Ok(apps.remove(0)),
        0 => Err("В Payload нет .app".into()),
        _ => Err("В Payload несколько .app".into()),
    }
}

fn patch_info_plist(
    plist_path: &Path,
    metadata: &mut IpaMetadata,
    options: &SignOptions,
) -> Result<(), String> {
    if !plist_path.is_file() {
        return Err("Info.plist не найден".into());
    }
    let bytes = fs::read(plist_path).map_err(|e| e.to_string())?;
    let mut value = plist::Value::from_reader(Cursor::new(&bytes)).map_err(|e| e.to_string())?;
    let dict = value
        .as_dictionary_mut()
        .ok_or_else(|| "Info.plist: не словарь".to_string())?;

    if options.lower_min_ios {
        let ver = options
            .min_ios_version
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("12.0");
        dict.insert(
            "MinimumOSVersion".into(),
            plist::Value::String(ver.to_string()),
        );
        metadata.min_ios = Some(ver.to_string());
    }

    if options.file_sharing || options.supports_opening_docs || options.documents_in_place {
        dict.insert("UIFileSharingEnabled".into(), plist::Value::Boolean(true));
        dict.insert(
            "LSSupportsOpeningDocumentsInPlace".into(),
            plist::Value::Boolean(true),
        );
        dict.insert(
            "UISupportsDocumentBrowser".into(),
            plist::Value::Boolean(true),
        );
    }

    if options.remove_device_limits {
        dict.remove("UIRequiredDeviceCapabilities");
                        
        dict.insert(
            "UIDeviceFamily".into(),
            plist::Value::Array(vec![
                plist::Value::Integer(1.into()),
                plist::Value::Integer(2.into()),
            ]),
        );
                                                                      
        dict.remove("UISupportedDevices");
    }

    if options.change_app_name {
        if let Some(name) = options
            .app_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            dict.insert(
                "CFBundleDisplayName".into(),
                plist::Value::String(name.to_string()),
            );
            dict.insert(
                "CFBundleName".into(),
                plist::Value::String(name.to_string()),
            );
            metadata.name = name.to_string();
        }
    }

    if options.change_version {
        if let Some(ver) = options
            .app_version
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            dict.insert(
                "CFBundleShortVersionString".into(),
                plist::Value::String(ver.to_string()),
            );
                                                    
            let build = ver
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>();
            let build = if build.is_empty() {
                ver.to_string()
            } else {
                build
            };
            dict.insert("CFBundleVersion".into(), plist::Value::String(build));
            metadata.version = Some(ver.to_string());
        }
    }

                                                           
    let mut bundle = metadata.bundle_id.clone();
    if !options.auto_bundle_id {
        if let Some(custom) = options
            .custom_bundle_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            bundle = sanitize_bundle_id(custom);
        }
    }
    if options.force_unique_bundle {
        let suffix: String = {
            use rand::Rng;
            const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
            let mut rng = rand::thread_rng();
            (0..6)
                .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
                .collect()
        };
        bundle = format!("{bundle}.{suffix}");
    }
    if bundle != metadata.bundle_id || !options.auto_bundle_id || options.force_unique_bundle {
        dict.insert(
            "CFBundleIdentifier".into(),
            plist::Value::String(bundle.clone()),
        );
        metadata.bundle_id = bundle;
    }

    if options.clear_url_schemes {
        dict.remove("CFBundleURLTypes");
    }

    if options.allow_http {
        let mut ats = plist::Dictionary::new();
        ats.insert("NSAllowsArbitraryLoads".into(), plist::Value::Boolean(true));
        ats.insert(
            "NSAllowsLocalNetworking".into(),
            plist::Value::Boolean(true),
        );
        dict.insert(
            "NSAppTransportSecurity".into(),
            plist::Value::Dictionary(ats),
        );
    }

    if options.remove_orientation_lock {
        dict.remove("UISupportedInterfaceOrientations");
        dict.remove("UISupportedInterfaceOrientations~ipad");
        dict.remove("UIRequiresFullScreen");
    }

                                                     
    let mut out = Vec::new();
    value
        .to_writer_xml(&mut out)
        .map_err(|e| format!("Info.plist write: {e}"))?;
    fs::write(plist_path, out).map_err(|e| e.to_string())?;
    Ok(())
}

fn sanitize_bundle_id(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '.'
            }
        })
        .collect();
    let mut parts: Vec<&str> = cleaned.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return "app.signum.sideload".into();
    }
                                             
    if parts[0].chars().next().is_some_and(|c| c.is_ascii_digit()) {
        parts[0] = "app";
    }
    parts.join(".")
}

                                                                
fn pack_dir_to_ipa(root: &Path, output_ipa: &Path) -> Result<(), String> {
    let file = fs::File::create(output_ipa).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    add_dir_to_zip_rel(&mut zip, root, root, opts)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn add_dir_to_zip_rel(
    zip: &mut ZipWriter<fs::File>,
    root: &Path,
    dir: &Path,
    opts: SimpleFileOptions,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            zip.add_directory(format!("{rel}/"), opts)
                .map_err(|e| e.to_string())?;
            add_dir_to_zip_rel(zip, root, &path, opts)?;
        } else {
            let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            zip.start_file(rel, opts).map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn select_team_isideload(
    sideloader: &mut isideload::sideload::sideloader::Sideloader,
    mode: CertMode,
) -> Result<isideload::dev::teams::DeveloperTeam, String> {
    use isideload::dev::teams::TeamsApi;

    match TeamsApi::list_teams(sideloader.get_dev_session()).await {
        Ok(teams) if !teams.is_empty() => pick_team(&teams, mode)
            .cloned()
            .ok_or_else(|| "SIGN_IPA_TEAM_NONE".to_string()),
        Ok(_) => Err("SIGN_IPA_TEAM_NONE".into()),
        Err(_) => sideloader
            .get_team()
            .await
            .map_err(|e| format!("SIGN_IPA_TEAM_NONE: {e}")),
    }
}

fn pick_team<'a>(
    teams: &'a [isideload::dev::teams::DeveloperTeam],
    mode: CertMode,
) -> Option<&'a isideload::dev::teams::DeveloperTeam> {
    let is_free = |t: &isideload::dev::teams::DeveloperTeam| {
        let ty = t.r#type.as_deref().unwrap_or("").to_ascii_lowercase();
        let name = t.name.as_deref().unwrap_or("").to_ascii_lowercase();
        ty.contains("free") || name.contains("free")
    };
    match mode {
        CertMode::Paid => teams.iter().find(|t| !is_free(t)).or_else(|| teams.first()),
        CertMode::Free => teams.iter().find(|t| is_free(t)).or_else(|| teams.first()),
        // Custom never reaches team selection (signed via zsign, not Apple dev
        // team) — keep a valid fallback so the match is total.
        CertMode::Auto | CertMode::Custom => {
            teams.iter().find(|t| !is_free(t)).or_else(|| teams.first())
        }
    }
}

fn team_kind_label(team: &isideload::dev::teams::DeveloperTeam, _mode: CertMode) -> String {
    let ty = team.r#type.as_deref().unwrap_or("").to_ascii_lowercase();
    let name = team.name.as_deref().unwrap_or("").to_ascii_lowercase();
    if ty.contains("free") || name.contains("free") {
        "free".into()
    } else {
        "paid".into()
    }
}

                                                                     
fn pack_app_dir_to_ipa(app_dir: &Path, output_ipa: &Path) -> Result<(), String> {
    if !app_dir.is_dir() {
        return Err(format!(
            "SIGN_IPA_ZSIGN_FAILED: нет подписанного .app ({})",
            app_dir.display()
        ));
    }
    let app_name = app_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("App.app");
    if let Some(parent) = output_ipa.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Не удалось создать папку: {e}"))?;
    }
    let file = fs::File::create(output_ipa).map_err(|e| format!("Не удалось создать IPA: {e}"))?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let prefix = format!("Payload/{app_name}");
    add_dir_to_zip(&mut zip, app_dir, &prefix, opts)?;
    zip.finish()
        .map_err(|e| format!("Не удалось завершить IPA: {e}"))?;
    Ok(())
}

fn add_dir_to_zip(
    zip: &mut ZipWriter<fs::File>,
    dir: &Path,
    zip_prefix: &str,
    opts: SimpleFileOptions,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let zip_path = format!("{zip_prefix}/{name}");
        if path.is_dir() {
            zip.add_directory(format!("{zip_path}/"), opts)
                .map_err(|e| e.to_string())?;
            add_dir_to_zip(zip, &path, &zip_path, opts)?;
        } else {
            let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            zip.start_file(zip_path, opts).map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Пересобирает IPA одним проходом.
///
/// ipatool не пакует архив заново, а дописывает `iTunesMetadata.plist` и
/// `SC_Info/*.sinf` в конец готового файла от Apple. Служебная карта архива
/// `META-INF/com.apple.ZipMetadata.plist` после этого описывает состояние «до»
/// (RecordCount меньше фактического), и установщик на iPhone по ней спотыкается:
/// файл скачивается целиком, а установка падает с «Не удаётся установить».
/// Карту выбрасываем — без неё iOS ставит нормально. Содержимое не меняется.
pub fn repack_ipa(path: &Path) -> Result<(), String> {
    let tmp = path.with_extension("repack");
    let result = (|| -> Result<(), String> {
        let mut zin = ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut zout = ZipWriter::new(fs::File::create(&tmp).map_err(|e| e.to_string())?);
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(6));


        let mut order: Vec<usize> = (0..zin.len()).collect();
        order.sort_by_key(|&i| {
            let name = zin.name_for_index(i).unwrap_or("").to_string();
            (name != "iTunesMetadata.plist", name)
        });

        for i in order {
            let mut entry = zin.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            if entry.is_dir() || name.starts_with("META-INF/") {
                continue;
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            zout.start_file(name, opts).map_err(|e| e.to_string())?;
            zout.write_all(&buf).map_err(|e| e.to_string())?;
        }
        zout.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
        return result;
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod repack_tests {
    use super::*;

    #[test]
    fn repack_drops_apple_metadata_and_puts_purchase_plist_first() {
        let dir = std::env::temp_dir().join("signum-repack-tests");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("demo.ipa");

        let file = fs::File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in [
            ("META-INF/com.apple.ZipMetadata.plist", &b"stale map"[..]),
            ("Payload/Demo.app/Info.plist", &b"info"[..]),
            ("Payload/Demo.app/SC_Info/Demo.sinf", &b"sinf"[..]),
            ("iTunesMetadata.plist", &b"meta"[..]),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();

        repack_ipa(&path).unwrap();

        let mut out = ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let names: Vec<String> = out.file_names().map(str::to_string).collect();
        assert_eq!(out.by_index(0).unwrap().name(), "iTunesMetadata.plist");
        assert!(!names.iter().any(|n| n.starts_with("META-INF")));

        assert!(names.iter().any(|n| n.ends_with(".sinf")));
        assert!(!dir.join("demo.repack").exists());

        let _ = fs::remove_file(&path);
    }
}

#[cfg(test)]
mod provision_expiry_tests {
    use super::*;

    fn write(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join("signum-provision-tests");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn expiry_is_read_through_the_binary_wrapper() {
        // A real .mobileprovision is a signed container: the plist sits between
        // bytes that are not valid UTF-8, which is the whole point of the test.
        let mut raw = vec![0x30, 0x82, 0x0a, 0xff, 0x00];
        raw.extend_from_slice(
            b"<?xml version=\"1.0\"?><plist version=\"1.0\"><dict>\
              <key>Name</key><string>Signum</string>\
              <key>ExpirationDate</key><date>2026-09-11T12:00:00Z</date>\
              </dict></plist>",
        );
        raw.extend_from_slice(&[0xff, 0x00, 0x01]);
        let path = write("valid.mobileprovision", &raw);
        assert_eq!(
            provision_expires_at(&path).as_deref(),
            Some("2026-09-11T12:00:00Z")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_profile_without_the_key_reports_nothing() {
        let path = write("empty.mobileprovision", b"\x30\x82 no plist here");
        assert_eq!(provision_expires_at(&path), None);
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod p12_tests {
    use super::*;
    use std::io::Write;

    fn write_temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        path
    }

    #[test]
    fn a_der_certificate_is_not_mistaken_for_p12() {
        // Настоящий случай из поддержки: человек выбрал не тот файл, а ему
        // сказали «неверный пароль» — потому что проверялся один байт 0x30,
        // с которого начинается ЛЮБОЙ DER. У сертификата за заголовком снова
        // последовательность, а не версия PKCS#12.
        let cer = [0x30u8, 0x82, 0x01, 0x00, 0x30, 0x81, 0xaa, 0xa0, 0x03];
        let path = write_temp("signum-test.cer", &cer);
        assert!(matches!(p12_shape(&path), P12Shape::NotDer));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_real_p12_is_recognised() {
        // У PKCS#12 сразу за внешней последовательностью версия: 02 01 03.
        let short = [0x30u8, 0x20, 0x02, 0x01, 0x03, 0x30, 0x10];
        let path = write_temp("signum-test-short.p12", &short);
        assert!(matches!(p12_shape(&path), P12Shape::LooksLikeP12));
        let _ = fs::remove_file(&path);

        // Длинная форма длины: два байта на саму длину.
        let long = [0x30u8, 0x82, 0x0a, 0x00, 0x02, 0x01, 0x03, 0x30];
        let path = write_temp("signum-test-long.p12", &long);
        assert!(matches!(p12_shape(&path), P12Shape::LooksLikeP12));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_pem_file_is_named_as_pem() {
        let path = write_temp("signum-test.pem", b"-----BEGIN PRIVATE KEY-----\n");
        assert!(matches!(p12_shape(&path), P12Shape::Pem));
        let _ = fs::remove_file(&path);
    }
}

#[cfg(test)]
mod outage_tests {
    use super::*;

    #[test]
    fn app_id_containing_503_is_not_an_outage() {
        // Настоящий случай из поддержки: идентификатор 6503087901 содержит
        // «503», и проверка на голое вхождение объявляла отказ сервиса там,
        // где Apple просто не нашла приложение.
        assert!(!is_apple_outage("purchase error: app not found (id 6503087901)"));
        assert!(!is_apple_outage("list-versions -i 6803087901 failed"));
    }

    #[test]
    fn real_outage_is_recognised() {
        assert!(is_apple_outage("gsa.apple.com returned 503"));
        assert!(is_apple_outage("http status: 502 bad gateway"));
        assert!(is_apple_outage("service temporarily unavailable"));
    }
}
