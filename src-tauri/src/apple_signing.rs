                                                                                                                         
   
                     
                                                               
                                                             
                                         
   
                                                                       

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
    /// Когда подпись перестанет работать (профиль или сертификат), ISO-8601.
    pub expires_at: Option<String>,
    pub expires_unix: Option<u64>,
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
    validate_cert_pair(p12_src, prov_src, password)?;
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

/// Проверить пару .p12 + .mobileprovision ещё при сохранении: неверный
/// пароль или профиль от другого сертификата раньше всплывали только при
/// подписи, а то и при установке на iPhone.
fn validate_cert_pair(p12: &Path, prov: &Path, password: &str) -> Result<(), String> {
    let raw = fs::read(prov).map_err(|e| format!("Не удалось прочитать профиль: {e}"))?;
    let profile = crate::signing_profile::parse_profile(&raw)
        .map_err(|e| format!("Файл .mobileprovision не читается ({e})."))?;
    match read_p12(p12, password) {
        P12Check::WrongPassword => Err(p12_shape_message(p12).into()),
        P12Check::Ok(identity) => {
            if profile.includes_certificate(&identity.cert_der) {
                Ok(())
            } else {
                Err("Сертификат .p12 и профиль .mobileprovision не пара: профиль выписан на другой сертификат.".into())
            }
        }
        // Редкий формат шифрования: не мешаем сохранить, zsign может справиться.
        P12Check::Unreadable(_) => match p12_shape(p12) {
            P12Shape::LooksLikeP12 => Ok(()),
            _ => Err(p12_shape_message(p12).into()),
        },
    }
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

                                                                                  
/// Ждём код 2FA из окна Signum (Signing2faGate).
///
/// Асинхронно: раньше ожидание блокировало поток рантайма на три минуты.
/// В ответ можно прислать код, `__resend__` (ещё раз), `__sms__` (прислать SMS)
/// или пустую строку — отмена.
async fn wait_for_signing_2fa_code(
    app: &tauri::AppHandle,
    params: &isideload::auth::apple_account::TwoFactorCallbackParams,
) -> isideload::auth::apple_account::TwoFactorCallbackResponse {
    use isideload::auth::apple_account::TwoFactorCallbackResponse;
    use tauri::{Emitter, Manager};

    let first_number = params
        .selected_number_id
        .or_else(|| params.numbers.first().map(|n| n.id));

    // Apple не сказала, каким способом подтверждать (прошлый способ не
    // сработал): код ввести некуда, поэтому сразу выбираем доставку.
    if params.unknown {
        return match first_number {
            Some(id) => TwoFactorCallbackResponse::SendSms(id),
            None => TwoFactorCallbackResponse::SendToDevices,
        };
    }

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    match app.try_state::<crate::AppState>() {
        Some(state) => {
            if let Ok(mut slot) = state.signing_2fa_reply.lock() {
                *slot = Some(tx);
            }
        }
        None => return TwoFactorCallbackResponse::Abort,
    }

    let phone_hint = params
        .numbers
        .iter()
        .find(|n| Some(n.id) == params.selected_number_id)
        .or_else(|| params.numbers.first())
        .map(|n| n.number_with_dial_code.clone());
    let mut message = if params.sms {
        match &phone_hint {
            Some(phone) => format!("Apple отправил SMS на {phone}. Введите 6-значный код."),
            None => "Apple отправил SMS. Введите 6-значный код.".to_string(),
        }
    } else {
        "Apple отправил код на iPhone. Введите 6 цифр.".to_string()
    };
    if params.last_error.is_some() {
        message = format!("Код не подошёл или устарел. {message}");
    }
    let _ = app.emit(
        "signing-2fa-required",
        serde_json::json!({
            "message": message,
            "sms": params.sms,
            "can_sms": first_number.is_some() && !params.sms,
            "retry": params.last_error.is_some(),
        }),
    );

    let reply = tokio::time::timeout(Duration::from_secs(300), rx).await;
    let response = match reply {
        Ok(Ok(code)) if code == "__resend__" => TwoFactorCallbackResponse::ResendCode,
        Ok(Ok(code)) if code == "__sms__" => match first_number {
            Some(id) => TwoFactorCallbackResponse::SendSms(id),
            None => TwoFactorCallbackResponse::ResendCode,
        },
        Ok(Ok(code)) => {
            let digits = only_code_digits(&code);
            if digits.len() == 6 {
                TwoFactorCallbackResponse::SubmitCode(digits)
            } else {
                TwoFactorCallbackResponse::Abort
            }
        }
        // Окно закрыли / время вышло.
        _ => TwoFactorCallbackResponse::Abort,
    };
    if matches!(response, TwoFactorCallbackResponse::Abort) {
        if let Some(state) = app.try_state::<crate::AppState>() {
            if let Ok(mut slot) = state.signing_2fa_reply.lock() {
                *slot = None;
            }
        }
        let _ = app.emit("signing-2fa-closed", serde_json::json!({}));
    }
    response
}

fn only_code_digits(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_digit()).take(6).collect()
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
    if s.trim().is_empty() {
        return "SIGN_IPA_CERT_FAILED: Не удалось подписать файл. Повторите попытку или войдите другим Apple ID.".into();
    }
    map_isideload_err(s)
}

const APP_ID_LIMIT_TEXT: &str = "SIGN_IPA_APP_ID_LIMIT: На этом Apple ID кончился лимит App ID — бесплатный аккаунт даёт 10 штук на 7 дней, и каждое расширение внутри приложения занимает ещё один. Включите «Удалить плагины», подождите, пока старые App ID освободятся, или войдите другим Apple ID.";

/// «Not enough available app IDs. 3 are required, but only 1 is available.»
fn is_app_id_limit(lower: &str) -> bool {
    lower.contains("not enough available app id")
        || (lower.contains("app id") && lower.contains("required") && lower.contains("available"))
        // 9401: «Cannot create more than 10 App IDs within 7 days» и т.п.
        || lower.contains("maximum app id")
        || (lower.contains("app ids") && lower.contains("7 days"))
}

/// Перехваченное TLS-соединение (антивирус с HTTPS-проверкой, корпоративный
/// прокси). Это не про сертификат разработчика, хотя слово то же.
fn is_tls_interception(lower: &str) -> bool {
    lower.contains("invalid peer certificate")
        || lower.contains("unknownissuer")
        || lower.contains("unknown issuer")
        || lower.contains("certificate verify failed")
        || lower.contains("self signed certificate")
        || lower.contains("self-signed certificate")
        || lower.contains("badsignature")
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
    if is_app_id_limit(&lower) {
        return APP_ID_LIMIT_TEXT.into();
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
        || lower.contains("aborting")
    {
        return "SIGN_IPA_DEV_LOGIN_FAILED: Apple ждёт код подтверждения (2FA). Введите его в окне Signum, которое появится во время подписи.".into();
    }
    if lower.contains("-20101")
        || lower.contains("-22406")
        || lower.contains("password")
        || lower.contains("credential")
    {
        return "SIGN_IPA_DEV_LOGIN_FAILED: Apple не принял пароль. Выйдите и войдите снова в «Аккаунт».".into();
    }
    if is_tls_interception(&lower) {
        return "SIGN_IPA_NETWORK: Защищённое соединение с Apple подменено — обычно это антивирус с проверкой HTTPS или прокси. Отключите проверку HTTPS (или добавьте Signum в исключения) и повторите.".into();
    }
    if lower.contains("anisette") {
        return "SIGN_IPA_DEV_LOGIN_FAILED: Apple не подтвердила этот компьютер. Проверьте интернет и повторите.".into();
    }
    // 503/502 от gsa.apple.com — это отказ на стороне Apple, а не наша ошибка и
    // не проблема пароля. Без этой ветки текст падал в общий случай и
    // показывался пользователю сырым дампом.
    if is_apple_outage(&lower) {
        return "SIGN_IPA_DEV_LOGIN_FAILED: Apple временно не отвечает (сервис входа перегружен). Это не ошибка пароля — подождите несколько минут и повторите."
            .into();
    }
    if lower.contains("no developer teams") || lower.contains("no team selected") {
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
    // Сеть проверяем до «certificate»: обёртка «Failed to retrieve certificate
    // identity» иначе выдавала обрыв связи за проблему сертификата.
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
    if lower.contains("7460") || lower.contains("maximum number of certificates") {
        return "SIGN_IPA_CERT_FAILED: на Apple ID уже выпущен максимум сертификатов разработки, и освободить место не удалось. Отзовите лишний сертификат на developer.apple.com (Certificates) или подождите и повторите.".into();
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
        auth::apple_account::{AppleAccount, TwoFactorCallbackParams},
        dev::{developer_session::DeveloperSession, devices::DevicesApi},
        sideload::{
            builder::{ExtensionsBehavior, ExtensionsBehaviorSelection, MaxCertsBehavior},
            SideloaderBuilder, TeamSelection,
        },
        util::{
            callbacks::{ExtensionsBehaviorCallbackBox, MaxCertsCallbackBox},
            fs_storage::FsStorage,
        },
    };
    if !ipa_path.is_file() {
        return Err("SIGN_IPA_FILE_REQUIRED".into());
    }
    let udid = udid.map(str::trim).filter(|v| !v.is_empty());
    if email.trim().is_empty() || password.is_empty() {
        return Err("SIGN_IPA_AUTH_REQUIRED".into());
    }

    // Повторный вызов возвращает ошибку «hooks already installed» — это норма.
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
    let prepared = prepare_app_dir_with_options(ipa_path, &work_dir, &mut metadata, options)?;
    // Watch-приложения, App Clip и ExtensionKit подписчик не трогает, а
    // неподписанный вложенный бандл iOS не установит вовсе.
    strip_unsignable_nested(&prepared.app_dir);

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
        let two_factor = move |params: TwoFactorCallbackParams| {
            let app = app_for_2fa.clone();
            async move {
                Ok::<_, rootcause::Report>(wait_for_signing_2fa_code(&app, &params).await)
            }
        };
        // danger_debug(true) отключал проверку TLS-сертификатов Apple: пароль
        // Apple ID уходил бы и через подменённое соединение. isideload 0.5
        // сам доверяет корневому сертификату Apple, отладка не нужна.
        match AppleAccount::builder(email.trim())
            .danger_debug(false)
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
    let mut account = match account {
        Some(account) => account,
        None => return Err(last_error),
    };

    progress(22.0, "Получаем разрешение Apple…");
    let mut dev_session = DeveloperSession::from_account(&mut account)
        .await
        .map_err(map_isideload_err)?;

    progress(30.0, "Выбираем аккаунт разработчика…");
    let team = select_team(&mut dev_session, cert_mode).await?;

    if let Some(udid) = udid {
        progress(38.0, "Регистрируем устройство…");
        dev_session
            .ensure_device_registered(&team, device_name, udid, None)
            .await
            .map_err(map_sign_app_err)?;
    } else {
        progress(38.0, "Проверяем iPhone в аккаунте…");
        let known = dev_session
            .list_devices(&team, None)
            .await
            .map_err(map_sign_app_err)?;
        if known.is_empty() {
            return Err(
                "SIGN_IPA_NO_TEAM_DEVICE: Для подписи Apple нужна хотя бы одна регистрация iPhone на этом Apple ID. Подключите iPhone кабелем, нажмите «Доверять» и повторите. Потом файл можно сохранять и без телефона.".into(),
            );
        }
    }

    let mut sideloader =
        SideloaderBuilder::<MaxCertsCallbackBox, ExtensionsBehaviorCallbackBox>::new(
            dev_session,
            email.trim().to_string(),
        )
        .team_selection(TeamSelection::First)
        .max_certs_behavior(MaxCertsBehavior::Revoke)
        // Каждому расширению — свой App ID и профиль: с чужим профилем
        // расширение iOS не примет. Лимит App ID при этом расходуется быстрее,
        // поэтому при нехватке объясняем, что делать.
        .extensions_behavior(ExtensionsBehaviorSelection::Fixed(
            ExtensionsBehavior::RegisterAll,
        ))
        .storage(Box::new(FsStorage::new(cert_store)))
        .machine_name(SIGNUM_MACHINE_NAME.into())
        .delete_app_after_install(false)
        .build();

    progress(48.0, "Выпускаем сертификат и профиль…");
    let sign_fut = sideloader.sign_app(
        prepared.app_dir.clone(),
        Some(team.clone()),
        false,
        None::<fn(f32) -> std::future::Ready<()>>,
        None,
        udid,
    );
    // Сама подпись большого приложения (сотни МБ) идёт минуты на слабом ПК,
    // поэтому запас щедрый: таймаут ловит зависшую сеть, а не медленный диск.
    let sign_result = tokio::time::timeout(Duration::from_secs(600), sign_fut)
        .await
        .map_err(|_| {
            "SIGN_IPA_NETWORK: Apple слишком долго не отвечает при подписи. Проверьте VPN/интернет и повторите."
                .to_string()
        })?;
    let (bundle_dir, _special) = match sign_result {
        Ok(done) => done,
        Err(error) => {
            let mapped = map_sign_app_err(error);
            if mapped.starts_with("SIGN_IPA_APP_ID_LIMIT") {
                if let Some(when) = next_app_id_release(sideloader.get_dev_session(), &team).await
                {
                    return Err(format!(
                        "{mapped} Ближайший App ID освободится {}.",
                        crate::signing_profile::unix_to_ru_date(when)
                    ));
                }
            }
            return Err(mapped);
        }
    };

    // Когда подпись перестанет работать — из профиля, который выдала Apple:
    // у бесплатного аккаунта неделя, у платного — до года.
    let expires_unix = fs::read(bundle_dir.join("embedded.mobileprovision"))
        .ok()
        .and_then(|raw| crate::signing_profile::parse_profile(&raw).ok())
        .and_then(|profile| profile.expires_unix);

    progress(75.0, "Собираем приложение…");
    pack_app_dir_to_ipa(&bundle_dir, output_ipa)?;
    drop(prepared);

    // Тип команды Apple отдаёт не всегда; надёжнее срок профиля: бесплатный
    // выдаётся на 7 дней, платный — на год.
    let team_kind = match expires_unix {
        Some(at) if at <= crate::signing_profile::unix_now() + 8 * 86_400 => "free".to_string(),
        _ => team_kind_label(&team),
    };
    let team_name = team.name.clone().unwrap_or_else(|| "Team".into());

    progress(90.0, "Подпись готова");
    Ok(SignedIpaResult {
        signed_ipa: output_ipa.to_path_buf(),
        metadata,
        team_id: team.team_id.clone(),
        team_name,
        team_kind,
        expires_at: expires_unix.map(crate::signing_profile::unix_to_iso),
        expires_unix,
    })
}

/// Имя «машины» в сертификате разработки: по нему Signum узнаёт свои
/// сертификаты и при нехватке места отзывает в первую очередь их.
const SIGNUM_MACHINE_NAME: &str = "Signum";

/// Когда освободится ближайший App ID (бесплатный аккаунт: 7 дней от создания).
async fn next_app_id_release(
    session: &mut isideload::dev::developer_session::DeveloperSession,
    team: &isideload::dev::teams::DeveloperTeam,
) -> Option<u64> {
    use isideload::dev::app_ids::AppIdsApi;
    let list = session.list_app_ids(team, None).await.ok()?;
    let now = crate::signing_profile::unix_now();
    list.app_ids
        .iter()
        .filter_map(|id| id.expiration_date)
        .filter_map(|d| {
            std::time::SystemTime::from(d)
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
        })
        .map(|d| d.as_secs())
        .filter(|at| *at > now)
        .min()
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

/// Сертификат и ключ, вынутые из .p12.
struct P12Identity {
    key_der: Vec<u8>,
    cert_der: Vec<u8>,
    chain_der: Vec<Vec<u8>>,
}

enum P12Check {
    Ok(P12Identity),
    WrongPassword,
    /// Не удалось разобрать: не .p12 или редкий формат шифрования.
    Unreadable(String),
}

fn read_p12(path: &Path, password: &str) -> P12Check {
    use p12_keystore::{error::Error as P12Error, KeyStore, Pkcs12ImportPolicy};
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => return P12Check::Unreadable(e.to_string()),
    };
    match KeyStore::from_pkcs12(&bytes, password, Pkcs12ImportPolicy::Relaxed) {
        Ok(store) => match store.private_key_chain() {
            Some((_, chain)) if !chain.certs().is_empty() => P12Check::Ok(P12Identity {
                key_der: chain.key().as_der().to_vec(),
                cert_der: chain.certs()[0].as_der().to_vec(),
                chain_der: chain.certs()[1..]
                    .iter()
                    .map(|c| c.as_der().to_vec())
                    .collect(),
            }),
            _ => P12Check::Unreadable("в .p12 нет закрытого ключа с сертификатом".into()),
        },
        Err(P12Error::MacError(_)) => P12Check::WrongPassword,
        Err(e) => P12Check::Unreadable(e.to_string()),
    }
}

/// Текст для «файл не тот», а не «пароль не тот».
fn p12_shape_message(path: &Path) -> &'static str {
    match p12_shape(path) {
        P12Shape::Pem => {
            "Это не .p12, а текстовый PEM-файл. Экспортируйте сертификат из «Связки ключей» как «Личный обмен информацией (.p12)»."
        }
        P12Shape::NotDer => {
            "Это не файл .p12. Похоже, выбран сертификат .cer или профиль .mobileprovision — а нужен .p12, в котором лежит и ключ."
        }
        P12Shape::Empty => "Файл сертификата пустой. Выберите .p12 заново.",
        P12Shape::LooksLikeP12 => "Неверный пароль от сертификата .p12. Проверьте его и повторите.",
    }
}

/// Подпись своим сертификатом (.p12 + .mobileprovision), без Apple ID:
/// годовые / enterprise сертификаты.
///
/// До подписи проверяем то, что раньше всплывало только ошибкой установки на
/// iPhone: срок профиля и сертификата, пару сертификат—профиль, UDID в
/// профиле и Bundle ID. Подписывает zsign, если он есть (Windows), иначе — или
/// если zsign не справился — встроенный подписчик.
pub async fn sign_ipa_with_custom_cert(
    _app: &tauri::AppHandle,
    ipa_path: &Path,
    p12_path: &Path,
    mprov_path: &Path,
    password: &str,
    udid: Option<&str>,
    options: &SignOptions,
    zsign_exe: Option<&Path>,
    cache_dir: &Path,
    output_ipa: &Path,
    mut progress: impl FnMut(f64, &str),
) -> Result<SignedIpaResult, String> {
    use crate::signing_profile::{
        certificate_not_after, earliest, parse_profile, unix_now, unix_to_iso, unix_to_ru_date,
    };
    if !ipa_path.is_file() {
        return Err("SIGN_IPA_FILE_REQUIRED".into());
    }
    if !p12_path.is_file() {
        return Err("Файл .p12 не найден. Выберите его заново.".into());
    }
    if !mprov_path.is_file() {
        return Err("Файл .mobileprovision не найден. Выберите его заново.".into());
    }

    progress(3.0, "Проверяем сертификат и профиль…");
    let profile_raw =
        fs::read(mprov_path).map_err(|e| format!("Не удалось прочитать .mobileprovision: {e}"))?;
    let profile = parse_profile(&profile_raw)
        .map_err(|e| format!("Файл .mobileprovision не читается ({e}). Выберите профиль заново."))?;
    let profile_name = if profile.name.is_empty() {
        "профиль".to_string()
    } else {
        format!("профиль «{}»", profile.name)
    };
    let now = unix_now();
    if profile.is_expired(now) {
        let date = profile.expires_unix.map(unix_to_ru_date).unwrap_or_default();
        return Err(format!(
            "SIGN_IPA_PROFILE_EXPIRED: {profile_name} истёк {date}. Нужен новый .mobileprovision от того, кто выдал сертификат."
        ));
    }
    if let Some(udid) = udid.map(str::trim).filter(|v| !v.is_empty()) {
        if !profile.allows_device(udid) {
            return Err(format!(
                "SIGN_IPA_DEVICE_NOT_IN_PROFILE: Этот iPhone (UDID {udid}) не добавлен в {profile_name}. Приложение не установится: попросите владельца сертификата добавить UDID и прислать новый .mobileprovision."
            ));
        }
    }

    let zsign = zsign_exe.filter(|p| p.is_file());
    let identity = match read_p12(p12_path, password) {
        P12Check::Ok(identity) => Some(identity),
        P12Check::WrongPassword => {
            return Err(format!(
                "{}\n\nПодробности: проверка пароля .p12 не прошла.",
                p12_shape_message(p12_path)
            ))
        }
        // Редкое шифрование .p12 может оказаться по силам zsign — тогда не
        // мешаем ему. Без zsign подписать нечем.
        P12Check::Unreadable(detail) if zsign.is_some() => {
            eprintln!("custom cert: p12 not parsed natively ({detail}), leaving it to zsign");
            None
        }
        P12Check::Unreadable(detail) => {
            return Err(format!("{}\n\nПодробности: {detail}", p12_shape_message(p12_path)))
        }
    };
    let mut cert_expires = None;
    if let Some(identity) = &identity {
        cert_expires = certificate_not_after(&identity.cert_der);
        if cert_expires.is_some_and(|at| at <= now) {
            return Err(format!(
                "SIGN_IPA_CERT_EXPIRED: Сертификат .p12 истёк {}. Подписанное им приложение не запустится — нужен действующий сертификат.",
                cert_expires.map(unix_to_ru_date).unwrap_or_default()
            ));
        }
        if !profile.includes_certificate(&identity.cert_der) {
            return Err(format!(
                "SIGN_IPA_CERT_PROFILE_MISMATCH: Сертификат .p12 и {profile_name} не пара: профиль выписан на другой сертификат. Возьмите .mobileprovision, который выдали вместе с этим .p12."
            ));
        }
    }

    progress(8.0, "Читаем файл…");
    let mut metadata = parse_ipa_metadata(ipa_path)?;

    fs::create_dir_all(cache_dir).map_err(|e| format!("Не удалось создать кэш подписи: {e}"))?;
    let work_dir = cache_dir.join("work-custom");
    fs::create_dir_all(&work_dir).map_err(|e| e.to_string())?;

    progress(12.0, "Готовим приложение…");
    let prepared = prepare_app_dir_with_options(ipa_path, &work_dir, &mut metadata, options)?;

    // Явный профиль подходит ровно одному Bundle ID. С другим идентификатором
    // приложение подписывалось, но iPhone отказывал в установке.
    if !profile.allows_bundle(&metadata.bundle_id) {
        let wanted = profile.bundle_id_for(&metadata.bundle_id);
        progress(20.0, &format!("Bundle ID под профиль: {wanted}"));
        set_bundle_id(&prepared.app_dir, &metadata.bundle_id, &wanted)?;
        metadata.bundle_id = wanted;
    }

    progress(30.0, "Подписываем сертификатом…");
    let mut zsign_error: Option<String> = None;
    if let Some(zsign) = zsign {
        let prepared_ipa = work_dir.join(format!(
            "{}.ipa",
            prepared
                .root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("prepared")
        ));
        pack_dir_to_ipa(&prepared.root, &prepared_ipa)?;
        let result =
            run_zsign(zsign, p12_path, mprov_path, password, &prepared_ipa, output_ipa).await;
        let _ = fs::remove_file(&prepared_ipa);
        match result {
            Ok(()) => {}
            // Неверный пароль/файл zsign не исправит и встроенный подписчик.
            Err(ZsignError::Credentials(detail)) if identity.is_none() => {
                return Err(format!(
                    "{}\n\nПодробности: {detail}",
                    p12_shape_message(p12_path)
                ));
            }
            Err(ZsignError::Credentials(detail)) | Err(ZsignError::Other(detail)) => {
                zsign_error = Some(detail);
            }
        }
    }
    let signed_by_zsign = zsign.is_some() && zsign_error.is_none();
    if !signed_by_zsign {
        let Some(identity) = identity else {
            let detail = zsign_error.unwrap_or_default();
            let lowered = detail.to_ascii_lowercase();
            let friendly = if lowered.contains("provision") {
                "Файл .mobileprovision не подошёл к сертификату. Проверьте оба файла."
            } else {
                "Не удалось подписать IPA этим сертификатом. Проверьте, что файлы не повреждены."
            };
            return Err(format!("{friendly}\n\nПодробности: {detail}"));
        };
        if zsign_error.is_some() {
            progress(45.0, "Пробуем встроенный подписчик…");
        }
        let app_dir = prepared.app_dir.clone();
        let bundle_id = metadata.bundle_id.clone();
        let raw = profile_raw.clone();
        tokio::task::spawn_blocking(move || sign_bundle_with_p12(&app_dir, &raw, &identity, &bundle_id))
            .await
            .map_err(|e| format!("Не удалось запустить подпись: {e}"))?
            .map_err(|e| match &zsign_error {
                Some(z) => format!("Не удалось подписать IPA этим сертификатом.\n\nПодробности: {e}\nzsign: {z}"),
                None => format!("Не удалось подписать IPA этим сертификатом.\n\nПодробности: {e}"),
            })?;
        progress(75.0, "Собираем приложение…");
        pack_app_dir_to_ipa(&prepared.app_dir, output_ipa)?;
    }
    drop(prepared);

    let expires_unix = earliest(profile.expires_unix, cert_expires);
    progress(90.0, "Подпись готова");
    Ok(SignedIpaResult {
        signed_ipa: output_ipa.to_path_buf(),
        metadata,
        team_id: profile.team_id.clone(),
        team_name: "Свой сертификат".into(),
        team_kind: "custom".into(),
        expires_at: expires_unix.map(unix_to_iso),
        expires_unix,
    })
}

enum ZsignError {
    /// zsign говорит про пароль / pkcs12.
    Credentials(String),
    Other(String),
}

async fn run_zsign(
    zsign: &Path,
    p12_path: &Path,
    mprov_path: &Path,
    password: &str,
    input_ipa: &Path,
    output_ipa: &Path,
) -> Result<(), ZsignError> {
    // Bundle ID, имя и версия уже записаны в Info.plist (в том числе у
    // расширений), поэтому -b/-n/-r не передаём.
    let mut args: Vec<String> = Vec::new();
    // -p только если пароль непустой — иначе zsign падает на пустом.
    if !password.is_empty() {
        args.push("-p".into());
        args.push(password.to_string());
    }
    args.extend([
        "-k".to_string(),
        p12_path.to_string_lossy().into_owned(),
        "-m".to_string(),
        mprov_path.to_string_lossy().into_owned(),
        "-z".to_string(),
        "9".to_string(),
        "-o".to_string(),
        output_ipa.to_string_lossy().into_owned(),
        input_ipa.to_string_lossy().into_owned(),
    ]);
    let _ = fs::remove_file(output_ipa);

    let exe = zsign.to_path_buf();
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
    .map_err(|e| ZsignError::Other(format!("Не удалось запустить zsign: {e}")))?
    .map_err(|e| ZsignError::Other(format!("Не удалось запустить zsign: {e}")))?;

    if output.status.success() && output_ipa.is_file() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = format!("{stderr}\n{stdout}")
        .trim()
        .chars()
        .take(600)
        .collect::<String>();
    let lowered = detail.to_ascii_lowercase();
    // zsign на любую неудачу с сертификатом отвечает «input the correct file
    // and password», поэтому слово password не значит, что дело в пароле.
    if lowered.contains("password") || lowered.contains("pkcs12") {
        Err(ZsignError::Credentials(detail))
    } else {
        Err(ZsignError::Other(detail))
    }
}

/// Встроенная подпись (тот же движок, что у isideload 0.5) сертификатом из .p12.
fn sign_bundle_with_p12(
    app_dir: &Path,
    profile_raw: &[u8],
    identity: &P12Identity,
    bundle_id: &str,
) -> Result<(), String> {
    use apple_codesign_quick::{sign_bundle, BundleSigningSettings, ProvisioningProfile, RustCryptoCmsSigner};

    let parsed = ProvisioningProfile::parse(profile_raw).map_err(|e| e.to_string())?;
    let mut chain = identity.chain_der.clone();
    chain.extend(parsed.certificate_chain_der().iter().cloned());
    let signer = RustCryptoCmsSigner::from_der(&identity.key_der, &identity.cert_der, chain)
        .map_err(|e| e.to_string())?;
    let team_id = parsed.team_id().to_string();
    let base = parsed.entitlements().clone();

    let mut settings = BundleSigningSettings::new(
        team_id.clone(),
        entitlements_for_bundle(&base, &team_id, bundle_id),
        Some(&signer),
    );
    settings.embedded_mobileprovision = Some(profile_raw);
    for nested in nested_bundle_ids(app_dir) {
        settings
            .entitlements_by_bundle_id
            .insert(nested.clone(), entitlements_for_bundle(&base, &team_id, &nested));
        settings
            .embedded_mobileprovisions_by_bundle_id
            .insert(nested, profile_raw);
    }
    sign_bundle(app_dir, &settings).map_err(|e| e.to_string())
}

/// Права из профиля для конкретного бандла: в шаблонном профиле
/// `application-identifier` = `TEAM.*`, а в подписи должен стоять полный ID.
fn entitlements_for_bundle(
    base: &plist::Dictionary,
    team_id: &str,
    bundle_id: &str,
) -> plist::Dictionary {
    let mut out = base.clone();
    for key in ["application-identifier", "com.apple.application-identifier"] {
        let wildcard = out
            .get(key)
            .and_then(|v| v.as_string())
            .is_some_and(|v| v.ends_with('*'));
        if wildcard {
            out.insert(
                key.to_string(),
                plist::Value::String(format!("{team_id}.{bundle_id}")),
            );
        }
    }
    out
}

/// Идентификаторы расширений (PlugIns/*.appex), которые подписываются отдельно.
fn nested_bundle_ids(app_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(app_dir.join("PlugIns")) else {
        return out;
    };
    for entry in entries.flatten() {
        let plist_path = entry.path().join("Info.plist");
        if let Ok(value) = plist::Value::from_file(&plist_path) {
            if let Some(id) = value
                .as_dictionary()
                .and_then(|d| d.get("CFBundleIdentifier"))
                .and_then(|v| v.as_string())
            {
                out.push(id.to_string());
            }
        }
    }
    out
}

                                                                                            
/// Распакованное и подготовленное приложение. Папка удаляется сама, когда
/// значение выходит из области видимости — в том числе на любой ошибке.
struct PreparedApp {
    /// Корень распаковки (в нём лежит `Payload/`).
    root: PathBuf,
    /// `Payload/<Name>.app`.
    app_dir: PathBuf,
}

impl Drop for PreparedApp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Распаковать IPA и применить опции подписи (Info.plist, удаление плагинов…).
///
/// Подписчик получает готовую папку `.app`: раньше её снова упаковывали в
/// IPA, а isideload тут же распаковывал обратно — двойная работа и потеря
/// прав доступа к файлам на Windows.
fn prepare_app_dir_with_options(
    ipa_path: &Path,
    work_dir: &Path,
    metadata: &mut IpaMetadata,
    options: &SignOptions,
) -> Result<PreparedApp, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let root = work_dir.join(format!("extract-{stamp}"));
    if root.exists() {
        let _ = fs::remove_dir_all(&root);
    }
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut prepared = PreparedApp {
        root: root.clone(),
        app_dir: PathBuf::new(),
    };

    {
        let file = fs::File::open(ipa_path).map_err(|e| e.to_string())?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("Не удалось прочитать IPA: {e}"))?;
        archive
            .extract(&root)
            .map_err(|e| format!("Не удалось распаковать IPA: {e}"))?;
    }

    let app_dir = find_app_bundle(&root.join("Payload"))?;
    prepared.app_dir = app_dir.clone();

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

    let original_bundle_id = metadata.bundle_id.clone();
    patch_info_plist(&app_dir.join("Info.plist"), metadata, options)?;
    // Сменили Bundle ID приложения — расширения должны остаться «детьми»
    // нового идентификатора, иначе Apple отвергает весь пакет
    // («is not part of the main app bundle identifier»).
    if metadata.bundle_id != original_bundle_id {
        rebase_nested_bundle_ids(&app_dir, &original_bundle_id, &metadata.bundle_id)?;
    }
    Ok(prepared)
}

/// Убрать то, что подписчик Apple ID не подписывает: Watch-приложения,
/// App Clip и ExtensionKit-расширения. Неподписанный вложенный бандл ломает
/// установку всего приложения, а работать они без своих профилей не будут.
fn strip_unsignable_nested(app_dir: &Path) {
    for dir in [
        "Watch",
        "WatchKit",
        "WatchKitSupport",
        "WatchKitSupport2",
        "com.apple.WatchPlaceholder",
        "AppClips",
        "Extensions",
    ] {
        let path = app_dir.join(dir);
        if path.exists() {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Поменять Bundle ID у уже подготовленного приложения (вместе с расширениями).
fn set_bundle_id(app_dir: &Path, old: &str, new: &str) -> Result<(), String> {
    let path = app_dir.join("Info.plist");
    let mut value = plist::Value::from_file(&path).map_err(|e| format!("Info.plist: {e}"))?;
    let dict = value
        .as_dictionary_mut()
        .ok_or_else(|| "Info.plist: не словарь".to_string())?;
    dict.insert(
        "CFBundleIdentifier".into(),
        plist::Value::String(new.to_string()),
    );
    value
        .to_file_xml(&path)
        .map_err(|e| format!("Info.plist write: {e}"))?;
    rebase_nested_bundle_ids(app_dir, old, new)
}

/// `old.ext` -> `new.ext` во всех вложенных бандлах (PlugIns, Extensions,
/// AppClips, Watch и их PlugIns) плюс ссылки Watch на «родителя».
fn rebase_nested_bundle_ids(app_dir: &Path, old: &str, new: &str) -> Result<(), String> {
    if old.is_empty() || old == new {
        return Ok(());
    }
    let mut stack = vec![app_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(ext.as_str(), "appex" | "app") {
                rebase_one_plist(&path.join("Info.plist"), old, new)?;
                stack.push(path);
            } else if matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("PlugIns" | "Extensions" | "AppClips" | "Watch")
            ) {
                stack.push(path);
            }
        }
    }
    Ok(())
}

fn rebase_id(value: &str, old: &str, new: &str) -> Option<String> {
    if value == old {
        return Some(new.to_string());
    }
    value
        .strip_prefix(old)
        .filter(|rest| rest.starts_with('.'))
        .map(|rest| format!("{new}{rest}"))
}

fn rebase_one_plist(path: &Path, old: &str, new: &str) -> Result<(), String> {
    let Ok(mut value) = plist::Value::from_file(path) else {
        return Ok(());
    };
    let Some(dict) = value.as_dictionary_mut() else {
        return Ok(());
    };
    let mut changed = false;
    for key in ["CFBundleIdentifier", "WKCompanionAppBundleIdentifier"] {
        if let Some(updated) = dict
            .get(key)
            .and_then(|v| v.as_string())
            .and_then(|v| rebase_id(v, old, new))
        {
            dict.insert(key.into(), plist::Value::String(updated));
            changed = true;
        }
    }
    if let Some(attrs) = dict
        .get_mut("NSExtension")
        .and_then(|v| v.as_dictionary_mut())
        .and_then(|d| d.get_mut("NSExtensionAttributes"))
        .and_then(|v| v.as_dictionary_mut())
    {
        if let Some(updated) = attrs
            .get("WKAppBundleIdentifier")
            .and_then(|v| v.as_string())
            .and_then(|v| rebase_id(v, old, new))
        {
            attrs.insert(
                "WKAppBundleIdentifier".into(),
                plist::Value::String(updated),
            );
            changed = true;
        }
    }
    if changed {
        value
            .to_file_xml(path)
            .map_err(|e| format!("Не удалось записать {}: {e}", path.display()))?;
    }
    Ok(())
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
            zip.start_file(rel, zip_options_for(&path, &buf, opts)).map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn select_team(
    session: &mut isideload::dev::developer_session::DeveloperSession,
    mode: CertMode,
) -> Result<isideload::dev::teams::DeveloperTeam, String> {
    use isideload::dev::teams::TeamsApi;

    let teams = session
        .list_teams()
        .await
        .map_err(|e| format!("SIGN_IPA_TEAM_NONE: {}", report_text(e)))?;
    pick_team(&teams, mode)
        .cloned()
        .ok_or_else(|| "SIGN_IPA_TEAM_NONE".to_string())
}

fn pick_team<'a>(
    teams: &'a [isideload::dev::teams::DeveloperTeam],
    mode: CertMode,
) -> Option<&'a isideload::dev::teams::DeveloperTeam> {
    let is_free = team_is_free;
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

fn team_is_free(team: &isideload::dev::teams::DeveloperTeam) -> bool {
    let ty = team.r#type.as_deref().unwrap_or("").to_ascii_lowercase();
    let name = team.name.as_deref().unwrap_or("").to_ascii_lowercase();
    // Бесплатная команда Apple ID называется «Имя (Personal Team)».
    ty.contains("free") || name.contains("free") || name.contains("personal team")
}

fn team_kind_label(team: &isideload::dev::teams::DeveloperTeam) -> String {
    if team_is_free(team) { "free" } else { "paid" }.into()
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

/// Права файла в архиве. Без них zip пишет 0644 всем файлам, и у исполняемых
/// файлов приложения (сама программа, dylib, бинарники расширений) пропадает
/// бит запуска. На Windows прав у файлов нет вовсе, поэтому Mach-O узнаём по
/// сигнатуре в начале файла.
fn zip_options_for(path: &Path, data: &[u8], base: SimpleFileOptions) -> SimpleFileOptions {
    base.unix_permissions(if is_executable_file(path, data) { 0o755 } else { 0o644 })
}

fn is_executable_file(path: &Path, data: &[u8]) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
            return true;
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    is_macho(data)
}

fn is_macho(data: &[u8]) -> bool {
    const MAGICS: [[u8; 4]; 6] = [
        [0xfe, 0xed, 0xfa, 0xce],
        [0xfe, 0xed, 0xfa, 0xcf],
        [0xce, 0xfa, 0xed, 0xfe],
        [0xcf, 0xfa, 0xed, 0xfe],
        [0xca, 0xfe, 0xba, 0xbe],
        [0xbe, 0xba, 0xfe, 0xca],
    ];
    data.len() >= 4 && MAGICS.iter().any(|m| data[..4] == *m)
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
            zip.start_file(zip_path, zip_options_for(&path, &buf, opts)).map_err(|e| e.to_string())?;
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

#[cfg(test)]
mod signing_flow_tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "signum-signing-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_plist(path: &Path, pairs: &[(&str, &str)]) {
        let mut dict = plist::Dictionary::new();
        for (k, v) in pairs {
            dict.insert((*k).into(), plist::Value::String((*v).into()));
        }
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        plist::Value::Dictionary(dict).to_file_binary(path).unwrap();
    }

    fn read_id(path: &Path, key: &str) -> String {
        plist::Value::from_file(path)
            .unwrap()
            .as_dictionary()
            .unwrap()
            .get(key)
            .unwrap()
            .as_string()
            .unwrap()
            .to_string()
    }

    #[test]
    fn rebase_only_touches_children_of_the_old_id() {
        assert_eq!(rebase_id("com.a.app", "com.a.app", "x.y"), Some("x.y".into()));
        assert_eq!(
            rebase_id("com.a.app.widget", "com.a.app", "x.y"),
            Some("x.y.widget".into())
        );
        assert_eq!(rebase_id("com.a.application", "com.a.app", "x.y"), None);
        assert_eq!(rebase_id("org.other", "com.a.app", "x.y"), None);
    }

    #[test]
    fn changing_the_bundle_id_moves_extensions_along() {
        let dir = temp_dir("rebase");
        let app = dir.join("Payload/Bank.app");
        write_plist(&app.join("Info.plist"), &[("CFBundleIdentifier", "ru.bank")]);
        let ext = app.join("PlugIns/Widget.appex/Info.plist");
        write_plist(&ext, &[("CFBundleIdentifier", "ru.bank.widget")]);
        let watch = app.join("Watch/W.app/Info.plist");
        write_plist(
            &watch,
            &[
                ("CFBundleIdentifier", "ru.bank.watchkitapp"),
                ("WKCompanionAppBundleIdentifier", "ru.bank"),
            ],
        );
        let framework = app.join("Frameworks/Lib.framework/Info.plist");
        write_plist(&framework, &[("CFBundleIdentifier", "ru.bank.lib")]);

        set_bundle_id(&app, "ru.bank", "ru.bank.u1").unwrap();

        assert_eq!(read_id(&app.join("Info.plist"), "CFBundleIdentifier"), "ru.bank.u1");
        assert_eq!(read_id(&ext, "CFBundleIdentifier"), "ru.bank.u1.widget");
        assert_eq!(read_id(&watch, "CFBundleIdentifier"), "ru.bank.u1.watchkitapp");
        assert_eq!(read_id(&watch, "WKCompanionAppBundleIdentifier"), "ru.bank.u1");
        // Фреймворки не обязаны быть «детьми» приложения — их не трогаем.
        assert_eq!(read_id(&framework, "CFBundleIdentifier"), "ru.bank.lib");
        assert_eq!(nested_bundle_ids(&app), vec!["ru.bank.u1.widget".to_string()]);

        strip_unsignable_nested(&app);
        assert!(!app.join("Watch").exists());
        assert!(app.join("PlugIns").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wildcard_entitlements_get_the_concrete_bundle_id() {
        let mut base = plist::Dictionary::new();
        base.insert(
            "application-identifier".into(),
            plist::Value::String("TEAM123456.*".into()),
        );
        base.insert("get-task-allow".into(), plist::Value::Boolean(true));
        let ent = entitlements_for_bundle(&base, "TEAM123456", "ru.bank.widget");
        assert_eq!(
            ent.get("application-identifier").unwrap().as_string(),
            Some("TEAM123456.ru.bank.widget")
        );
        assert_eq!(ent.get("get-task-allow").unwrap().as_boolean(), Some(true));

        let mut explicit = plist::Dictionary::new();
        explicit.insert(
            "application-identifier".into(),
            plist::Value::String("TEAM123456.com.foo".into()),
        );
        let ent = entitlements_for_bundle(&explicit, "TEAM123456", "com.foo");
        assert_eq!(
            ent.get("application-identifier").unwrap().as_string(),
            Some("TEAM123456.com.foo")
        );
    }

    #[test]
    fn executables_keep_the_exec_bit_in_the_archive() {
        let dir = temp_dir("perm");
        let file = dir.join("data.txt");
        fs::write(&file, b"hello").unwrap();
        assert!(is_macho(&[0xcf, 0xfa, 0xed, 0xfe, 0x0c]));
        assert!(is_macho(&[0xca, 0xfe, 0xba, 0xbe]));
        assert!(!is_macho(b"hello"));
        assert!(is_executable_file(&file, &[0xcf, 0xfa, 0xed, 0xfe]));
        #[cfg(unix)]
        assert!(!is_executable_file(&file, b"hello"));

        // Упаковка .app: у Mach-O в архиве 0755, у ресурсов 0644.
        let app = dir.join("A.app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("A"), [0xcf, 0xfa, 0xed, 0xfe, 0, 0]).unwrap();
        fs::write(app.join("Info.plist"), b"<plist/>").unwrap();
        let ipa = dir.join("out.ipa");
        pack_app_dir_to_ipa(&app, &ipa).unwrap();
        let mut zip = ZipArchive::new(fs::File::open(&ipa).unwrap()).unwrap();
        let exe_mode = zip.by_name("Payload/A.app/A").unwrap().unix_mode().unwrap();
        let res_mode = zip
            .by_name("Payload/A.app/Info.plist")
            .unwrap()
            .unix_mode()
            .unwrap();
        assert_eq!(exe_mode & 0o777, 0o755);
        assert_eq!(res_mode & 0o777, 0o644);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_are_classified_for_the_user() {
        // Обрыв сети внутри выпуска сертификата — это сеть, а не сертификат.
        assert!(map_isideload_err(
            "Failed to retrieve certificate identity: error sending request for url (https://developerservices2.apple.com)"
        )
        .starts_with("SIGN_IPA_NETWORK"));
        // Подменённый TLS — не «сертификат разработки».
        assert!(map_isideload_err("invalid peer certificate: UnknownIssuer")
            .contains("антивирус"));
        assert!(map_isideload_err(
            "Not enough available app IDs. 3 are required, but only 1 is available."
        )
        .starts_with("SIGN_IPA_APP_ID_LIMIT"));
        assert!(map_isideload_err("Developer error 7460: Maximum number of certificates reached")
            .starts_with("SIGN_IPA_CERT_FAILED"));
        assert!(map_isideload_err("No 2FA code provided, aborting")
            .contains("2FA"));
        assert!(map_isideload_err(
            "Extension Widget is not part of the main app bundle identifier: x"
        )
        .starts_with("SIGN_IPA_BUNDLE_INVALID"));
        // «team» в произвольном тексте больше не означает «нет команды».
        assert!(!map_isideload_err("Failed to list app IDs for the developer team: no response")
            .starts_with("SIGN_IPA_TEAM_NONE"));
        assert_eq!(map_isideload_err("No developer teams available"), "SIGN_IPA_TEAM_NONE");
    }

    #[test]
    fn personal_team_counts_as_free() {
        let team = |name: &str, ty: Option<&str>| isideload::dev::teams::DeveloperTeam {
            name: Some(name.into()),
            team_id: name.into(),
            r#type: ty.map(str::to_string),
            status: None,
        };
        let teams = vec![
            team("Ivan Petrov (Personal Team)", Some("Individual")),
            team("Company LLC", Some("Company/Organization")),
        ];
        assert_eq!(team_kind_label(&teams[0]), "free");
        assert_eq!(team_kind_label(&teams[1]), "paid");
        assert_eq!(pick_team(&teams, CertMode::Free).unwrap().team_id, teams[0].team_id);
        assert_eq!(pick_team(&teams, CertMode::Paid).unwrap().team_id, teams[1].team_id);
        assert_eq!(pick_team(&teams, CertMode::Auto).unwrap().team_id, teams[1].team_id);
    }

    #[test]
    fn two_factor_code_is_six_digits() {
        assert_eq!(only_code_digits(" 123-456 "), "123456");
        assert_eq!(only_code_digits("12345"), "12345");
        assert_eq!(only_code_digits("1234567"), "123456");
    }
}

#[cfg(test)]
mod custom_cert_tests {
    use super::*;

    fn make_p12(password: &str) -> (Vec<u8>, Vec<u8>) {
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["signum.test".into()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let cert_der = cert.der().to_vec();
        let chain = p12_keystore::PrivateKeyChain::new(
            vec![1, 2, 3, 4],
            p12_keystore::PrivateKey::from_der(&key.serialize_der()).unwrap(),
            vec![p12_keystore::Certificate::from_der(&cert_der).unwrap()],
        );
        let mut store = p12_keystore::KeyStore::new();
        store.add_entry("test", p12_keystore::KeyStoreEntry::PrivateKeyChain(chain));
        (store.writer(password).write().unwrap(), cert_der)
    }

    #[test]
    fn p12_password_and_certificate_are_checked_natively() {
        let dir = std::env::temp_dir().join(format!("signum-p12-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cert.p12");
        let (p12, cert_der) = make_p12("секрет 1");
        fs::write(&path, p12).unwrap();

        match read_p12(&path, "секрет 1") {
            P12Check::Ok(identity) => {
                assert_eq!(identity.cert_der, cert_der);
                assert!(!identity.key_der.is_empty());
                let not_after =
                    crate::signing_profile::certificate_not_after(&identity.cert_der).unwrap();
                assert!(not_after > crate::signing_profile::unix_now());
            }
            _ => panic!("valid p12 not read"),
        }
        assert!(matches!(read_p12(&path, "wrong"), P12Check::WrongPassword));
        // Пробелы по краям — часть пароля, обрезать их нельзя.
        assert!(matches!(read_p12(&path, "секрет 1 "), P12Check::WrongPassword));

        let garbage = dir.join("not.p12");
        fs::write(&garbage, b"-----BEGIN CERTIFICATE-----").unwrap();
        assert!(matches!(read_p12(&garbage, ""), P12Check::Unreadable(_)));
        let _ = fs::remove_dir_all(&dir);
    }
}
