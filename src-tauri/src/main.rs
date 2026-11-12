#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apple_history;
mod apple_region;
mod apple_signing;
mod apple_store;
mod app_transfer;
mod bank_creds_page;
mod bank_otp;
mod diag_log;
mod history_secrets;
mod ipa_inspect;
mod store_search;
mod uninstall;

use futures_util::{stream, StreamExt};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex as StdMutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    path::BaseDirectory,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_notification::NotificationExt;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::{Mutex, Notify},
};
use uuid::Uuid;

pub(crate) const APP_VERSION: &str = "1.2.2";
const APP_BUILD: &str = "42";
const DEFAULT_API: &str = "https://api.appbank.pw/signum";
const FALLBACK_API: Option<&str> = Some("https://apiz.appbank.pw/signum");
const HAPP_APP_STORE_ID: &str = "6504287215";
// No TLS pinning: it broke the whole client when the leaf certificate rotated
// or a corporate/VPN proxy presented its own chain (catalog and tools.php went
// dark for the user). Plain WebPKI via rustls is enough — the API content is
// signed/verified where it matters (managed tool updates use SHA-256).
const SMART_UPDATE_INTERVAL_SECS: u64 = 600;
const SMART_UPDATE_INITIAL_DELAY_SECS: u64 = 15;
/// Soft refresh of purchase-history RAP cookies so users rarely re-enter 2FA.
/// Touch RAP cookies often enough that Apple rarely expires myacinfo mid-day.
const HISTORY_SESSION_KEEPALIVE_SECS: u64 = 8 * 60;
const HISTORY_SESSION_KEEPALIVE_INITIAL_SECS: u64 = 45;
// Every bundled engine shares one cookie jar; parallel writers corrupt it.
const ACCOUNT_VERSION_PROBE_CONCURRENCY: usize = 1;
/// Сколько ждать ответа на запрос списка версий.
///
/// Было 25 секунд — и этого перестало хватать, когда последней ступени
/// разрешили порождать подтверждение FairPlay. Холодный путь измеряется так:
/// около 9 секунд на само порождение плюс запрос к Apple, в сумме 20–22 на
/// свободной машине. Двадцатипятисекундный предел срезал это на любой заминке,
/// и в логах у людей видно, как запрос убивали ровно на 25-й секунде —
/// а человек видел «не удалось получить список версий» на ровном месте.
const ACCOUNT_VERSION_PROBE_TIMEOUT_SECS: u64 = 75;
/// "Pick another version" must not download metadata for every historical
/// build (VK/Mail.ru can have 80–200). Newest external IDs first.
const ACCOUNT_VERSION_PICKER_LIMIT: usize = 12;
static DETAILED_ERRORS_ENABLED: AtomicBool = AtomicBool::new(false);
const DEMO_MODE: bool = false;
const DEMO_ACCOUNT_EMAIL: &str = "demo@signum.app";
const DEMO_HAPP_BUNDLE_ID: &str = "cc.happ.happ";
static DEMO_LOGGED_OUT: AtomicBool = AtomicBool::new(false);
static DEMO_ACTIVE_EMAIL: StdMutex<Option<String>> = StdMutex::new(None);
static DEMO_STORE_REGION: StdMutex<Option<(String, String)>> = StdMutex::new(None);

/// Legacy hardcoded key — never use for new installs. Migrated away on load.
const LEGACY_BUILTIN_IPATOOL_KEYCHAIN_PASSPHRASE: &str = "signum-ipatool-keychain-v1";
/// Primary Apple iCloud for Windows bootstrapper (required for ipatool v3 / anisette).
const ICLOUD_SETUP_URL_PRIMARY: &str =
    "https://updates.cdn-apple.com/2020/windows/001-39935-20200911-1A70AA56-F448-11EA-8CC0-99D41950005E/iCloudSetup.exe";
/// Primary Apple iTunes 64-bit Windows installer.
const ITUNES_SETUP_URL_PRIMARY: &str = "https://www.apple.com/itunes/download/win64";
/// iTunes 12.10.11 — последний, что ставится на Windows 7/8/8.1; новые просят 10+.
#[cfg(target_os = "windows")]
const ITUNES_LEGACY_SETUP_URL: &str = "https://secure-appldnld.apple.com/itunes12/001-80053-20210422-E8A3B28C-A3B2-11EB-BE07-CE1B67FC6302/iTunes64Setup.exe";
#[cfg(target_os = "windows")]
const ITUNES_LEGACY_MIRROR_NAME: &str = "iTunes64Setup-12.10.11.exe";
/// Apple Mobile Device Support 20.0.0.35 из iTunes 12.13.11 (подпись Apple).
/// Это всё, что Signum нужно от iTunes: служба USB для iPhone, 34 МБ вместо
/// 193. Сам USB-драйвер не входит ни сюда, ни в iTunes — его ставит Windows
/// Update. Работает с Windows 10; на 7/8/8.1 ставим iTunes 12.10.11 целиком.
#[cfg(target_os = "windows")]
const AMDS_MIRROR_NAME: &str = "AppleMobileDeviceSupport64.msi";
/// iCloud для Windows Signum больше не нужен (04.10.2026): ipatool 1.1.1 несёт
/// подпись SAP и kbsync в себе, подпись IPA берёт anisette с удалённого сервера.
/// Оставался только anisette.exe нашего клиента App Store — без него подписи
/// версий спрашиваются у ipatool. А сам iCloud у Apple теперь только в
/// Microsoft Store, и Signum удалял его оттуда ради версии 2020 года.
/// Код установки оставлен за этим флагом.
#[cfg(target_os = "windows")]
const SIGNUM_NEEDS_ICLOUD: bool = false;
/// Mirror filenames under the Signum API's files/ directory.
const ICLOUD_SETUP_MIRROR_NAME: &str = "iCloudSetup.exe";
const ITUNES_SETUP_MIRROR_NAME: &str = "iTunes64Setup.exe";
const MFP_X64_MIRROR_NAME: &str = "mfp-x64.msu";
const MFP_X32_MIRROR_NAME: &str = "mfp-x32.msu";
/// Распространяемый пакет Visual C++ 2015–2022.
///
/// От него зависит деинсталлятор: uninstall.exe тянет vcruntime140.dll. На
/// машине без пакета он не запускается вовсе, и удалить Signum штатно нельзя —
/// человек видит окно «не удалось запустить приложение» без объяснений.
/// Само приложение и инструменты Apple пакет не используют: проверено разбором
/// таблиц импорта всех 33 вложенных бинарей.
const VC_REDIST_MIRROR_NAME: &str = "vc_redist.x64.exe";
const VC_REDIST_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";
/// Microsoft Store product page for modern iCloud for Windows.
const ICLOUD_MS_STORE_URL: &str = "ms-windows-store://pdp/?ProductId=9PKTQ5699M62";
const ICLOUD_APPLE_SUPPORT_URL: &str = "https://support.apple.com/ru-ru/HT204283";
/// Media Feature Pack help (manual fallback button).
const MEDIA_FEATURE_PACK_URL: &str =
    "https://support.microsoft.com/ru-ru/topic/media-feature-pack-list-for-windows-n-editions-c1c6fffa-d052-8338-7a79-a4bb980a700a";
static ICLOUD_ENSURE_RUNNING: AtomicBool = AtomicBool::new(false);
/// Process-wide cache so login + auth info never diverge mid-session.
static CACHED_IPATOOL_PASSPHRASE: StdMutex<Option<String>> = StdMutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WindowState {
    width: u32,
    height: u32,
    #[serde(default)]
    maximized: bool,
}

/// Адрес прокси из текущих настроек.
///
/// Настройки читаются с диска: сюда попадают и фоновые задачи, у которых нет
/// под рукой состояния приложения. Файл маленький, а промах по прокси стоит
/// дороже одного чтения.
fn current_proxy_url() -> Option<String> {
    proxy_cache().lock().ok().and_then(|value| value.clone())
}

fn proxy_cache() -> &'static StdMutex<Option<String>> {
    static CACHE: OnceLock<StdMutex<Option<String>>> = OnceLock::new();
    CACHE.get_or_init(|| StdMutex::new(None))
}

/// Запоминает адрес прокси для тех мест, где настроек под рукой нет —
/// дочерние процессы и фоновые задачи.
fn remember_proxy(settings: &AppSettings) {
    let url = proxy_url(settings);
    if let Ok(mut cache) = proxy_cache().lock() {
        *cache = url.clone();
    }
    // Свои HTTP-клиенты строятся один раз за запуск и читают прокси из
    // окружения, поэтому переменные ставим и себе. Уже созданные клиенты
    // переключатся только после перезапуска — об этом сказано в настройках.
    for key in ["ALL_PROXY", "HTTPS_PROXY", "HTTP_PROXY"] {
        match url.as_deref() {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

fn default_proxy_kind() -> String {
    "socks5".into()
}

/// Адрес прокси в виде, понятном и reqwest, и libcurl.
///
/// libcurl (движок v5) читает его из ALL_PROXY, поэтому одна строка покрывает
/// и наши запросы, и дочерние процессы — отдельной настройки для них не нужно.
fn proxy_url(settings: &AppSettings) -> Option<String> {
    if !settings.proxy_enabled {
        return None;
    }
    let host = settings.proxy_host.trim();
    if host.is_empty() || settings.proxy_port == 0 {
        return None;
    }
    // Схемы с «h» и «a» на конце просят прокси разрешать имена самому. Без
    // них имя узла разрешается на этой машине — а ради обхода именно такой
    // блокировки прокси обычно и включают, так что socks5:// молча ломал ровно
    // тот случай, ради которого настройку и добавляли.
    let kind = match settings.proxy_kind.trim().to_ascii_lowercase().as_str() {
        "socks4" => "socks4a",
        "http" => "http",
        "https" => "https",
        _ => "socks5h",
    };
    let user = settings.proxy_user.trim();
    let auth = if user.is_empty() {
        String::new()
    } else {
        // Пароль может содержать что угодно, поэтому кодируем обе части.
        format!(
            "{}:{}@",
            urlencoding_component(user),
            urlencoding_component(settings.proxy_password.trim())
        )
    };
    Some(format!("{kind}://{auth}{host}:{}", settings.proxy_port))
}

fn urlencoding_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn default_true() -> bool {
    true
}

// Default нужен тестам, чтобы задавать только значимые поля. На поведение не
// влияет: недостающие поля файла настроек и так закрывает serde.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppSettings {
    /// Работа в фоне: автозапуск при входе в систему и поддержание кеша
    /// FairPlay горячим.
    ///
    /// Включено для новых настроек; сохранённое отключение сохраняется.
    /// Генерация выполняется с низким приоритетом:
    /// после снятия сторожевого хука в эмуляторе обновление подтверждения стоит
    /// около девяти секунд раз в двадцать пять минут.
    #[serde(default)]
    background_enabled: bool,
    /// Прокси для всех обращений к Apple: и наших, и дочерних процессов.
    ///
    /// Нужен там, где Apple недоступна напрямую. Храним разобранным по полям, а
    /// не строкой — так человеку не нужно знать синтаксис, а нам не нужно
    /// разбирать то, что он набрал.
    #[serde(default)]
    proxy_enabled: bool,
    /// socks5 | socks4 | http | https
    #[serde(default = "default_proxy_kind")]
    proxy_kind: String,
    #[serde(default)]
    proxy_host: String,
    #[serde(default)]
    proxy_port: u16,
    #[serde(default)]
    proxy_user: String,
    #[serde(default)]
    proxy_password: String,
    downloads_dir: String,
    #[serde(default)]
    exports_dir: String,
    theme: String,
    auto_update: bool,
    /// Auto-buy bank/service App Store apps onto Apple ID (no iPhone required). Default off.
    #[serde(default)]
    auto_purchase_new_apps: bool,
    #[serde(default)]
    lite_design: bool,
    #[serde(default)]
    detailed_errors: bool,
    /// Отправлять журнал работы после закрытия приложения.
    ///
    /// По умолчанию включено: без журналов ошибки у людей приходится разбирать
    /// по описанию, а половина сегодняшних находок — устаревший движок на маке,
    /// двойной запрос на вход, истёкшая сессия — видна только в журнале.
    /// Личное из него вырезается до отправки: почта, имя, пароли, коды,
    /// токены, куки и имя пользователя в путях.
    #[serde(default = "default_true")]
    auto_diag: bool,
    #[serde(default = "default_true")]
    lite_static_loaders: bool,
    #[serde(default = "default_true")]
    lite_reduce_motion: bool,
    #[serde(default = "default_true")]
    lite_simple_scrolling: bool,
    #[serde(default = "default_true")]
    lite_disable_blur: bool,
    #[serde(default = "default_true")]
    lite_disable_shadows: bool,
    #[serde(default = "default_true")]
    lite_flat_backgrounds: bool,
    #[serde(default = "default_ipatool_keychain_mode")]
    ipatool_keychain_mode: String,
    #[serde(default)]
    ipatool_keychain_passphrase: Option<String>,
    #[serde(default)]
    selected_device_udid: Option<String>,
    /// How to reach selected device: "usb" | "wifi". Prefer usb when both work.
    #[serde(default)]
    selected_device_connection: Option<String>,
    /// Keep purchase-history RAP session in background (no re-2FA). Default on.
    #[serde(default = "default_true")]
    keep_history_session: bool,
    /// Which ipatool build to use: auto, v5, v4 or v6.
    #[serde(default = "default_ipatool_engine")]
    ipatool_engine: String,
    /// Catalog ids the user pinned to the top of the catalog.
    #[serde(default)]
    favorite_app_ids: Vec<String>,
    /// Re-sign and reinstall a signed app on its own, shortly before the
    /// profile expires. Off unless the user turns it on: it wakes the iPhone
    /// and talks to Apple without being asked.
    #[serde(default)]
    auto_resign_before_expiry: bool,
}

/// Последняя успешно определённая страна App Store для конкретного Apple ID.
/// Хранится в `cache.json`, а не в настройках, поскольку запись привязана к
/// аккаунту и не должна перезаписываться формой настроек.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreRegionCacheEntry {
    email: String,
    country_code: String,
    country_name: String,
    timestamp: u64,
}

#[derive(Debug, Clone)]
enum IpatoolKeychain {
    Passphrase(String),
}

fn default_ipatool_keychain_mode() -> String {
    "random".into()
}

fn default_ipatool_engine() -> String {
    "auto".into()
}

#[tauri::command]
fn get_ipatool_engine_status(app: AppHandle) -> Result<IpatoolEngineStatus, String> {
    let settings = load_settings(&app)?;
    Ok(IpatoolEngineStatus {
        selected: normalize_ipatool_engine(&settings.ipatool_engine),
        active: active_ipatool_engine(&app),
    })
}

#[derive(Clone)]
struct ToolPaths {
    ipatool: String,
    anisette: Option<String>,
    ideviceinfo: String,
    ideviceinstaller: String,
    idevicepair: String,
    /// Optional USB enumerator shipped by recent IPA_Downloader bundles.
    /// Older Signum installs keep working through idevicepair/default probing.
    idevice_id: Option<String>,
    zsign: Option<String>,
}

const TOOLS_MANIFEST_SCHEMA: u64 = 1;
const MAX_MANAGED_TOOL_BYTES: u64 = 128 * 1024 * 1024;
const MANAGED_TOOL_NAMES: &[&str] =
    &["ipatool-v2", "ipatool-v3", "ipatool-v4", "ipatool-v5", "sapsigner", "anisette"];

#[derive(Debug, Deserialize)]
struct ToolManifestResponse {
    ok: bool,
    schema_version: u64,
    target: String,
    #[serde(default)]
    tools: HashMap<String, ToolRelease>,
}

#[derive(Debug, Deserialize)]
struct ToolRelease {
    version: String,
    path: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManagedToolState {
    version: String,
    sha256: String,
    file: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct ToolUpdateReport {
    checked: bool,
    updated: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceInfo {
    udid: String,
    product_type: Option<String>,
    product_version: Option<String>,
    device_name: Option<String>,
    battery_current_capacity: Option<u8>,
    /// "usb" | "wifi" | "both" — how the device is reachable right now.
    #[serde(default = "default_device_connection_usb")]
    connection: String,
}

fn default_device_connection_usb() -> String {
    "usb".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledApp {
    bundle_id: String,
    name: Option<String>,
    version: Option<String>,
    #[serde(default)]
    app_store_id: Option<String>,
    /// True for an iOS offloaded app placeholder (the app container is gone,
    /// but its App Store record remains on the device).
    #[serde(default)]
    is_placeholder: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CachedIpa {
    filename: String,
    path: String,
    bundle_id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    size: u64,
    modified_at: u64,
    /// Apple ID that downloaded this IPA (from iTunesMetadata.plist). Shown as a
    /// label and used to avoid reusing another account's licensed copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    apple_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_compatible: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

fn default_job_kind() -> String {
    "install".into()
}

#[derive(Debug, Clone)]
enum RetryPayload {
    Catalog(CatalogInstallInput),
    Export(CatalogInstallInput),
    Local(PathBuf),
    OpenExportsFolder,
    SignAndInstall(apple_signing::SignAndInstallInput),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallJob {
    id: String,
    title: String,
    subtitle: String,
    phase: String,
    percent: f64,
    message: String,
    status: JobStatus,
    error: Option<String>,
    created_at: u64,
    #[serde(default = "default_job_kind")]
    kind: String,
    #[serde(default)]
    retryable: bool,
    #[serde(skip)]
    retry_payload: Option<RetryPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppleAccount {
    email: String,
    added_at: u64,
}

#[derive(Debug, Clone, Serialize)]
struct OtaUpdateInfo {
    has_update: bool,
    version: Option<String>,
    build: Option<String>,
    download_url: Option<String>,
    notes: Option<String>,
    mandatory: bool,
    #[serde(default)]
    platform: Option<String>,
    #[serde(default)]
    release_available: bool,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    min_client_version: Option<String>,
    /// false when this desktop build is below API min_client_version.
    #[serde(default = "default_true")]
    client_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ClientOutdatedEvent {
    message: String,
    min_client_version: Option<String>,
    latest_version: Option<String>,
    download_url: Option<String>,
    sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AppVersionInfo {
    version: &'static str,
    build: &'static str,
    beta: bool,
}

#[derive(Debug, Clone, Serialize)]
struct IpatoolEngineStatus {
    selected: String,
    active: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AppUpdateFoundEvent {
    app_id: String,
    title: String,
    bundle_id: String,
    installed_version: String,
    remote_version: String,
    in_store: bool,
    app_store_id: Option<String>,
    min_ios: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProgressEvent {
    job_id: String,
    phase: String,
    percent: f64,
    message: String,
    status: JobStatus,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginInput {
    email: String,
    #[serde(default)]
    password: String,
    /// 6-digit Apple 2FA. Primary: snake_case (UI). Alias: camelCase.
    /// Do NOT send both keys from the frontend — serde rejects duplicates.
    #[serde(default, alias = "twoFactorCode")]
    two_factor_code: Option<String>,
    #[serde(default, alias = "resendCode")]
    resend_code: Option<bool>,
}

/// Result of Apple ID login: ipatool session + automatic history auth setup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginAppleResult {
    email: String,
    /// ipatool asked for 2FA — resubmit with two_factor_code.
    #[serde(default)]
    needs_2fa: bool,
    /// History (reportaproblem) asked for 2FA after successful ipatool login.
    #[serde(default)]
    history_needs_2fa: bool,
    /// RAP session cookies saved — history can load without re-auth.
    #[serde(default)]
    history_ready: bool,
    /// History setup failed (ipatool login itself succeeded).
    #[serde(skip_serializing_if = "Option::is_none")]
    history_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    /// Raw catalog/session failure (ipatool/SAP/anisette) for the Account UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogInstallInput {
    title: String,
    bundle_id: String,
    app_store_id: Option<String>,
    in_store: bool,
    min_ios: Option<String>,
    version: Option<String>,
    /// Explicit App Store build selected from `list-versions`.
    #[serde(default)]
    external_version_id: Option<String>,
    /// Known on this Apple ID (purchase history / restore). Skip hard-fail on
    /// store «app not found» and prefer `download -i … --purchase`.
    #[serde(default)]
    already_owned: bool,
    /// Happ is US-only: temporarily switch an unowned account to USA, purchase,
    /// install, then restore the exact country/address captured before the switch.
    #[serde(default)]
    temporary_us_region: bool,
    /// Same flow for any app: temporarily switch to another country (iso3),
    /// purchase, install, restore. Set by the UI when the app is unavailable in
    /// the account's current region but exists in another storefront.
    #[serde(default)]
    temporary_region: Option<String>,
}

#[derive(Debug)]
enum QueuedInstall {
    Catalog {
        job_id: String,
        input: CatalogInstallInput,
    },
    Export {
        job_id: String,
        input: CatalogInstallInput,
    },
    Local {
        job_id: String,
        path: PathBuf,
    },
    OpenExportsFolder {
        job_id: String,
    },
    SignAndInstall {
        job_id: String,
        input: apple_signing::SignAndInstallInput,
    },
    /// Install Sovcombank/Rocketbank from bank .plist manifest (IPA URL inside).
    BankPlistInstall {
        job_id: String,
        bank: bank_otp::BankKey,
        link: String,
        title: String,
    },
}

impl QueuedInstall {
    fn id(&self) -> &str {
        match self {
            Self::Catalog { job_id, .. } | Self::Export { job_id, .. }
            | Self::Local { job_id, .. } | Self::OpenExportsFolder { job_id }
            | Self::SignAndInstall { job_id, .. } | Self::BankPlistInstall { job_id, .. } => job_id,
        }
    }
}

#[derive(Default)]
struct InstallQueue {
    pending: Mutex<VecDeque<QueuedInstall>>,
    paused: AtomicBool,
    current: Mutex<Option<String>>,
    changed: Notify,
}

impl InstallQueue {
    async fn send(&self, item: QueuedInstall) -> Result<(), String> {
        self.pending.lock().await.push_back(item);
        self.changed.notify_one();
        Ok(())
    }

    async fn next(&self) -> QueuedInstall {
        loop {
            let changed = self.changed.notified();
            let mut pending = self.pending.lock().await;
            if !self.paused.load(Ordering::Relaxed) {
                if let Some(item) = pending.pop_front() {
                    *self.current.lock().await = Some(item.id().to_string());
                    return item;
                }
            }
            drop(pending);
            changed.await;
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct VersionPickerList {
    ids: Vec<String>,
    latest_id: Option<String>,
    latest_display: Option<String>,
    /// Apple owns up to the licence but will not serve the download: the app is
    /// not in this Apple ID's storefront. The history cannot be listed, yet
    /// `download --purchase` still returns the latest build.
    region_blocked: bool,
}

struct AppState {
    account_cache_owner: StdMutex<Option<String>>,
    version_page_cache: StdMutex<HashMap<String, AppStoreVersionsPage>>,
    jobs: Arc<Mutex<HashMap<String, InstallJob>>>,
    queue: Arc<InstallQueue>,
    ipatool_lock: Arc<Mutex<()>>,
    cancel_flags: Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>,
    background_mode: Arc<AtomicBool>,
    // Cache of account-restore candidate versions per catalog app_id (keyed by app_id).
    // Cleared on logout/switch to avoid stale data for different Apple ID.
    account_version_cache: Arc<StdMutex<HashMap<String, Vec<JsonValue>>>>,
    // Cache of list-versions external IDs per adamId/bundle (not a single bool —
    // different catalog rows may require different external_version_id).
    // Some(vec) = parsed result (empty = not owned); missing key = not probed yet.
    version_probe_cache: Arc<StdMutex<HashMap<String, Vec<String>>>>,
    /// Ordered App Store version IDs for the picker (newest first). Load-more
    /// reuses this so list-versions is not repeated.
    version_picker_cache: Arc<StdMutex<HashMap<String, VersionPickerList>>>,
    /// idmsa session waiting for purchase-history 2FA (bound to first password step).
    history_2fa_pending: Arc<StdMutex<Option<apple_history::PendingHistoryAuth>>>,
    /// Same idmsa freeze when смена страны просит код.
    region_2fa_pending: Arc<StdMutex<Option<apple_history::PendingHistoryAuth>>>,
    /// Reply channel for isideload GSA 2FA during IPA signing (blocks until UI submits code).
    signing_2fa_reply: Arc<StdMutex<Option<std::sync::mpsc::Sender<String>>>>,
    login_2fa_engine: Arc<StdMutex<HashMap<String, String>>>,
    /// Pending bank OTP sessions (phone → SMS → credentials).
    bank_otp: Arc<bank_otp::BankOtpStore>,
    /// Serializes downloads and atomic replacements of managed helper tools.
    tool_update_lock: Arc<Mutex<()>>,
}

fn register_cancel_flag(state: &AppState, job_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    state
        .cancel_flags
        .lock()
        .unwrap()
        .insert(job_id.to_string(), flag.clone());
    flag
}

fn is_job_cancelled(state: &AppState, job_id: &str) -> bool {
    state
        .cancel_flags
        .lock()
        .unwrap()
        .get(job_id)
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
}

fn ensure_not_cancelled(state: &AppState, job_id: &str) -> Result<(), String> {
    if is_job_cancelled(state, job_id) {
        Err("Установка отменена".into())
    } else {
        Ok(())
    }
}

fn clear_cancel_flag(state: &AppState, job_id: &str) {
    state.cancel_flags.lock().unwrap().remove(job_id);
}

fn get_cached_account_versions(state: &AppState, app_id: &str) -> Option<Vec<JsonValue>> {
    state
        .account_version_cache
        .lock()
        .unwrap()
        .get(app_id)
        .cloned()
}

fn set_cached_account_versions(state: &AppState, app_id: &str, versions: Vec<JsonValue>) {
    state
        .account_version_cache
        .lock()
        .unwrap()
        .insert(app_id.to_string(), versions);
}

fn clear_account_version_cache(state: &AppState) {
    *state.account_cache_owner.lock().unwrap() = None;
    state.version_page_cache.lock().unwrap().clear();
    state.account_version_cache.lock().unwrap().clear();
    state.version_probe_cache.lock().unwrap().clear();
    state.version_picker_cache.lock().unwrap().clear();
}

#[derive(Default, Serialize, Deserialize)]
struct AccountCatalogCache {
    email: String,
    saved_at: u64,
    owned: HashMap<String, Vec<JsonValue>>,
    probes: HashMap<String, Vec<String>>,
    pickers: HashMap<String, VersionPickerList>,
    #[serde(default)]
    pages: HashMap<String, AppStoreVersionsPage>,
}

fn account_catalog_cache_path(app: &AppHandle, email: &str) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let hash = hex::encode(Sha256::digest(normalize_account_email(email).as_bytes()));
    Ok(dir.join(format!("catalog-account-{hash}.json")))
}

fn account_catalog_cache_matches(cache: &AccountCatalogCache, email: &str) -> bool {
    cache.email == normalize_account_email(email)
        && cache.saved_at <= now()
        && now().saturating_sub(cache.saved_at) < 24 * 3600
}

// Called only after auth info confirms the actual session, under ipatool_lock.
fn bind_account_catalog_cache(app: &AppHandle, email: &str) {
    let state = app.state::<AppState>();
    let email = normalize_account_email(email);
    if state.account_cache_owner.lock().unwrap().as_deref() == Some(email.as_str()) {
        return;
    }
    clear_account_version_cache(&state);
    let cached = account_catalog_cache_path(app, &email).ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|raw| serde_json::from_slice::<AccountCatalogCache>(&raw).ok())
        .filter(|cache| account_catalog_cache_matches(cache, &email))
        .unwrap_or_default();
    *state.account_version_cache.lock().unwrap() = cached.owned;
    *state.version_probe_cache.lock().unwrap() = cached.probes;
    *state.version_picker_cache.lock().unwrap() = cached.pickers;
    *state.version_page_cache.lock().unwrap() = cached.pages;
    *state.account_cache_owner.lock().unwrap() = Some(email);
}

fn persist_account_catalog_cache(app: &AppHandle) {
    if DEMO_MODE { return; }
    let state = app.state::<AppState>();
    let Some(email) = state.account_cache_owner.lock().unwrap().clone() else { return; };
    if stored_auth_email(app).as_deref() != Some(email.as_str()) { return; }
    let cache = AccountCatalogCache {
        email: email.clone(),
        saved_at: now(),
        owned: state.account_version_cache.lock().unwrap().clone(),
        probes: state.version_probe_cache.lock().unwrap().clone(),
        pickers: state.version_picker_cache.lock().unwrap().clone(),
        pages: state.version_page_cache.lock().unwrap().clone(),
    };
    if let (Ok(path), Ok(raw)) = (account_catalog_cache_path(app, &email), serde_json::to_vec(&cache)) {
        // A truncated cache is ignored on read; account/session secrets are never stored here.
        if let Err(error) = fs::write(path, raw) {
            ipatool_debug_log(&format!("catalog cache save failed: {error}"));
        }
    }
}

#[derive(Clone)]
struct ProcessProgress {
    app: AppHandle,
    job_id: String,
    phase: String,
    from: f64,
    to: f64,
    last_raw: Arc<StdMutex<f64>>,
    known_total_bytes: Arc<StdMutex<Option<u64>>>,
}

impl ProcessProgress {
    fn new(app: AppHandle, job_id: String, phase: String, from: f64, to: f64) -> Self {
        Self {
            app,
            job_id,
            phase,
            from,
            to,
            last_raw: Arc::new(StdMutex::new(0.0)),
            known_total_bytes: Arc::new(StdMutex::new(None)),
        }
    }

    fn seed_raw(&self, raw: f64) {
        *self.last_raw.lock().unwrap() = raw.clamp(0.0, 100.0);
    }

    fn note_total_bytes(&self, total_bytes: u64) {
        if total_bytes == 0 {
            return;
        }
        let mut known = self.known_total_bytes.lock().unwrap();
        *known = Some(known.map_or(total_bytes, |value| value.max(total_bytes)));
    }

    async fn publish_raw(&self, raw: f64, message: &str) {
        let raw = raw.clamp(0.0, 100.0);
        let should_publish = {
            let mut last = self.last_raw.lock().unwrap();
            if raw < *last {
                false
            } else {
                *last = raw;
                true
            }
        };
        if !should_publish {
            return;
        }
        self.emit_download_progress(raw, message).await;
    }

    /// Разобранный из вывода процент.
    ///
    /// Назад не отматываем: раньше эта запись затирала last_raw вниз и тем
    /// обнуляла защиту в publish_raw — полоса прыгала между источниками.
    /// Осознанный сброс делается через seed_raw при повторной попытке.
    async fn publish_download_progress(&self, raw: f64, message: &str) {
        let raw = raw.clamp(0.0, 100.0);
        let should_publish = {
            let mut last = self.last_raw.lock().unwrap();
            if raw < *last {
                false
            } else {
                *last = raw;
                true
            }
        };
        if !should_publish {
            return;
        }
        self.emit_download_progress(raw, message).await;
    }

    async fn emit_download_progress(&self, raw: f64, message: &str) {
        let percent = self.from + (self.to - self.from) * raw / 100.0;
        update_job(
            &self.app,
            &self.job_id,
            &self.phase,
            percent,
            message,
            JobStatus::Running,
            None,
        )
        .await;
    }

    async fn publish(&self, line: &str) {
        let clean = strip_ansi(line);
        if clean.is_empty() {
            return;
        }
        if let Some((raw, message, total_bytes)) = parse_download_progress(&clean) {
            if let Some(total_bytes) = total_bytes {
                self.note_total_bytes(total_bytes);
            }
            self.publish_download_progress(raw, &message).await;
            return;
        }
        if is_noisy_process_line(&clean) {
            return;
        }
        let state = self.app.state::<AppState>();
        let percent = {
            let last = self.last_raw.lock().unwrap();
            self.from + (self.to - self.from) * (*last / 100.0)
        };
        let message = if clean.chars().count() > 180 {
            format!("{}…", clean.chars().take(180).collect::<String>())
        } else {
            clean
        };
        update_job(
            &self.app,
            &self.job_id,
            &self.phase,
            percent.max(1.0),
            &message,
            JobStatus::Running,
            None,
        )
        .await;
        let _ = state;
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку настроек: {e}"))?;
    Ok(dir.join("settings.json"))
}

fn cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("cache.json"))
}

fn window_state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("window.json"))
}

const WINDOW_MIN_WIDTH: u32 = 980;
const WINDOW_MIN_HEIGHT: u32 = 640;

fn load_window_state(app: &AppHandle) -> Option<WindowState> {
    let path = window_state_path(app).ok()?;
    if !path.exists() {
        return None;
    }
    let mut state: WindowState = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<WindowState>(strip_utf8_bom(&raw)).ok())?;
    state.width = state.width.max(WINDOW_MIN_WIDTH);
    state.height = state.height.max(WINDOW_MIN_HEIGHT);
    Some(state)
}

fn save_window_state(app: &AppHandle, state: &WindowState) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(window_state_path(app)?, raw).map_err(|e| e.to_string())
}

fn capture_window_state(window: &tauri::WebviewWindow) -> Option<WindowState> {
    let size = window.inner_size().ok()?;
    Some(WindowState {
        width: size.width.max(WINDOW_MIN_WIDTH),
        height: size.height.max(WINDOW_MIN_HEIGHT),
        maximized: window.is_maximized().unwrap_or(false),
    })
}

fn restore_window_state(window: &tauri::WebviewWindow) {
    let app = window.app_handle();
    let Some(state) = load_window_state(&app) else {
        return;
    };
    if state.maximized {
        let _ = window.maximize();
        return;
    }
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
        state.width,
        state.height,
    )));
}

fn persist_window_state(window: &tauri::WebviewWindow) {
    let app = window.app_handle();
    if let Some(state) = capture_window_state(window) {
        let _ = save_window_state(&app, &state);
    }
}

/// A signed app on the iPhone, and what it takes to sign it again.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedAppRecord {
    bundle_id: String,
    title: String,
    ipa_path: String,
    #[serde(default)]
    udid: Option<String>,
    #[serde(default)]
    cert_name: Option<String>,
    /// "free" or "paid" — a free Apple ID profile only lasts a week.
    #[serde(default)]
    team_kind: String,
    /// Human-readable expiry, when the profile states one.
    #[serde(default)]
    expires_at: Option<String>,
    /// The same moment as unix seconds, so the renewal check is arithmetic.
    #[serde(default)]
    expires_unix: Option<u64>,
    signed_at: u64,
}

/// A free Apple ID signing profile is good for a week.
const FREE_PROFILE_DAYS: u64 = 7;

/// Renew this long before the profile dies, so a failure still leaves time.
const RESIGN_LEAD_SECS: u64 = 2 * 24 * 3600;

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar (Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// "2026-09-11T12:00:00Z" -> unix seconds. Provisioning profiles are UTC.
fn parse_iso_seconds(text: &str) -> Option<u64> {
    let (date, time) = text.trim().split_once('T')?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    let mut time = time.trim_end_matches('Z').split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let second: i64 = time
        .next()
        .and_then(|value| value.split('.').next())
        .unwrap_or("0")
        .parse()
        .ok()?;
    let total = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    (total > 0).then_some(total as u64)
}

fn saved_cert_expiry(app: &AppHandle, name: &str) -> Option<String> {
    let dir = app.path().app_config_dir().ok()?;
    apple_signing::list_custom_certs(&dir)
        .into_iter()
        .find(|cert| cert.name == name)
        .and_then(|cert| cert.expires_at)
}

/// Remember a signed install so it can be renewed later. One row per bundle.
fn remember_signed_app(
    app: &AppHandle,
    input: &apple_signing::SignAndInstallInput,
    bundle_id: &str,
    title: &str,
    team_kind: &str,
) {
    let bundle_id = bundle_id.trim();
    if !input.install || bundle_id.is_empty() {
        return;
    }
    let signed_at = unix_now();
    let cert_name = input
        .cert_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let expires_at = cert_name
        .as_deref()
        .and_then(|name| saved_cert_expiry(app, name));
    let expires_unix = expires_at
        .as_deref()
        .and_then(parse_iso_seconds)
        // ponytail: the free-team week is a known Apple rule, not something the
        // signer reports back. Read it off the generated profile if that ever
        // becomes available here.
        .or_else(|| (team_kind == "free").then(|| signed_at + FREE_PROFILE_DAYS * 86_400));
    let record = SignedAppRecord {
        bundle_id: bundle_id.to_string(),
        title: title.to_string(),
        ipa_path: input.ipa_path.clone(),
        udid: input.udid.clone(),
        cert_name,
        team_kind: team_kind.to_string(),
        expires_at,
        expires_unix,
        signed_at,
    };
    let mut cache = load_cache(app);
    cache.signed_apps.retain(|item| item.bundle_id != record.bundle_id);
    cache.signed_apps.push(record);
    let _ = save_cache(app, &cache);
}

/// Signed apps whose profile is about to die, newest signature first.
#[tauri::command]
fn list_signed_apps(app: AppHandle) -> Result<Vec<SignedAppRecord>, String> {
    let mut apps = load_cache(&app).signed_apps;
    apps.sort_by_key(|item| std::cmp::Reverse(item.signed_at));
    Ok(apps)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenewSignedAppInput {
    bundle_id: String,
}

/// Sign and install the same IPA again with the same certificate.
#[tauri::command]
async fn renew_signed_app(
    app: AppHandle,
    state: State<'_, AppState>,
    input: RenewSignedAppInput,
) -> Result<String, String> {
    let bundle_id = input.bundle_id.trim().to_string();
    let record = load_cache(&app)
        .signed_apps
        .into_iter()
        .find(|item| item.bundle_id == bundle_id)
        .ok_or_else(|| "Это приложение не подписывалось через Signum".to_string())?;
    if !PathBuf::from(&record.ipa_path).is_file() {
        return Err(
            "Исходный IPA-файл больше не лежит по прежнему пути. Выберите его заново.".into(),
        );
    }
    let cert_mode = if record.cert_name.is_some() {
        apple_signing::CertMode::Custom
    } else {
        apple_signing::CertMode::Auto
    };
    enqueue_sign_and_install(
        app.clone(),
        state,
        apple_signing::SignAndInstallInput {
            ipa_path: record.ipa_path,
            udid: record.udid,
            install: true,
            save_signed: false,
            output_dir: None,
            cert_mode,
            cert_name: record.cert_name,
            custom_p12_path: None,
            custom_prov_path: None,
            custom_cert_password: None,
            options: Default::default(),
        },
    )
    .await
}

/// Re-sign whatever is about to expire. Opt-in, and silent when off.
async fn renew_expiring_signatures(app: &AppHandle) {
    if !load_settings(app)
        .map(|settings| settings.auto_resign_before_expiry)
        .unwrap_or(false)
    {
        return;
    }
    let now = unix_now();
    let due: Vec<SignedAppRecord> = load_cache(app)
        .signed_apps
        .into_iter()
        .filter(|record| {
            record
                .expires_unix
                .is_some_and(|at| at <= now + RESIGN_LEAD_SECS)
        })
        .filter(|record| PathBuf::from(&record.ipa_path).is_file())
        .collect();
    for record in due {
        let title = record.title.clone();
        let queued = renew_signed_app(
            app.clone(),
            app.state::<AppState>(),
            RenewSignedAppInput {
                bundle_id: record.bundle_id.clone(),
            },
        )
        .await;
        match queued {
            Ok(_) => show_signum_notification(
                app,
                "Signum",
                &format!("{title}: продлеваем подпись, скоро истечёт"),
            ),
            Err(error) => ipatool_debug_log(&format!(
                "auto-resign {} failed: {error}",
                record.bundle_id
            )),
        }
    }
}

fn load_cache(app: &AppHandle) -> AppCache {
    let path = match cache_path(app) {
        Ok(path) => path,
        Err(_) => return AppCache::default(),
    };
    if !path.exists() {
        return AppCache::default();
    }
    let mut cache = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(strip_utf8_bom(&raw)).ok())
        .unwrap_or_default();
    migrate_auth_accounts(&mut cache);
    cache
}

fn migrate_auth_accounts(cache: &mut AppCache) {
    if !cache.apple_accounts.is_empty() {
        return;
    }
    let Some(email) = cache
        .auth_email
        .as_ref()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    cache.apple_accounts.push(AppleAccount {
        email: email.clone(),
        added_at: now(),
    });
    if cache.active_account_email.is_none() {
        cache.active_account_email = Some(email);
    }
}

fn normalize_account_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn account_storage_key(email: &str) -> String {
    normalize_account_email(email)
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn account_session_dir(app: &AppHandle, email: &str) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("accounts").join(account_storage_key(email)))
}

fn saved_account_session_exists(app: &AppHandle, email: &str) -> bool {
    account_session_dir(app, email).is_ok_and(|dir| dir.join("account").is_file())
}

fn file_nonempty(path: &Path) -> bool {
    path.is_file()
        && fs::metadata(path)
            .map(|meta| meta.len() > 0)
            .unwrap_or(false)
}

fn first_non_ws_byte(raw: &[u8]) -> Option<u8> {
    raw.iter().copied().find(|b| !b.is_ascii_whitespace())
}

/// FileBackend stores a JWE compact token. JSON `{...}` is `auth export` output
/// and must not be fed back into Get("account").
fn ipatool_account_bytes_are_json_sidecar(raw: &[u8]) -> bool {
    matches!(first_non_ws_byte(raw), Some(b'{' | b'['))
}

fn ipatool_account_bytes_are_jwe(raw: &[u8]) -> bool {
    let trimmed: Vec<u8> = raw
        .iter()
        .copied()
        .skip_while(|b| b.is_ascii_whitespace())
        .collect();
    trimmed.starts_with(b"eyJ")
        && trimmed.iter().filter(|b| **b == b'.').count() >= 2
}

/// ipatool-cpp writes its own binary, machine-bound account file (leading
/// format-version byte) straight to `~/.ipatool/account`. The Go engines write
/// a JWE under `keyring/`, and `auth export` writes a JSON sidecar — so
/// "neither of those" identifies the cpp engine's store.
fn ipatool_account_bytes_are_cpp(raw: &[u8]) -> bool {
    !raw.is_empty()
        && !ipatool_account_bytes_are_jwe(raw)
        && !ipatool_account_bytes_are_json_sidecar(raw)
}

/// The jar ipatool-cpp writes, straight from libcurl. The Go engines store
/// JSON and rename anything else to `cookies.corrupt` on sight.
fn ipatool_cookie_bytes_are_libcurl(raw: &[u8]) -> bool {
    let head = &raw[..raw.len().min(64)];
    head.starts_with(b"# Netscape HTTP Cookie File") || head.starts_with(b"# HTTP Cookie File")
}

/// True while ipatool-cpp (engine v5) owns `~/.ipatool`. Its cookie jar is
/// libcurl's Netscape format and its account file lives at the top level;
/// the Go-shaped repairs below would throw both away.
fn cpp_engine_owns_ipatool_store() -> bool {
    let Some(dir) = ipatool_data_dir() else {
        return false;
    };
    let account = dir.join("account");
    if file_nonempty(&account)
        && fs::read(&account)
            .map(|raw| ipatool_account_bytes_are_cpp(&raw))
            .unwrap_or(false)
    {
        return true;
    }
    // Mid-login there is no account file yet: cpp writes its jar first and saves
    // the account only once Apple has accepted the 2FA code. The jar alone is
    // proof enough that cpp is the engine in play, and it is precisely what the
    // pending-2FA check reads.
    fs::read(dir.join("cookies"))
        .map(|raw| ipatool_cookie_bytes_are_libcurl(&raw))
        .unwrap_or(false)
}

/// Bytes of the live cookie jar, to put back if a probe eats it.
fn ipatool_cookie_snapshot() -> Option<(PathBuf, Vec<u8>)> {
    let path = ipatool_data_dir()?.join("cookies");
    let raw = fs::read(&path).ok()?;
    (!raw.is_empty()).then_some((path, raw))
}

fn restore_ipatool_cookie_snapshot(snapshot: Option<(PathBuf, Vec<u8>)>) {
    let Some((path, raw)) = snapshot else {
        return;
    };
    if fs::read(&path).is_ok_and(|current| current == raw) {
        return;
    }
    let _ = fs::write(&path, &raw);
}

fn ipatool_account_file_is_json_sidecar(path: &Path) -> bool {
    fs::read(path)
        .map(|raw| ipatool_account_bytes_are_json_sidecar(&raw))
        .unwrap_or(false)
}

fn ipatool_keyring_dir() -> Option<PathBuf> {
    ipatool_data_dir().map(|dir| dir.join("keyring"))
}

fn ipatool_keyring_account_path() -> Option<PathBuf> {
    ipatool_keyring_dir().map(|dir| dir.join("account"))
}

fn current_ipatool_session_exists() -> bool {
    if ipatool_keyring_account_path().is_some_and(|path| file_nonempty(&path)) {
        return true;
    }
    ipatool_data_dir().is_some_and(|dir| {
        let account = dir.join("account");
        file_nonempty(&account) && !ipatool_account_file_is_json_sidecar(&account)
    })
}

fn two_factor_required_login_error() -> String {
    #[cfg(target_os = "macos")]
    let where_to_check = "Проверьте уведомление на доверенном Apple-устройстве; если SMS включена для аккаунта — сообщения на доверенный номер.";
    #[cfg(not(target_os = "macos"))]
    let where_to_check = "Код приходит на доверенное устройство Apple или по SMS на номер аккаунта.";
    format!("SIGNUM_2FA_REQUIRED: Apple запросил код подтверждения. Введите 6 цифр. {where_to_check}")
}

async fn recover_ipatool_account_file(program: &str, keychain: &IpatoolKeychain) -> bool {
    if current_ipatool_session_exists() {
        return true;
    }
    copy_ipatool_session_from_sidecar(program);
    if current_ipatool_session_exists() {
        return true;
    }
    // ipatool-cpp 1.1.4 has no `auth export`. Never invoke it: its help output
    // was being mistaken for a broken login and delayed every recovery attempt.
    let _ = (program, keychain);
    false
}

fn copy_file_if_exists(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(src, dst).map_err(|e| e.to_string())?;
    Ok(())
}

fn backup_ipatool_session_for_account(app: &AppHandle, email: &str) -> Result<(), String> {
    let Some(ipatool_dir) = ipatool_data_dir() else {
        return Ok(());
    };
    let keyring_account = ipatool_dir.join("keyring").join("account");
    let legacy_account = ipatool_dir.join("account");
    let src_account = if file_nonempty(&keyring_account) {
        keyring_account
    } else if file_nonempty(&legacy_account) && !ipatool_account_file_is_json_sidecar(&legacy_account)
    {
        legacy_account
    } else {
        return Ok(());
    };
    let target = account_session_dir(app, email)?;
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    copy_file_if_exists(&src_account, &target.join("account"))?;
    let cookies = ipatool_dir.join("cookies");
    if cookies.is_file() && !ipatool_cookie_file_is_corrupt(&cookies) {
        copy_file_if_exists(&cookies, &target.join("cookies"))?;
    }
    Ok(())
}

fn restore_ipatool_session_for_account(app: &AppHandle, email: &str) -> Result<(), String> {
    let Some(ipatool_dir) = ipatool_data_dir() else {
        return Err("Не найдена папка сохранённого входа Apple ID".into());
    };
    let source = account_session_dir(app, email)?;
    if !source.join("account").exists() {
        return Err(format!(
            "Сессия для {} не сохранена. Войдите в этот Apple ID снова.",
            email
        ));
    }
    fs::create_dir_all(&ipatool_dir).map_err(|e| e.to_string())?;
    let src_account = source.join("account");
    let src_is_cpp = fs::read(&src_account)
        .map(|raw| ipatool_account_bytes_are_cpp(&raw))
        .unwrap_or(false);
    if ipatool_account_file_is_json_sidecar(&src_account) {
        copy_file_if_exists(&src_account, &ipatool_dir.join("account.json.bak"))?;
        let _ = fs::remove_file(ipatool_dir.join("account"));
    } else if src_is_cpp {
        // ipatool-cpp reads `~/.ipatool/account`; putting it under keyring/
        // leaves the engine logged out with the session sitting right there.
        copy_file_if_exists(&src_account, &ipatool_dir.join("account"))?;
        let _ = fs::remove_file(ipatool_dir.join("keyring").join("account"));
    } else {
        let keyring_dir = ipatool_dir.join("keyring");
        fs::create_dir_all(&keyring_dir).map_err(|e| e.to_string())?;
        copy_file_if_exists(&src_account, &keyring_dir.join("account"))?;
        if ipatool_dir.join("account").is_file()
            && ipatool_account_file_is_json_sidecar(&ipatool_dir.join("account"))
        {
            let _ = fs::remove_file(ipatool_dir.join("account"));
        }
    }
    copy_file_if_exists(&source.join("cookies"), &ipatool_dir.join("cookies"))?;
    sanitize_ipatool_session_files();
    Ok(())
}

fn delete_account_session(app: &AppHandle, email: &str) -> Result<(), String> {
    if let Ok(path) = account_catalog_cache_path(app, email) {
        let _ = fs::remove_file(path);
    }
    let dir = account_session_dir(app, email)?;
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Drop encrypted purchase-history password for this Apple ID.
    history_secrets::delete_all_history_secrets(email);
    history_secrets::delete_region_session(email);
    Ok(())
}

fn active_account_email(app: &AppHandle) -> Option<String> {
    let cache = load_cache(app);
    cache
        .active_account_email
        .or(cache.auth_email)
        .map(|value| normalize_account_email(&value))
        .filter(|value| !value.is_empty())
}

fn demo_active() -> bool {
    DEMO_MODE && !DEMO_LOGGED_OUT.load(Ordering::Relaxed)
}

fn demo_email() -> String {
    DEMO_ACTIVE_EMAIL
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_else(|| DEMO_ACCOUNT_EMAIL.to_string())
}

fn demo_set_email(email: &str) {
    let normalized = normalize_account_email(email);
    let value = if normalized.is_empty() {
        DEMO_ACCOUNT_EMAIL.to_string()
    } else {
        normalized
    };
    if let Ok(mut slot) = DEMO_ACTIVE_EMAIL.lock() {
        *slot = Some(value);
    }
}

fn demo_sign_out() {
    DEMO_LOGGED_OUT.store(true, Ordering::Relaxed);
}

fn demo_device() -> DeviceInfo {
    DeviceInfo {
        udid: "demo-iphone-0001".into(),
        product_type: Some("iPhone15,2".into()),
        product_version: Some("17.5.1".into()),
        device_name: Some("iPhone".into()),
        battery_current_capacity: Some(84),
        connection: "usb".into(),
    }
}

fn demo_region() -> (String, String) {
    DEMO_STORE_REGION
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_else(|| ("RUS".to_string(), "Россия".to_string()))
}

fn demo_set_region(code: &str, name: &str) {
    if let Ok(mut slot) = DEMO_STORE_REGION.lock() {
        *slot = Some((code.to_string(), name.to_string()));
    }
}

fn demo_auth_status() -> AuthStatus {
    let email = demo_email();
    AuthStatus {
        authenticated: true,
        email: Some(email.clone()),
        accounts: vec![AppleAccountInfo {
            email,
            active: true,
            session_saved: true,
            session_state: SessionState::Active,
            added_at: now(),
            session_error: None,
        }],
        history_password_saved: true,
        history_session_saved: true,
        catalog_error: None,
    }
}

fn demo_app(
    adam_id: &str,
    name: &str,
    artist: &str,
    bundle_id: &str,
    date: &str,
) -> PurchaseHistoryApp {
    PurchaseHistoryApp {
        adam_id: adam_id.to_string(),
        name: name.to_string(),
        artist: artist.to_string(),
        artwork: String::new(),
        price: String::new(),
        date: format!("{date}T12:00:00Z"),
        bundle_id: bundle_id.to_string(),
    }
}

fn demo_history_apps() -> Vec<PurchaseHistoryApp> {
    vec![
        demo_app("585305891", "Т-Банк", "АО «ТБанк»", "com.tinkoff.bank", "2026-07-28"),
        demo_app("1085228657", "СберБанк Онлайн", "ПАО Сбербанк", "ru.sberbankmobile", "2026-06-15"),
        demo_app("310633997", "WhatsApp Messenger", "WhatsApp Inc.", "net.whatsapp.WhatsApp", "2026-05-19"),
        demo_app("686449807", "Telegram", "Telegram FZ-LLC", "ph.telegra.Telephone", "2026-04-02"),
        demo_app("1036912922", "Госуслуги", "Минцифры России", "ru.gov.gosuslugi", "2026-02-11"),
    ]
}

fn demo_app_store_versions() -> Vec<AppStoreVersion> {
    vec![
        AppStoreVersion {
            version: "8.0.1".into(),
            external_version_id: "859124036".into(),
            is_latest: true,
            already_owned: true,
            min_ios: None,
        },
        AppStoreVersion {
            version: "7.9.4".into(),
            external_version_id: "862100394".into(),
            is_latest: false,
            already_owned: true,
            min_ios: None,
        },
    ]
}

fn cached_store_region_for_email(app: &AppHandle, email: &str) -> Option<StoreRegionCacheEntry> {
    let email = normalize_account_email(email);
    load_cache(app)
        .store_region_cache
        .into_iter()
        .find(|entry| entry.email == email)
}

fn cache_store_region(
    app: &AppHandle,
    email: &str,
    country_code: &str,
    country_name: &str,
) -> Result<(), String> {
    let email = normalize_account_email(email);
    let country_code = country_code.trim().to_ascii_uppercase();
    let country_name = country_name.trim();
    if email.is_empty() || country_code.len() != 3 || country_name.is_empty() {
        return Ok(());
    }

    let mut cache = load_cache(app);
    cache
        .store_region_cache
        .retain(|entry| entry.email != email);
    cache.store_region_cache.push(StoreRegionCacheEntry {
        email,
        country_code,
        country_name: country_name.to_string(),
        timestamp: now(),
    });
    cache
        .store_region_cache
        .sort_by(|left, right| left.email.cmp(&right.email));
    save_cache(app, &cache)
}

fn set_active_account(app: &AppHandle, email: &str) -> Result<(), String> {
    let email = normalize_account_email(email);
    let mut cache = load_cache(app);
    if !cache
        .apple_accounts
        .iter()
        .any(|account| account.email == email)
    {
        return Err("Аккаунт не найден в списке".into());
    }
    cache.active_account_email = Some(email.clone());
    cache.auth_email = Some(email.clone());
    save_cache(app, &cache)?;
    refresh_v6_identity(app, Some(&email));
    if let Ok(entry) = auth_entry() {
        let _ = entry.set_password(&email);
    }
    Ok(())
}

fn register_apple_account(app: &AppHandle, email: &str) -> Result<(), String> {
    let email = normalize_account_email(email);
    let mut cache = load_cache(app);
    if !cache
        .apple_accounts
        .iter()
        .any(|account| account.email == email)
    {
        cache.apple_accounts.push(AppleAccount {
            email: email.clone(),
            added_at: now(),
        });
        cache
            .apple_accounts
            .sort_by(|left, right| left.email.cmp(&right.email));
    }
    save_cache(app, &cache)
}

fn remove_apple_account_from_cache(app: &AppHandle, email: &str) -> Result<(), String> {
    let email = normalize_account_email(email);
    let mut cache = load_cache(app);
    cache
        .apple_accounts
        .retain(|account| account.email != email);
    cache
        .store_region_cache
        .retain(|entry| entry.email != email);
    if cache.active_account_email.as_deref() == Some(email.as_str()) {
        cache.active_account_email = None;
        cache.auth_email = None;
    }
    save_cache(app, &cache)
}

fn save_cache(app: &AppHandle, cache: &AppCache) -> Result<(), String> {
    let mut payload = cache.clone();
    payload.known_app_ids.sort();
    payload.known_app_ids.dedup();
    let raw = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(cache_path(app)?, raw).map_err(|e| e.to_string())
}

fn cache_remember_app_ids(app: &AppHandle, ids: HashSet<String>) {
    let mut cache = load_cache(app);
    if cache.known_app_ids.is_empty() {
        cache.known_app_ids = ids.into_iter().collect();
        let _ = save_cache(app, &cache);
        return;
    }
    cache.known_app_ids = ids.into_iter().collect();
    let _ = save_cache(app, &cache);
}

fn persist_pending_job(app: &AppHandle, record: PendingJobRecord) {
    let mut cache = load_cache(app);
    cache.pending_jobs.retain(|job| job.id != record.id);
    cache.pending_jobs.push(record);
    let _ = save_cache(app, &cache);
}

fn remove_pending_job(app: &AppHandle, job_id: &str) {
    let mut cache = load_cache(app);
    cache.pending_jobs.retain(|job| job.id != job_id);
    let _ = save_cache(app, &cache);
}

fn append_action_history(app: &AppHandle, entry: ActionHistoryEntry) {
    let mut cache = load_cache(app);
    cache.action_history.retain(|item| item.id != entry.id);
    cache.action_history.push(entry);
    cache
        .action_history
        .sort_by_key(|item| std::cmp::Reverse(item.finished_at));
    cache.action_history.truncate(200);
    let _ = save_cache(app, &cache);
}

fn remove_action_history(app: &AppHandle, job_id: &str) {
    let mut cache = load_cache(app);
    cache.action_history.retain(|item| item.id != job_id);
    let _ = save_cache(app, &cache);
}

fn record_job_history(
    app: &AppHandle,
    job: &InstallJob,
    status: &str,
    account_email: Option<String>,
    device_udid: Option<String>,
) {
    append_action_history(
        app,
        ActionHistoryEntry {
            id: job.id.clone(),
            account_email,
            device_udid,
            title: job.title.clone(),
            subtitle: job.subtitle.clone(),
            status: status.into(),
            kind: job.kind.clone(),
            finished_at: now(),
        },
    );
}

fn selected_device_connection(app: &AppHandle) -> String {
    load_settings(app)
        .ok()
        .and_then(|s| s.selected_device_connection)
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| v == "wifi" || v == "usb" || v == "network")
        .map(|v| if v == "network" { "wifi".into() } else { v })
        .unwrap_or_else(|| "usb".into())
}

/// Build args for idevice* tools: optional `-n` (Wi‑Fi) + `-u UDID` + command.
fn idevice_args_with_udid(app: &AppHandle, base: Vec<String>) -> Vec<String> {
    let mut args = Vec::new();
    let conn = selected_device_connection(app);
    let use_wifi = conn == "wifi";
    if use_wifi {
        args.push("-n".into());
    }
    if let Some(udid) = selected_device_udid(app) {
        args.push("-u".into());
        args.push(udid);
    }
    args.extend(base);
    args
}

/// Like `idevice_args_with_udid` but force network (`-n`).
fn idevice_args_network(app: &AppHandle, base: Vec<String>) -> Vec<String> {
    let mut args = vec!["-n".into()];
    if let Some(udid) = selected_device_udid(app) {
        args.push("-u".into());
        args.push(udid);
    }
    args.extend(base);
    args
}

fn has_active_ipa_work(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    tauri::async_runtime::block_on(async {
        let jobs = state.jobs.lock().await;
        jobs.values().any(|job| {
            matches!(job.status, JobStatus::Queued | JobStatus::Running)
                && matches!(job.kind.as_str(), "install" | "export")
        })
    })
}

fn window_is_hidden(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .is_some_and(|visible| !visible)
}

fn show_signum_notification(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn show_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    app.state::<AppState>()
        .background_mode
        .store(false, Ordering::Relaxed);
}

fn take_tray_notice(path: &Path) -> bool {
    let count = fs::read_to_string(path)
        .ok()
        .and_then(|value| value.trim().parse::<u8>().ok())
        .unwrap_or(0);
    count < 2 && fs::write(path, (count + 1).to_string()).is_ok()
}

fn hide_window_to_tray(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.hide();
    app.state::<AppState>()
        .background_mode
        .store(true, Ordering::Relaxed);
    if !load_settings(app).map(|settings| settings.background_enabled).unwrap_or(false) {
        return;
    }
    let Ok(path) = settings_path(app).map(|path| path.with_file_name("tray-notice-count.txt")) else {
        return;
    };
    if !take_tray_notice(&path) {
        return;
    }
    show_signum_notification(
        app,
        "Signum в трее",
        "Загрузка продолжается в фоне. Нажмите на иконку Signum в трее, чтобы открыть приложение.",
    );
}

fn notify_job_finished(app: &AppHandle, job: &InstallJob, success: bool) {
    if !app
        .state::<AppState>()
        .background_mode
        .load(Ordering::Relaxed)
        && !window_is_hidden(app)
    {
        return;
    }
    let title = job.title.trim();
    let body = match (success, job.kind.as_str()) {
        (true, "export") => format!("{title}: файл сохранён в папку экспорта"),
        (true, "install") => format!("{title}: установлено на iPhone"),
        (false, "export") => format!("{title}: загрузка не удалась. {}", job.error.as_deref().unwrap_or("Откройте Signum и повторите.")),
        (false, "install") => format!("{title}: установка не удалась. {}", job.error.as_deref().unwrap_or("Откройте Signum и повторите.")),
        _ => return,
    };
    show_signum_notification(app, "Signum", &body);
}

fn setup_tray(app: &AppHandle) -> Result<(), String> {
    let show_item = MenuItem::with_id(app, "tray-show", "Открыть Signum", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit_item = MenuItem::with_id(app, "tray-quit", "Выход", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let menu =
        Menu::with_items(app, &[&show_item, &separator, &quit_item]).map_err(|e| e.to_string())?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "Не удалось загрузить иконку трея".to_string())?;
    let app_handle = app.clone();
    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("Signum")
        .show_menu_on_left_click(false)
        .on_menu_event(move |handle, event| match event.id.as_ref() {
            "tray-show" => show_window(&handle),
            "tray-quit" => {
                diag_log::on_exit(handle);
                handle.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .build(&app_handle)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn looks_like_version(value: &str) -> bool {
    static VERSION: OnceLock<Regex> = OnceLock::new();
    VERSION
        .get_or_init(|| Regex::new(r"^\d+(?:\.\d+)*(?:[-+][A-Za-z0-9.]+)?$").unwrap())
        .is_match(value.trim())
}

fn parse_ota(value: &JsonValue) -> OtaUpdateInfo {
    parse_ota_for_package(value, cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none())
}

fn parse_ota_for_package(value: &JsonValue, prefer_deb: bool) -> OtaUpdateInfo {
    let use_deb = prefer_deb && value.get("deb_download_url").and_then(JsonValue::as_str).is_some()
        && value.get("deb_sha256").and_then(JsonValue::as_str).is_some();
    let client_supported = value
        .get("client_supported")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    OtaUpdateInfo {
        has_update: json_bool(value.get("has_update"), false) || !client_supported,
        version: value
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        build: value
            .get("build")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        download_url: value
            .get(if use_deb { "deb_download_url" } else { "download_url" })
            .and_then(|v| v.as_str())
            .map(str::to_string),
        notes: value
            .get("notes")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        mandatory: json_bool(value.get("mandatory"), false) || !client_supported,
        platform: value
            .get("platform")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        release_available: json_bool(value.get("release_available"), true),
        sha256: value
            .get(if use_deb { "deb_sha256" } else { "sha256" })
            .and_then(|v| v.as_str())
            .map(str::to_string),
        min_client_version: value
            .get("min_client_version")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        client_supported,
    }
}

fn emit_client_outdated(app: &AppHandle, update: &OtaUpdateInfo, message: &str) {
    let _ = app.emit(
        "client-outdated",
        ClientOutdatedEvent {
            message: message.to_string(),
            min_client_version: update.min_client_version.clone(),
            latest_version: update.version.clone(),
            download_url: update.download_url.clone(),
            sha256: update.sha256.clone(),
        },
    );
}

fn catalog_item_id(item: &JsonValue) -> Option<String> {
    item.get("id")
        .and_then(|v| v.as_str().map(str::to_string))
        .or_else(|| {
            item.get("id")
                .and_then(|v| v.as_u64())
                .map(|v| v.to_string())
        })
}

fn catalog_item_title(item: &JsonValue) -> String {
    item.get("title")
        .or_else(|| item.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("Приложение")
        .to_string()
}

fn dev_candidate(relative: &str) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    [cwd.join(relative), cwd.parent()?.join(relative)]
        .into_iter()
        .find(|p| p.exists())
}

fn bundled_or(app: &AppHandle, resource: &str, dev: &str, fallback: &str) -> String {
    if let Ok(path) = app.path().resolve(resource, BaseDirectory::Resource) {
        if path.exists() {
            return path.to_string_lossy().into_owned();
        }
    }
    dev_candidate(dev)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| fallback.to_string())
}

/// Where the `apple-tools` tree shipped with the app lives, remembered once.
///
/// A managed (OTA) ipatool runs from AppData / Application Support, far from
/// the helpers bundled beside it, and the spawn path has no `AppHandle` to ask.
static BUNDLED_TOOLS_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn remember_bundled_tools_root(app: &AppHandle) {
    if BUNDLED_TOOLS_ROOT.get().is_some() {
        return;
    }
    if let Ok(path) = app
        .path()
        .resolve("resources/apple-tools", BaseDirectory::Resource)
    {
        if path.is_dir() {
            let _ = BUNDLED_TOOLS_ROOT.set(path);
        }
    }
}

/// Bundled `apple-tools/<sub>` directory (e.g. "windows-x64/v2"), if shipped.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn bundled_tools_dir(sub: &str) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(root) = BUNDLED_TOOLS_ROOT.get() {
        roots.push(root.clone());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Windows keeps resources beside the binary; a .app puts them in
            // Contents/Resources while the binary sits in Contents/MacOS.
            roots.push(dir.join("resources").join("apple-tools"));
            roots.push(
                dir.join("..")
                    .join("Resources")
                    .join("resources")
                    .join("apple-tools"),
            );
        }
    }
    if let Some(dev) = dev_candidate("apple-tools") {
        roots.push(dev);
    }
    roots
        .into_iter()
        .map(|root| root.join(sub))
        .find(|dir| dir.is_dir())
}

/// Resolve a command name from PATH (Linux/macOS). Falls back to bare name.
///
/// GUI apps on macOS often have a stripped PATH (no Homebrew). We also probe
/// common install prefixes so device tools work when launched from Finder.
#[cfg(unix)]
fn which_or(name: &str) -> String {
    if let Ok(path) = std::process::Command::new("which").arg(name).output() {
        if path.status.success() {
            let s = String::from_utf8_lossy(&path.stdout).trim().to_string();
            if !s.is_empty() && Path::new(&s).is_file() {
                return s;
            }
        }
    }
    // Homebrew / system prefixes (macOS GUI has minimal PATH).
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from("/usr/local/bin").join(name),
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/bin").join(name),
        PathBuf::from("/bin").join(name),
    ];
    if let Some(h) = home {
        candidates.push(h.join(".local/bin").join(name));
    }
    for path in candidates {
        if path.is_file() {
            return path.to_string_lossy().into_owned();
        }
    }
    name.to_string()
}

#[cfg(not(unix))]
fn which_or(name: &str) -> String {
    name.to_string()
}

#[cfg(unix)]
fn ensure_unix_executable(path: &str) {
    use std::os::unix::fs::PermissionsExt;
    let p = Path::new(path);
    if !p.is_file() {
        return;
    }
    if let Ok(meta) = fs::metadata(p) {
        let mode = meta.permissions().mode();
        if mode & 0o111 == 0 {
            let mut perms = meta.permissions();
            perms.set_mode(mode | 0o755);
            let _ = fs::set_permissions(p, perms);
        }
    }
}

/// Bundled libimobiledevice CLI, then Homebrew / PATH. Never asks the user to
/// install brew packages — the app ships the tools (and can silently fetch them).
/// Bundled tool directories to try, native slice first.
///
/// A universal (or Apple Silicon) build still has to work when only the Intel
/// slice of a helper is shipped: Rosetta 2 runs an x86_64 child of an arm64
/// parent, so falling back keeps device tools and the Store signer working
/// instead of silently disappearing.
#[cfg(target_os = "macos")]
fn macos_tool_arches() -> [&'static str; 2] {
    if cfg!(target_arch = "aarch64") {
        ["macos-arm64", "macos-x64"]
    } else {
        ["macos-x64", "macos-arm64"]
    }
}

#[cfg(target_os = "macos")]
fn macos_device_tool(app: &AppHandle, name: &str) -> String {
    for arch in macos_tool_arches() {
        let bundled = bundled_or(
            app,
            &format!("resources/apple-tools/{arch}/support/{name}"),
            &format!("apple-tools/{arch}/support/{name}"),
            name,
        );
        if Path::new(&bundled).is_file() {
            ensure_unix_executable(&bundled);
            return bundled;
        }
    }
    which_or(name)
}

/// Report missing device tools; never install anything.
///
/// This used to run `brew install libimobiledevice ideviceinstaller` whenever a
/// bundled binary was missing. Signum ships to people who have never heard of
/// Homebrew: installing packages on their Mac without asking is not ours to do,
/// it needs brew to exist in the first place, and it blocked the caller for
/// minutes on every retry because `resolve_tool_paths` runs on each operation.
/// The tools are bundled instead — the Apple Silicon slice falls back to the
/// Intel one through Rosetta — and a copy already on PATH is still accepted by
/// `macos_device_tool`. What is left here is the log line that tells support
/// which tool was missing.
#[cfg(target_os = "macos")]
fn ensure_macos_idevice_tools(paths: &ToolPaths) {
    for (path, name) in [
        (&paths.ideviceinfo, "ideviceinfo"),
        (&paths.ideviceinstaller, "ideviceinstaller"),
        (&paths.idevicepair, "idevicepair"),
    ] {
        if !tool_available(path) {
            ipatool_debug_log(&format!(
                "macos device tool missing: {name} (resolved to {path}) — bundle is incomplete"
            ));
        }
    }
}

fn command_on_path(name: &str) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("where")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        if std::process::Command::new("which")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return true;
        }
        // GUI macOS apps often have an empty PATH — probe Homebrew prefixes.
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut candidates: Vec<PathBuf> = vec![
            PathBuf::from("/usr/local/bin").join(name),
            PathBuf::from("/opt/homebrew/bin").join(name),
            PathBuf::from("/usr/bin").join(name),
        ];
        if let Some(h) = home {
            candidates.push(h.join(".local/bin").join(name));
        }
        candidates.iter().any(|p| p.is_file())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = name;
        false
    }
}

/// Engines in "auto" probe order: v5 is ipatool-cpp (Sorvigolova) whose SAP
/// signer is compiled in — no anisette, no helper process, same on every OS;
/// v4 is ipatool-go (sapfix) and signs Apple actions locally; v3 is the
/// long-standing fallback; v2 last because its sapsigner.exe crashes
/// (0xc0000409) on many Windows PCs.
/// Also the single source of truth for which engine names are valid.
/// v2 и v3 убраны 15.09.2026: ни один не знает ступени updateProduct, без
/// которой Apple не отдаёт сборки, выпущенные с начала сентября. Сохранённый у
/// пользователя выбор «v2»/«v3» приводится к «auto» — см. нормализацию ниже.
/// v6 убран 04.10.2026: это был тот же ipatool-cpp со своим MAC и серийником на
/// Apple ID, но релизный ipatool `IPATOOL_MAC` не читает, и подмена работала
/// только в наших собственных запросах. Сохранённый «v6» тоже приводится к
/// «auto»; код подмены оставлен и выключен — см. `ACCOUNT_IDENTITY_SPOOF`.
const IPATOOL_ENGINE_ORDER: &[&str] = &["v5", "v4"];
const IPATOOL_ENGINES: &[&str] = &["v5", "v4"];
static V6_RUNTIME_SELECTED: AtomicBool = AtomicBool::new(false);

struct V6LoginRuntimeGuard(AppHandle);

impl Drop for V6LoginRuntimeGuard {
    fn drop(&mut self) {
        let keep_v6 = active_ipatool_engine(&self.0).as_deref() == Some("v6");
        V6_RUNTIME_SELECTED.store(keep_v6, Ordering::Relaxed);
        if !keep_v6 {
            refresh_v6_identity(&self.0, None);
        }
    }
}

fn normalize_ipatool_engine(engine: &str) -> String {
    let lowered = engine.trim().to_ascii_lowercase();
    match IPATOOL_ENGINES.iter().find(|known| **known == lowered) {
        Some(known) => (*known).to_string(),
        // Anything else (including an engine name from a newer install that
        // was downgraded) is treated as "auto" rather than pinning a engine
        // that does not exist.
        None => "auto".into(),
    }
}

fn windows_plain_path(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    let stripped = raw
        .strip_prefix(r"\\?\")
        .or_else(|| raw.strip_prefix("//?/"))
        .unwrap_or(raw.as_ref());
    PathBuf::from(stripped)
}

fn sap_helper_path_for(ipatool: &str) -> Option<String> {
    let parent = Path::new(ipatool).parent()?;
    for name in ["sapsigner.exe", "sapsigner", "sapsign-ssh.exe", "sapsign-ssh"] {
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Some(windows_plain_path(&candidate).to_string_lossy().into_owned());
        }
    }
    None
}

/// Can this ipatool sign Store requests on its own? v2 needs a sapsigner.exe
/// next to it, so the helper probe answers for the Go engines. ipatool-cpp (v5)
/// has the signer compiled in and ships no helper — probing for a neighbouring
/// file would call it unsigned and push Signum onto its native Store client,
/// which then has to authenticate all over again.
fn ipatool_signs_store_requests(ipatool: &str) -> bool {
    is_cpp_ipatool(ipatool) || sap_helper_path_for(ipatool).is_some()
}

fn windows_sap_engine_ready(v2: &str) -> bool {
    tool_available(v2) && sap_helper_path_for(v2).is_some()
}

/// True for the ipatool-cpp binary (engine v5), bundled under `.../v5/` or
/// delivered by the tool updater as `ipatool-v5-<version>`. It parses flags
/// itself and has no `--non-interactive`: passing it swallows the subcommand
/// and prints "Unknown command" with exit code 0.
fn is_cpp_ipatool(program: &str) -> bool {
    let path = Path::new(program);
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    if !name.starts_with("ipatool") {
        return false;
    }
    // Tool updater writes "ipatool-v5-<version>"; the bundle ships ".../v5/ipatool".
    name.starts_with("ipatool-v5")
        || path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|dir| dir.to_str())
            .is_some_and(|dir| dir.eq_ignore_ascii_case("v5"))
}

fn store_session_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Signum").join("store-session"))
}

fn ipatool_debug_log(msg: &str) {
    diag_log::note("app", msg);
}

struct V6Identity {
    mac: String,
    serial: String,
    fresh: bool,
}

fn v6_slot() -> &'static StdMutex<Option<V6Identity>> {
    static SLOT: OnceLock<StdMutex<Option<V6Identity>>> = OnceLock::new();
    SLOT.get_or_init(|| StdMutex::new(None))
}

fn process_account_mac() -> &'static StdMutex<Option<String>> {
    static SLOT: OnceLock<StdMutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| StdMutex::new(None))
}

fn account_mac_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("HOME"))?;
    let dir = PathBuf::from(base).join("Signum").join("account-mac");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Locally administered unicast MAC, 12 lowercase hex. Avoids addresses already issued.
fn mint_local_mac(taken: &HashSet<String>) -> String {
    for _ in 0..8 {
        let raw = uuid::Uuid::new_v4();
        let bytes = raw.as_bytes();
        let first = (bytes[0] & 0xFE) | 0x02;
        let mac = format!(
            "{first:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
        );
        if !taken.contains(&mac) {
            return mac;
        }
    }
    let raw = uuid::Uuid::new_v4();
    let bytes = raw.as_bytes();
    let first = (bytes[6] & 0xFE) | 0x02;
    format!(
        "{first:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[7], bytes[8], bytes[9], bytes[10], bytes[11]
    )
}

fn mac_is_stored(mac: &str) -> bool {
    mac.len() == 12
        && mac.chars().all(|c| c.is_ascii_hexdigit())
        && (u8::from_str_radix(&mac[..2], 16).unwrap_or(0) & 0x03) == 0x02
}

fn load_or_create_account_mac(email: &str) -> (String, bool) {
    let key = account_storage_key(email);
    let Some(dir) = account_mac_dir() else {
        return (mint_local_mac(&HashSet::new()), true);
    };
    let path = dir.join(&key);
    if let Ok(raw) = fs::read_to_string(&path) {
        let mac = raw.trim().to_ascii_lowercase();
        if mac_is_stored(&mac) {
            return (mac, false);
        }
    }
    let mut taken = HashSet::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path() == path {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(entry.path()) {
                let mac = raw.trim().to_ascii_lowercase();
                if mac.len() == 12 {
                    taken.insert(mac);
                }
            }
        }
    }
    let mac = mint_local_mac(&taken);
    let _ = fs::write(path, &mac);
    (mac, true)
}

/// 10-character serial, the randomized form Apple has put on Macs since 2021.
/// No I or O, same alphabet the factory serials use. Stable per Apple ID.
fn mint_apple_serial(taken: &HashSet<String>) -> String {
    const ABC: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ0123456789";
    for salt in 0..8u8 {
        let raw = uuid::Uuid::new_v4();
        let bytes = raw.as_bytes();
        let mut serial = String::with_capacity(10);
        for (i, byte) in bytes.iter().take(10).enumerate() {
            let idx = (*byte as usize + salt as usize + i) % ABC.len();
            serial.push(ABC[idx] as char);
        }
        if !taken.contains(&serial) {
            return serial;
        }
    }
    mint_apple_serial_once(uuid::Uuid::new_v4().as_bytes(), 3)
}

fn mint_apple_serial_once(bytes: &[u8], salt: usize) -> String {
    const ABC: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ0123456789";
    let mut serial = String::with_capacity(10);
    for (i, byte) in bytes.iter().take(10).enumerate() {
        let idx = (*byte as usize + salt + i) % ABC.len();
        serial.push(ABC[idx] as char);
    }
    serial
}

fn serial_is_stored(serial: &str) -> bool {
    serial.len() == 10
        && serial
            .bytes()
            .all(|b| matches!(b, b'A'..=b'H' | b'J'..=b'N' | b'P'..=b'Z' | b'0'..=b'9'))
}

fn load_or_create_account_serial(email: &str) -> (String, bool) {
    let key = account_storage_key(email);
    let Some(dir) = account_mac_dir() else {
        return (mint_apple_serial(&HashSet::new()), true);
    };
    let path = dir.join(format!("{key}.serial"));
    if let Ok(raw) = fs::read_to_string(&path) {
        let serial = raw.trim().to_ascii_uppercase();
        if serial_is_stored(&serial) {
            return (serial, false);
        }
    }
    let mut taken = HashSet::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".serial") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(entry.path()) {
                let serial = raw.trim().to_ascii_uppercase();
                if serial_is_stored(&serial) {
                    taken.insert(serial);
                }
            }
        }
    }
    let serial = mint_apple_serial(&taken);
    let _ = fs::write(path, &serial);
    (serial, true)
}

fn v6_selected(app: &AppHandle) -> bool {
    let selected = V6_RUNTIME_SELECTED.load(Ordering::Relaxed)
        || load_settings(app)
            .map(|settings| settings.ipatool_engine == "v6")
            .unwrap_or(false);
    #[cfg(target_os = "macos")]
    { selected }
    #[cfg(not(target_os = "macos"))]
    { selected || active_ipatool_engine(app).as_deref() == Some("v6") }
}

fn v6_log_tag() -> String {
    match v6_slot().lock() {
        Ok(guard) => match guard.as_ref() {
            Some(id) => format!(
                "identity=v6 ua=configurator mac={}..{} sn={}..{} {}",
                &id.mac[..4],
                &id.mac[10..],
                &id.serial[..4],
                &id.serial[8..],
                if id.fresh { "new" } else { "reused" }
            ),
            None => "identity=stock".into(),
        },
        Err(_) => "identity=stock".into(),
    }
}

/// Своя личность на каждый Apple ID: MAC (для всех движков и нашего клиента
/// App Store) плюс серийник Mac и профиль Configurator для v6. Выключено вместе
/// с v6 — запросы уходят с одним GUID машины. Код оставлен на всякий случай:
/// включить можно этим флагом, но релизный ipatool `IPATOOL_MAC` не читает.
const ACCOUNT_IDENTITY_SPOOF: bool = false;

/// Bind the test client. Off unless settings pin v6. The MAC is created on the
/// first login of this Apple ID and reused after that; another account gets
/// another address.
fn refresh_v6_identity(app: &AppHandle, email_override: Option<&str>) {
    let email = email_override
        .map(normalize_account_email)
        .filter(|value| !value.is_empty())
        .or_else(|| active_account_email(app));
    let Some(email) = email.filter(|_| ACCOUNT_IDENTITY_SPOOF) else {
        if let Ok(mut slot) = process_account_mac().lock() {
            *slot = None;
        }
        apple_store::set_process_mac(None);
        if let Ok(mut slot) = v6_slot().lock() {
            *slot = None;
        }
        apple_store::set_v6_guid(None);
        apple_store::set_v6_serial(None);
        return;
    };
    let (mac, mac_fresh) = load_or_create_account_mac(&email);
    if let Ok(mut slot) = process_account_mac().lock() {
        *slot = Some(mac.clone());
    }
    apple_store::set_process_mac(Some(mac.clone()));
    if !v6_selected(app) {
        if let Ok(mut slot) = v6_slot().lock() {
            *slot = None;
        }
        apple_store::set_v6_guid(None);
        apple_store::set_v6_serial(None);
        return;
    }
    let (serial, serial_fresh) = load_or_create_account_serial(&email);
    apple_store::set_v6_guid(Some(mac.clone()));
    apple_store::set_v6_serial(Some(serial.clone()));
    if let Ok(mut slot) = v6_slot().lock() {
        *slot = Some(V6Identity {
            mac,
            serial,
            fresh: mac_fresh || serial_fresh,
        });
    }
    ipatool_debug_log(&format!("v6-identity account={email} {}", v6_log_tag()));
}

fn auth_stage(stage: &str, email: &str, detail: &str) {
    ipatool_debug_log(&format!(
        "v6-auth stage={stage} account={} {detail} {}",
        normalize_account_email(email),
        v6_log_tag()
    ));
}

/// Текст ошибки входа без служебной метки в начале.
///
/// Метки нужны коду (`SIGNUM_APPLE_UNAVAILABLE:` останавливает переход на
/// второй движок), а на странице аккаунта сохранённая ошибка показывается как
/// есть — человеку метка ни к чему.
fn without_login_marker(msg: &str) -> &str {
    let msg = msg.trim();
    for marker in [APPLE_UNAVAILABLE_MARKER, "SIGNUM_LOGIN_OPAQUE:"] {
        if let Some(rest) = msg.strip_prefix(marker) {
            return rest.trim_start();
        }
    }
    msg
}

fn save_last_catalog_error(msg: &str) {
    let msg = without_login_marker(msg);
    let Some(dir) = store_session_dir() else {
        return;
    };
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("last-catalog-error.txt");
    let previous = fs::read_to_string(&path).unwrap_or_default();
    if previous.trim() != msg.trim() {
        ipatool_debug_log(&format!("catalog-error {msg}"));
    }
    let _ = fs::write(path, msg);
}

fn clear_last_catalog_error() {
    if let Some(dir) = store_session_dir() {
        let _ = fs::remove_file(dir.join("last-catalog-error.txt"));
    }
}

fn load_last_catalog_error() -> Option<String> {
    let path = store_session_dir()?.join("last-catalog-error.txt");
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn technical_excerpt(text: &str, max_chars: usize) -> String {
    let excerpt = strip_ansi(text)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(20)
        .collect::<Vec<_>>()
        .join("\n");
    if excerpt.chars().count() <= max_chars {
        excerpt
    } else {
        excerpt.chars().take(max_chars).collect::<String>()
    }
}

/// Attach the raw tool log once.
///
/// `error` has usually already been through `format_process_error`, which
/// prepends the very same friendly text plus its own «Подробный лог» block.
/// Wrapping that again showed the user the same paragraph twice and buried the
/// real line, so keep the innermost log under a single headline.
fn with_process_log(base: &str, error: &str) -> String {
    if !DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed) {
        return base.to_string();
    }
    let trimmed = error.trim();
    if trimmed.is_empty() || trimmed == base {
        return base.to_string();
    }
    let marker = "Подробный лог:";
    let innermost = match trimmed.rsplit_once(marker) {
        Some((_, rest)) => rest.trim(),
        None => trimmed,
    };
    if innermost.is_empty() || innermost == base {
        return base.to_string();
    }
    format!("{base}\n\n{marker}\n{innermost}")
}

fn with_technical_detail(friendly: &str, raw: &str) -> String {
    if !DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed) {
        return friendly.to_string();
    }
    let technical = technical_excerpt(raw, 1600);
    if technical.is_empty() || technical == friendly {
        friendly.to_string()
    } else {
        format!("{friendly}\n\nТехнически:\n{technical}")
    }
}

fn redact_cmd_args(args: &[String]) -> String {
    let mut out = Vec::with_capacity(args.len());
    let mut hide_next = false;
    for arg in args {
        if hide_next {
            out.push("********".into());
            hide_next = false;
            continue;
        }
        let lower = arg.to_ascii_lowercase();
        if lower == "-p"
            || lower == "--password"
            || lower == "--keychain-passphrase"
            || lower == "-a"
            || lower == "--auth-code"
        {
            hide_next = true;
        }
        out.push(arg.clone());
    }
    out.join(" ")
}

fn ipatool_session_diag() -> String {
    match ipatool_data_dir() {
        Some(dir) => {
            let account = dir.join("account");
            let keyring_account = dir.join("keyring").join("account");
            let cookies = dir.join("cookies");
            format!(
                "home={} account_exists={} account_len={} keyring_account_exists={} keyring_account_len={} cookies_exists={} cookies_len={} USERPROFILE={:?} HOMEDRIVE={:?} HOMEPATH={:?} HOME={:?}",
                dir.display(),
                account.is_file(),
                fs::metadata(&account).map(|m| m.len()).unwrap_or(0),
                keyring_account.is_file(),
                fs::metadata(&keyring_account).map(|m| m.len()).unwrap_or(0),
                cookies.is_file(),
                fs::metadata(&cookies).map(|m| m.len()).unwrap_or(0),
                std::env::var_os("USERPROFILE"),
                std::env::var_os("HOMEDRIVE"),
                std::env::var_os("HOMEPATH"),
                std::env::var_os("HOME"),
            )
        }
        None => format!(
            "home=<none> USERPROFILE={:?} HOMEDRIVE={:?} HOMEPATH={:?} HOME={:?}",
            std::env::var_os("USERPROFILE"),
            std::env::var_os("HOMEDRIVE"),
            std::env::var_os("HOMEPATH"),
            std::env::var_os("HOME"),
        ),
    }
}

fn should_log_helper_process(program: &str) -> bool {
    Path::new(program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase()
        .contains("ipatool")
        || Path::new(program)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains("anisette")
        || Path::new(program)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains("sapsign")
}

#[cfg(target_os = "windows")]
fn sap_sidecar_source_dirs(ipatool: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(parent) = Path::new(ipatool).parent() {
        dirs.push(parent.to_path_buf());
        // Библиотеки лежат в support. Раньше здесь искалась папка v2 — вместе
        // со снятым движком ушёл и источник, и kbsync.exe оставался без
        // libunicorn.dll: «система не обнаружила libunicorn.dll».
        dirs.push(parent.join("..").join("support"));
        dirs.push(parent.join("..").join("windows-x64").join("support"));
    }
    // The copy shipped with Signum. Without it nothing above matches on a real
    // install: a managed (OTA) ipatool lives in AppData, so v2 downloaded with
    // helper=None and Apple answered "invalid response". Only the developer box
    // was covered, by an absolute path that means nothing on a user's PC.
    if let Some(dir) = bundled_tools_dir("windows-x64/support") {
        dirs.push(dir);
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(
            PathBuf::from(local)
                .join("signum")
                .join("resources")
                .join("apple-tools")
                .join("windows-x64")
                .join("support"),
        );
    }
    dirs.push(PathBuf::from(r"C:\ipatool"));
    dirs
}

#[cfg(target_os = "windows")]
fn sap_source_complete(dir: &Path) -> bool {
    ["sapsigner.exe", "ucworker.dll", "libunicorn.dll"]
        .iter()
        .all(|name| dir.join(name).is_file())
}

/// Состояние защитных механизмов Windows, которые мешают запуску kbsync.
///
/// Целостность памяти (HVCI) и Smart App Control блокируют сторонние бинари,
/// и отказ выглядит как «kbsync не отработал» без объяснения. Человеку нужно
/// сказать, что именно выключить, а не оставлять его гадать.
#[derive(Clone, Copy, PartialEq)]
struct WindowsGuards {
    memory_integrity: bool,
    smart_app_control: bool,
}

impl WindowsGuards {
    fn any(self) -> bool {
        self.memory_integrity || self.smart_app_control
    }

    /// Короткая строка для лога.
    fn as_note(self) -> String {
        format!(
            "целостность памяти={} smart app control={}",
            if self.memory_integrity { "ВКЛ" } else { "выкл" },
            if self.smart_app_control { "ВКЛ" } else { "выкл" },
        )
    }

    /// Что сказать человеку. Пусто, если мешать нечему.
    fn advice(self) -> Option<String> {
        if !self.any() {
            return None;
        }
        let mut steps = Vec::new();
        if self.memory_integrity {
            steps.push(
                "Безопасность Windows → Безопасность устройства → Изоляция ядра →                  Целостность памяти → выключить и перезагрузить компьютер",
            );
        }
        if self.smart_app_control {
            steps.push(
                "Безопасность Windows → Управление приложениями и браузером →                  Smart App Control → выключить",
            );
        }
        Some(format!(
            "Защита Windows блокирует нужный Signum компонент. Откройте Параметры:

{}",
            steps
                .iter()
                .enumerate()
                .map(|(i, step)| format!("{}. {}", i + 1, step))
                .collect::<Vec<_>>()
                .join("
")
        ))
    }
}

/// Прочитать число из реестра.
///
/// Через reg.exe, а не winreg: лишняя зависимость ради пары значений не
/// окупается, а reg.exe есть в любой Windows.
#[cfg(target_os = "windows")]
fn read_registry_dword(key: &str, value: &str) -> Option<u32> {
    use std::os::windows::process::CommandExt;
    use std::process::Command as StdCommand;

    let out = StdCommand::new("reg")
        .args(["query", key, "/v", value])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let raw = text.split_whitespace().last()?;
    let hex = raw.strip_prefix("0x").unwrap_or(raw);
    u32::from_str_radix(hex, 16).ok()
}

/// Прочитать состояние защит из реестра.
#[cfg(target_os = "windows")]
fn windows_guards() -> WindowsGuards {
    WindowsGuards {
        memory_integrity: read_registry_dword(
            r"HKLM\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
            "Enabled",
        )
        .unwrap_or(0)
            == 1,
        // 0 — выключено, 1 — включено, 2 — режим оценки (тоже блокирует).
        smart_app_control: matches!(
            read_registry_dword(
                r"HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy",
                "VerifiedAndReputablePolicyState",
            ),
            Some(1) | Some(2)
        ),
    }
}

#[cfg(not(target_os = "windows"))]
fn windows_guards() -> WindowsGuards {
    WindowsGuards { memory_integrity: false, smart_app_control: false }
}

/// Стоит ли в системе распространяемый пакет Visual C++ 2015–2022.
///
/// Установщик пакета пишет сюда `Installed=1`. Ключ один на все выпуски с 2015
/// по 2022: они обновляют друг друга, а не ложатся рядом, — поэтому проверять
/// версию не нужно, достаточно факта установки. Второй путь нужен тем, у кого
/// запись легла в 32-разрядное отражение реестра.
#[cfg(target_os = "windows")]
fn windows_vc_redist_present() -> bool {
    const KEYS: [&str; 2] = [
        r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
    ];
    KEYS.iter()
        .any(|key| read_registry_dword(key, "Installed") == Some(1))
}

/// Что означает код возврата установщика пакета Visual C++.
///
/// 1638 — «уже стоит версия новее»: это успех, а не отказ. 3010 — поставилось,
/// но просит перезагрузку; пакет уже на месте, торопить человека незачем.
fn vc_redist_install_succeeded(code: i32) -> bool {
    matches!(code, 0 | 1638 | 3010)
}

/// Поставить пакет Visual C++, если его нет.
///
/// Если пакет уже есть — выходим молча, человеку ничего не показываем.
/// Если нет — тихо ставим: единственное, что он увидит, это запрос Windows
/// на повышение прав, без него установщик Microsoft не запускается.
#[cfg(target_os = "windows")]
async fn ensure_windows_vc_redist() -> Result<bool, String> {
    if windows_vc_redist_present() {
        return Ok(false);
    }
    diag_log::note("фон", "пакет Visual C++ не найден — ставлю");

    let target = std::env::temp_dir().join("Signum-vc_redist.x64.exe");
    let mirror = signum_mirror_url(VC_REDIST_MIRROR_NAME);
    // Зеркало первым: до него ближе и оно не упирается в блокировки, а
    // aka.ms остаётся запасным на случай, если зеркало отстало или лежит.
    let mut last = String::from("не удалось скачать пакет");
    let mut fetched = false;
    for url in [mirror.as_str(), VC_REDIST_URL] {
        match fetch_file_to(url, &target, 10_000_000).await {
            Ok(()) => {
                fetched = true;
                break;
            }
            Err(error) => last = error,
        }
    }
    if !fetched {
        return Err(last);
    }

    // Запускаем чужой установщик с повышенными правами — значит обязаны
    // убедиться, что это действительно файл Microsoft, а не подмена по дороге.
    // Пришпилить sha256 нельзя: Microsoft обновляет файл по тому же адресу.
    if !windows_file_signed_by_microsoft(&target).await {
        let _ = tokio::fs::remove_file(&target).await;
        return Err("пакет не прошёл проверку подписи".into());
    }

    let path = target.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $p = Start-Process -FilePath '{path}' \
           -ArgumentList @('/install','/quiet','/norestart') \
           -Verb RunAs -PassThru -Wait; \
         if ($null -eq $p) {{ exit 1 }}; \
         exit $p.ExitCode"
    );
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|_| "не удалось запустить установку пакета".to_string())?;

    let _ = tokio::fs::remove_file(&target).await;
    let code = output.status.code().unwrap_or(-1);
    if code == 1223 {
        return Err("установку пакета отклонили в запросе Windows".into());
    }
    if !vc_redist_install_succeeded(code) {
        return Err(format!("установка пакета вернула код {code}"));
    }
    Ok(true)
}

/// Скачать файл во временный. Отдельно от загрузчика установщиков Apple:
/// тот завязан на события окна iCloud, а здесь показывать нечего.
#[cfg(target_os = "windows")]
async fn fetch_file_to(url: &str, target: &Path, min_bytes: u64) -> Result<(), String> {
    let mut last_error = "не удалось скачать файл".to_string();
    for url in api_download_urls(url) {
        match fetch_file_once(&url, target, min_bytes).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                let _ = tokio::fs::remove_file(target).await;
                last_error = error;
            }
        }
    }
    Err(last_error)
}

#[cfg(target_os = "windows")]
async fn fetch_file_once(url: &str, target: &Path, min_bytes: u64) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    let response = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|error| error.to_string())?
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("сервер ответил {}", response.status().as_u16()));
    }

    let _ = tokio::fs::remove_file(target).await;
    let mut file = tokio::fs::File::create(target)
        .await
        .map_err(|error| error.to_string())?;
    let mut written = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        file.write_all(&chunk)
            .await
            .map_err(|error| error.to_string())?;
        written += chunk.len() as u64;
    }
    file.flush().await.map_err(|error| error.to_string())?;
    drop(file);

    // Обрыв посреди загрузки оставляет короткий файл, и запускать его нельзя.
    if written < min_bytes {
        let _ = tokio::fs::remove_file(target).await;
        return Err("файл скачался не полностью".into());
    }
    Ok(())
}

/// Подписан ли файл действующей подписью Microsoft.
#[cfg(target_os = "windows")]
async fn windows_file_signed_by_microsoft(path: &Path) -> bool {
    let quoted = path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$s = Get-AuthenticodeSignature -LiteralPath '{quoted}'; \
         if ($s.Status -ne 'Valid') {{ exit 1 }}; \
         if ($s.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {{ exit 1 }}; \
         exit 0"
    );
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Проверка при запуске. На остальных системах делать нечего.
async fn startup_vc_redist_check(_app: AppHandle) {
    #[cfg(target_os = "windows")]
    match ensure_windows_vc_redist().await {
        Ok(false) => {}
        Ok(true) => diag_log::note("фон", "пакет Visual C++ установлен"),
        Err(error) => diag_log::note("фон", &format!("пакет Visual C++: {error}")),
    }
}

/// Снимок окружения движка в лог: что лежит рядом и свеж ли кеш.
///
/// Пропавшая `libunicorn.dll` стоила полудня поисков — в логе было видно
/// только «не удалось получить список версий», а причина лежала на диске.
/// Печатаем раз за запуск: строка полезна ровно в начале разбора.
fn log_engine_environment(ipatool: &str) {
    use std::sync::atomic::AtomicBool;
    static PRINTED: AtomicBool = AtomicBool::new(false);
    if PRINTED.swap(true, Ordering::SeqCst) || !diag_log::is_active() {
        return;
    }
    let Some(dir) = Path::new(ipatool).parent() else {
        return;
    };
    // Имена спутников разные на разных системах, а смысл один: если чего-то из
    // этого нет рядом с движком, снятые с продажи приложения качаться не будут.
    #[cfg(target_os = "windows")]
    let needed = ["ipatool.exe", "kbsync.exe", "libunicorn.dll", "ucworker.dll", "assets.bin"];
    #[cfg(target_os = "macos")]
    let needed = ["ipatool", "kbsync", "libunicorn.2.dylib", "assets.bin"];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let needed = ["ipatool", "kbsync", "assets.bin"];

    let mut parts = Vec::new();
    for name in needed {
        let path = dir.join(name);
        parts.push(match fs::metadata(&path) {
            Ok(meta) => format!("{name}={}", meta.len()),
            Err(_) => format!("{name}=НЕТ"),
        });
    }
    // Возраст кеша объясняет, почему запрос пошёл быстрым или долгим путём.
    let cache = match fs::metadata(dir.join("kbsync.cache")).and_then(|m| m.modified()) {
        Ok(modified) => modified
            .elapsed()
            .map(|age| format!("kbsync.cache={}с", age.as_secs()))
            .unwrap_or_else(|_| "kbsync.cache=?".into()),
        Err(_) => "kbsync.cache=НЕТ".into(),
    };
    parts.push(cache);
    let guards = windows_guards();
    diag_log::note(
        "engine",
        &format!("{} | {} | {}", dir.display(), parts.join(" "), guards.as_note()),
    );
}

/// Спутники kbsync для cpp-движка.
///
/// Основная функция для него выходит сразу — и правильно, sapsigner рядом с
/// cpp сбивает определение движка. Но kbsync ему нужен, а вместе с ним
/// запечатанные артефакты и обе библиотеки Unicorn. Без этого ступень
/// ent/download молча выключена, и снятые с продажи сборки не скачиваются.
#[cfg(target_os = "windows")]
fn ensure_windows_kbsync_sidecars(ipatool: &str) {
    let Some(dest) = Path::new(ipatool).parent() else {
        return;
    };
    if !dest.is_dir() {
        return;
    }
    let Some(source) = sap_sidecar_source_dirs(ipatool)
        .into_iter()
        .find(|dir| dir.join("kbsync.exe").is_file() && dir.join("assets.bin").is_file())
    else {
        ipatool_debug_log("kbsync sidecars: источник не найден");
        return;
    };
    for name in ["kbsync.exe", "assets.bin", "libunicorn.dll", "ucworker.dll"] {
        let from = source.join(name);
        let to = dest.join(name);
        if from == to || !from.is_file() {
            continue;
        }
        // Размер сверяем, чтобы обновлённый artefact не остался старым рядом
        // с новым движком.
        let same = fs::metadata(&to)
            .ok()
            .zip(fs::metadata(&from).ok())
            .map(|(a, b)| a.len() == b.len())
            .unwrap_or(false);
        if !same {
            let _ = fs::copy(&from, &to);
        }
        unblock_windows_file(&to);
    }
    // Открытые дылибы рядом с движком больше не нужны: их заменил контейнер.
    let stale = dest.join("sap-cache");
    if stale.is_dir() {
        let _ = fs::remove_dir_all(&stale);
    }
}

/// Copy local FairPlay signer + Unicorn DLLs next to the ipatool Signum actually launches.
#[cfg(target_os = "windows")]
fn ensure_windows_sap_sidecars(ipatool: &str) {
    // ipatool-cpp carries CoreFP/storeagent inside the binary; dropping a
    // sapsigner.exe beside it only makes sap_helper_path_for() misreport the
    // engine as helper-driven.
    if is_cpp_ipatool(ipatool) {
        ensure_windows_kbsync_sidecars(ipatool);
        return;
    }
    let file_name = Path::new(ipatool)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if !file_name.to_ascii_lowercase().contains("ipatool") {
        return;
    }
    let Some(dest) = Path::new(ipatool).parent() else {
        return;
    };
    if !dest.is_dir() {
        return;
    }
    let source = sap_sidecar_source_dirs(ipatool)
        .into_iter()
        .find(|dir| sap_source_complete(dir));
    let Some(source) = source else {
        let probed = sap_sidecar_source_dirs(ipatool)
            .iter()
            .map(|dir| dir.display().to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        ipatool_debug_log(&format!("sap sidecars missing for {ipatool}; probed {probed}"));
        return;
    };
    // kbsync.exe — то же семейство: эмулирует FairPlay и выдаёт подтверждение,
    // без которого DownloadDispatch отказывает в снятых с продажи сборках.
    for name in ["sapsigner.exe", "kbsync.exe", "ucworker.dll", "libunicorn.dll"] {
        let from = source.join(name);
        let to = dest.join(name);
        if from == to {
            unblock_windows_file(&to);
            continue;
        }
        if !to.is_file() {
            if fs::copy(&from, &to).is_ok() {
                ipatool_debug_log(&format!("copied {name} -> {}", to.display()));
            }
        }
        unblock_windows_file(&to);
    }
    // Запечатанный контейнер вместо открытых дылибов. Плоскую папку sap-cache
    // рядом с движком больше не раскладываем и подчищаем оставшуюся от прежних
    // версий: скопировать её вместе с kbsync было самым простым способом унести
    // рабочий генератор подтверждений.
    let sealed_from = source.join("assets.bin");
    let sealed_to = dest.join("assets.bin");
    if sealed_from.is_file() && sealed_from != sealed_to && !sealed_to.is_file() {
        let _ = fs::copy(&sealed_from, &sealed_to);
    }
    let stale_cache = dest.join("sap-cache");
    if stale_cache.is_dir() {
        let _ = fs::remove_dir_all(&stale_cache);
    }
    unblock_windows_file(&dest.join("sapsigner.exe"));
}

/// Скачивает контейнер с библиотеками FairPlay с нашего сервера.
///
/// Содержимое одинаково для всех и ничего пользовательского не содержит —
/// это системные библиотеки Apple из публичного пакета обновления macOS.
/// Целостность сверяем по sha256 из заголовка: битый контейнер хуже
/// отсутствующего, потому что ошибка вылезет позже и не там.
async fn fetch_sealed_assets(app: &AppHandle, target: &Path) -> Result<usize, String> {
    let _ = app;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let mut last_error = "не удалось скачать библиотеки".to_string();
    for url in api_urls("lib.php?name=assets") {
        match fetch_sealed_assets_once(&client, &url, target).await {
            Ok(size) => return Ok(size),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

async fn fetch_sealed_assets_once(
    client: &reqwest::Client,
    url: &str,
    target: &Path,
) -> Result<usize, String> {
    let response = client.get(url)
        .send()
        .await
        .map_err(|e| format!("сеть: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("сервер ответил {}", response.status()));
    }
    let expected = response
        .headers()
        .get("x-signum-sha256")
        .and_then(|v| v.to_str().ok())
        .map(str::to_ascii_lowercase);
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("пустой ответ".into());
    }
    if let Some(expected) = expected {
        use sha2::{Digest, Sha256};
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if actual != expected {
            return Err("контрольная сумма не сошлась".into());
        }
    }
    // Пишем через временный файл: оборванная запись оставила бы обрезанный
    // контейнер, который выглядит существующим и больше не перекачивается.
    let tmp = target.with_extension("part");
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, target).map_err(|e| e.to_string())?;
    Ok(bytes.len())
}

/// Имя записи автозапуска в реестре Windows.
const AUTOSTART_NAME: &str = "Signum";

/// Включает или выключает запуск при входе в систему.
///
/// Через реестр, а не через службу: служба работает без сеанса пользователя и
/// потому не может ни показать значок в трее, ни спросить код двухфакторной
/// проверки, ни увидеть подключённый iPhone. Запуск в трей даёт то же удобство
/// без этих ограничений.
#[cfg(target_os = "windows")]
fn set_autostart(enabled: bool) -> Result<(), String> {
    use std::process::Command as StdCommand;
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut cmd = StdCommand::new("reg");
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // Флаг --background просит стартовать сразу в трей, не показывая окно.
        let value = format!("\"{}\" --background", exe.display());
        cmd.args(["add", KEY, "/v", AUTOSTART_NAME, "/t", "REG_SZ", "/d", &value, "/f"]);
    } else {
        cmd.args(["delete", KEY, "/v", AUTOSTART_NAME, "/f"]);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    // Удаление несуществующей записи — не ошибка.
    if !out.status.success() && enabled {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

/// Запуск при входе в систему на macOS — через агент запуска.
///
/// launchd читает plist из ~/Library/LaunchAgents при каждом входе. Прав
/// администратора не нужно: это папка самого пользователя.
#[cfg(target_os = "macos")]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "Не найдена домашняя папка".to_string())?;
    let dir = PathBuf::from(home).join("Library").join("LaunchAgents");
    let plist = dir.join("pw.appbank.signum.plist");
    if !enabled {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", &plist.to_string_lossy()])
            .output();
        let _ = fs::remove_file(&plist);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>pw.appbank.signum</string>
  <key>ProgramArguments</key><array><string>{}</string><string>--background</string></array>
  <key>RunAtLoad</key><true/>
</dict></plist>
"#,
        exe.display()
    );
    fs::write(&plist, body).map_err(|e| e.to_string())?;
    let _ = std::process::Command::new("launchctl")
        .args(["load", &plist.to_string_lossy()])
        .output();
    Ok(())
}

/// Запуск при входе в систему на Linux — по стандарту autostart.
///
/// Файл .desktop в ~/.config/autostart понимают все крупные рабочие среды.
#[cfg(all(unix, not(target_os = "macos")))]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "Не найдена домашняя папка".to_string())?;
    let dir = PathBuf::from(home).join(".config").join("autostart");
    let entry = dir.join("signum.desktop");
    if !enabled {
        let _ = fs::remove_file(&entry);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body = format!(
        "[Desktop Entry]
Type=Application
Name=Signum
Exec=\"{}\" --background
         X-GNOME-Autostart-enabled=true
NoDisplay=true
",
        exe.display()
    );
    fs::write(&entry, body).map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_background_mode(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = load_settings(&app)?;
    settings.background_enabled = enabled;
    save_settings(app, settings)
}

/// Обновляет KBSync только для активной сохранённой сессии.
fn start_background_refresh(app: &AppHandle) {
    static RUNNING: OnceLock<AtomicBool> = OnceLock::new();
    let running = RUNNING.get_or_init(|| AtomicBool::new(false));
    if running.swap(true, Ordering::SeqCst) {
        return; // уже работает
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut first_pass = true;
        loop {
            if !first_pass {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            first_pass = false;
            let enabled = load_settings(&app)
                .map(|s| s.background_enabled)
                .unwrap_or(false);
            if !enabled {
                continue;
            }
            let Some(email) = active_account_email(&app)
                .filter(|email| saved_account_session_exists(&app, email))
            else {
                continue;
            };
            if kbsync_cache_is_fresh(&app, &email, &active_v5_version(&app)) {
                continue;
            }
            let state = app.state::<AppState>();
            let Ok(_tool_guard) = state.ipatool_lock.try_lock() else { continue; };
            if state.jobs.lock().await.values().any(|job| matches!(job.status, JobStatus::Queued | JobStatus::Running)) {
                continue;
            }
            warm_kbsync_now(&app, &email, KbsyncWarm::RefreshIfStale).await;
        }
    });
}

#[cfg(target_os = "windows")]
const KBSYNC_UPSTREAM_VERSION: &str = "1.3.0";
#[cfg(not(target_os = "windows"))]
const KBSYNC_UPSTREAM_VERSION: &str = "1.2.0";
const KBSYNC_REFRESH_AFTER_SECS: u64 = 3600;
/// Сколько команд сейчас просят остановить KBSync.
///
/// Счётчик, а не флаг: выход, вход и смена аккаунта могут наложиться друг на
/// друга (двойной щелчок, выход во время входа). С флагом первая же
/// закончившаяся команда сбрасывала отмену, пока вторая ещё ждала замок, и
/// фоновый прогрев мог стартовать заново прямо перед ней.
static KBSYNC_CANCEL_REQUESTS: AtomicUsize = AtomicUsize::new(0);

fn kbsync_cancel_requested() -> bool {
    KBSYNC_CANCEL_REQUESTS.load(Ordering::SeqCst) > 0
}

/// Пока живёт — KBSync не запускается, а уже запущенный завершается.
///
/// Создаётся ДО ожидания общего замка ipatool: прогрев держит этот замок, и
/// без отмены выход из Apple ID ждал бы конца порождения (у людей — до 172 с).
/// Снимается только при выходе из команды, то есть уже после того, как она
/// сама отпустила замок — раньше времени отмена не пропадает.
struct KbsyncCancelGuard;

impl KbsyncCancelGuard {
    fn new() -> Self {
        KBSYNC_CANCEL_REQUESTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for KbsyncCancelGuard {
    fn drop(&mut self) {
        // Команда переписала или удалила файл аккаунта — прежняя проверка
        // «blob на месте» к новому файлу уже не относится.
        forget_kbsync_presence();
        KBSYNC_CANCEL_REQUESTS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Чем закончился запуск, прерванный сменой аккаунта.
const KBSYNC_CANCELLED: &str = "KBSYNC_CANCELLED";

/// Зачем прогревают KBSync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KbsyncWarm {
    /// Фон: пересоздать blob, если отметке больше часа или сменился движок.
    RefreshIfStale,
    /// Перед опросом версий: породить, только если у аккаунта blob нет вовсе.
    /// Готовый движок отдаёт из файла аккаунта за доли секунды.
    OnlyIfMissing,
}

fn kbsync_warm_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn warm_kbsync_cache(app: &AppHandle, email: &str) {
    let app = app.clone();
    let email = normalize_account_email(email);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if !load_settings(&app).map(|s| s.background_enabled).unwrap_or(false) { return; }
        let state = app.state::<AppState>();
        let Ok(_tool_guard) = state.ipatool_lock.try_lock() else { return; };
        warm_kbsync_now(&app, &email, KbsyncWarm::RefreshIfStale).await;
    });
}

fn kbsync_stamp_path(app: &AppHandle, email: &str) -> Option<PathBuf> {
    account_session_dir(app, email)
        .ok()
        .map(|dir| dir.join(format!("kbsync-{KBSYNC_UPSTREAM_VERSION}.stamp")))
}

fn kbsync_cache_is_fresh(app: &AppHandle, email: &str, engine_version: &str) -> bool {
    let Some(path) = kbsync_stamp_path(app, email) else {
        return false;
    };
    if fs::read_to_string(&path).ok().as_deref() != Some(engine_version) {
        return false;
    }
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(KBSYNC_REFRESH_AFTER_SECS))
}

fn active_v5_version(app: &AppHandle) -> String {
    let path = if let Some(path) = managed_tool_path(app, "ipatool-v5") {
        path
    } else {
        let resource = if cfg!(target_os = "windows") {
            "resources/apple-tools/windows-x64/v5/ipatool.exe"
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "resources/apple-tools/macos-arm64/v5/ipatool"
            } else {
                "resources/apple-tools/macos-x64/v5/ipatool"
            }
        } else if cfg!(target_arch = "aarch64") {
            "resources/linux-arm64/ipatool-v5"
        } else {
            "resources/linux-x64/ipatool-v5"
        };
        let path = app
            .path()
            .resolve(resource, BaseDirectory::Resource)
            .ok()
            .filter(|path| path.is_file())
            .or_else(|| {
                let dev = resource.strip_prefix("resources/").unwrap_or(resource);
                dev_candidate(dev)
            });
        let Some(path) = path else {
            return KBSYNC_UPSTREAM_VERSION.to_string();
        };
        path
    };
    if !path.is_file() || !is_cpp_ipatool(&path.to_string_lossy()) {
        return "unavailable".into();
    }
    sha256_file_hex(&path)
        .map(|hash| format!("{KBSYNC_UPSTREAM_VERSION}:{hash}"))
        .unwrap_or_else(|_| KBSYNC_UPSTREAM_VERSION.to_string())
}

/// Чем закончился прогрев.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KbsyncWarmOutcome {
    /// Blob у аккаунта есть (порождён сейчас или уже лежал в файле).
    Ready,
    /// Прогрев не понадобился или не удался; повторять сразу незачем.
    Unavailable,
    /// Прерван входом, выходом или сменой аккаунта.
    Cancelled,
}

async fn warm_kbsync_now(app: &AppHandle, email: &str, mode: KbsyncWarm) -> KbsyncWarmOutcome {
    let _warm_guard = kbsync_warm_lock().lock().await;
    if kbsync_cancel_requested() {
        return KbsyncWarmOutcome::Cancelled;
    }
    let scope = match mode {
        KbsyncWarm::RefreshIfStale => "фон",
        KbsyncWarm::OnlyIfMissing => "каталог",
    };
    let email = normalize_account_email(email);
    let engine_version = active_v5_version(app);
    let cache_fresh = kbsync_cache_is_fresh(app, &email, &engine_version);
    if active_account_email(app).as_deref() != Some(email.as_str())
        || !saved_account_session_exists(app, &email)
    {
        return KbsyncWarmOutcome::Unavailable;
    }
    if cache_fresh {
        // Отметка свежая: blob порождён этим движком меньше часа назад.
        return KbsyncWarmOutcome::Ready;
    }
    let Ok(tools) = resolve_tool_paths(app) else {
        return KbsyncWarmOutcome::Unavailable;
    };
    if !is_cpp_ipatool(&tools.ipatool) {
        return KbsyncWarmOutcome::Unavailable;
    }
    let Ok(keychain) = resolve_ipatool_keychain(app) else {
        return KbsyncWarmOutcome::Unavailable;
    };
    // Фон сверяет владельца сессии сам. Перед опросом версий это уже сделал
    // вызывающий (ensure_active_ipatool_session), второй запуск движка лишний.
    if mode == KbsyncWarm::RefreshIfStale {
        match run_auth_info_once(&tools.ipatool, &keychain).await {
            Ok(owner) if normalize_account_email(&owner) == email => {}
            Ok(_) => return KbsyncWarmOutcome::Unavailable,
            Err(error) => {
                ipatool_debug_log(&format!(
                    "kbsync warm skipped: session unavailable ({})",
                    technical_excerpt(&error, 180)
                ));
                return KbsyncWarmOutcome::Unavailable;
            }
        }
    }

    let mut args = vec!["--format".into(), "json".into()];
    append_ipatool_keychain_args(&mut args, &keychain);
    args.push("kbsync".into());
    // A missing/expired marker means the account's embedded blob may be stale.
    // Force one generation after upgrade; subsequent catalog checks reuse it.
    // Перед опросом версий --refresh не нужен: движок вернёт blob из файла
    // аккаунта, а породит его, только если там пусто.
    if mode == KbsyncWarm::RefreshIfStale {
        args.push("--refresh".into());
    }
    if kbsync_cancel_requested() {
        return KbsyncWarmOutcome::Cancelled;
    }
    diag_log::note(scope, "прогрев KBSync для активного Apple ID: начат");
    let started = Instant::now();
    // Человек ждёт список версий — приоритет не занижаем.
    let low_priority = mode == KbsyncWarm::RefreshIfStale
        && std::env::args().any(|arg| arg == "--background")
        && app.state::<AppState>().background_mode.load(Ordering::Relaxed);
    let output = match run_process_inner(&tools.ipatool, &args, Duration::from_secs(300), None, true, low_priority).await {
        Ok(output) => output,
        Err(error) if error == KBSYNC_CANCELLED => {
            diag_log::note(
                scope,
                &format!(
                    "прогрев KBSync остановлен через {} с: меняется вход в Apple ID",
                    started.elapsed().as_secs()
                ),
            );
            return KbsyncWarmOutcome::Cancelled;
        }
        Err(error) => {
            diag_log::note(
                scope,
                &format!(
                "прогрев KBSync: ошибка за {} с — {}",
                started.elapsed().as_secs(),
                technical_excerpt(&redact_kbsync_output(&error), 240)
            ),
            );
            return KbsyncWarmOutcome::Unavailable;
        }
    };
    let Some(result) = parse_json_object_from_text(&output) else {
        diag_log::note(scope, "прогрев KBSync: движок вернул некорректный ответ");
        return KbsyncWarmOutcome::Unavailable;
    };
    if !json_bool(result.get("success"), false) {
        diag_log::note(
            scope,
            &format!(
                "прогрев KBSync: движок не подтвердил blob, length={} header={}",
                result.get("length").and_then(JsonValue::as_u64).unwrap_or(0),
                result.get("header").and_then(JsonValue::as_str).unwrap_or("нет")
            ),
        );
        return KbsyncWarmOutcome::Unavailable;
    }
    let source = result.get("source").and_then(JsonValue::as_str).unwrap_or("готово");
    if !kbsync_result_is_fresh_generation(mode, source) {
        // Blob уже лежал в файле аккаунта. Отметку не трогаем: она говорит
        // фону, когда blob порождали в последний раз, а сейчас его не порождали.
        diag_log::note(
            scope,
            &format!("KBSync уже есть у аккаунта ({source}), порождать не пришлось"),
        );
        // Blob мог породить сам движок во время list-versions или загрузки —
        // тогда в сохранённой копии сессии его ещё нет, и после её
        // восстановления он пропал бы, а мы считали бы, что он на месте.
        if let Err(error) = backup_ipatool_session_for_account(app, &email) {
            ipatool_debug_log(&format!("KBSync present but account session backup failed: {error}"));
        }
        return KbsyncWarmOutcome::Ready;
    }
    if let Err(error) = backup_ipatool_session_for_account(app, &email) {
        ipatool_debug_log(&format!("KBSync generated but account session backup failed: {error}"));
        return KbsyncWarmOutcome::Ready;
    }
    let Some(stamp) = kbsync_stamp_path(app, &email) else {
        return KbsyncWarmOutcome::Ready;
    };
    if let Some(parent) = stamp.parent() {
        if fs::create_dir_all(parent).is_err() {
            return KbsyncWarmOutcome::Ready;
        }
    }
    let temp = stamp.with_extension(format!("stamp.{}.tmp", std::process::id()));
    if fs::write(&temp, engine_version.as_bytes()).is_err() {
        return KbsyncWarmOutcome::Ready;
    }
    let _ = fs::remove_file(&stamp);
    if fs::rename(&temp, &stamp).is_err() {
        let _ = fs::remove_file(&temp);
        return KbsyncWarmOutcome::Ready;
    }
    diag_log::note(
        scope,
        &format!(
            "прогрев KBSync {source} готов за {} с",
            started.elapsed().as_secs()
        ),
    );
    KbsyncWarmOutcome::Ready
}

/// Порождён ли blob этим запуском — только тогда обновляется отметка.
///
/// Фон всегда зовёт движок с `--refresh`, и тот порождает заново. Перед
/// опросом версий `--refresh` нет: `source: "cache"` значит, что blob взят из
/// файла аккаунта как есть.
fn kbsync_result_is_fresh_generation(mode: KbsyncWarm, source: &str) -> bool {
    mode == KbsyncWarm::RefreshIfStale || !source.eq_ignore_ascii_case("cache")
}

/// Через сколько повторить проверку KBSync после неудачи.
const KBSYNC_PRESENCE_RETRY: Duration = Duration::from_secs(600);

/// Что известно о KBSync активного аккаунта в этом запуске приложения.
struct KbsyncPresence {
    email: String,
    ready: bool,
    checked_at: Instant,
}

fn kbsync_presence_memo() -> &'static StdMutex<Option<KbsyncPresence>> {
    static MEMO: OnceLock<StdMutex<Option<KbsyncPresence>>> = OnceLock::new();
    MEMO.get_or_init(|| StdMutex::new(None))
}

/// Нужно ли снова спрашивать движок про KBSync этого аккаунта.
///
/// Удачу помним до смены аккаунта. Отказ — десять минут: движок без команды
/// `kbsync` иначе запускался бы впустую перед каждой проверкой каталога, а
/// вечно помнить отказ нельзя — причина могла быть разовой.
fn kbsync_presence_needs_check(
    memo: Option<&KbsyncPresence>,
    email: &str,
    now: Instant,
) -> bool {
    match memo {
        Some(known) if known.email == email => {
            !known.ready
                && now.saturating_duration_since(known.checked_at) >= KBSYNC_PRESENCE_RETRY
        }
        _ => true,
    }
}

/// Перед серией `list-versions`: убедиться, что у аккаунта есть KBSync.
///
/// `list-versions` зовёт в движке `prepare_kbsync`, и при пустом файле аккаунта
/// тот порождает blob сам — 20 секунд на быстрой машине и до трёх минут на
/// медленной. Опрос версий при этом живёт 75 секунд: на медленной машине его
/// убивали до конца порождения, blob не сохранялся, и следующий опрос начинал
/// с нуля — список версий не появлялся никогда. Поэтому порождаем один раз
/// заранее, с запасом по времени.
///
/// Прежний `ensure_kbsync_warm` делал то же, но пересоздавал blob каждый час
/// (`--refresh`), даже когда тот был на месте, — и каталог раз в час замирал
/// на всё время порождения. Здесь blob порождается, только если его нет;
/// ежечасное обновление остаётся фону.
async fn ensure_kbsync_present(app: &AppHandle) {
    let Some(email) = active_account_email(app)
        .filter(|email| saved_account_session_exists(app, email))
    else {
        return;
    };
    if !kbsync_presence_needs_check(
        kbsync_presence_memo().lock().unwrap().as_ref(),
        &email,
        Instant::now(),
    ) {
        return;
    }
    let ready = match warm_kbsync_now(app, &email, KbsyncWarm::OnlyIfMissing).await {
        // Прервали сменой аккаунта — для нового аккаунта спросим заново.
        KbsyncWarmOutcome::Cancelled => return,
        KbsyncWarmOutcome::Ready => true,
        KbsyncWarmOutcome::Unavailable => false,
    };
    *kbsync_presence_memo().lock().unwrap() = Some(KbsyncPresence {
        email,
        ready,
        checked_at: Instant::now(),
    });
}

/// Забыть, что KBSync проверяли: файл аккаунта переписан или удалён.
fn forget_kbsync_presence() {
    *kbsync_presence_memo().lock().unwrap() = None;
}

#[cfg(not(target_os = "windows"))]
fn ensure_windows_sap_sidecars(_ipatool: &str) {}

/// Put the FairPlay signer next to the ipatool Signum is about to launch.
///
/// `sap_helper_path_for` only looks beside the binary, and a managed (OTA) v2
/// arrives alone. Without the signer v2 sends the Store request unsigned:
/// version names degrade to build numbers and downloads fail with
/// "invalid response" - the Mac twin of the Windows sidecar copy above.
/// Положить рядом с движком то, без чего он не породит kbsync.
///
/// Тот же смысл, что у `ensure_windows_kbsync_sidecars`: движок запускает
/// `kbsync` соседним процессом, а тому нужны библиотека Unicorn и запечатанный
/// контейнер. Без них на маке молча переставали работать снятые с продажи
/// приложения — ровно как на Windows, где это стоило людям нерабочей сборки.
///
/// Native arm64 files live in macos-arm64/v5; Intel in macos-x64/v5. On Apple
/// Silicon we prefer the native slice and only fall back to Rosetta.
/// Путь к zsign: сначала свой, потом PATH.
///
/// Свой лежит в support рядом с остальными инструментами и несёт библиотеки
/// рядом с собой (@executable_path), поэтому Homebrew на машине не нужен.
#[cfg(target_os = "macos")]
fn macos_bundled_zsign() -> String {
    for arch in macos_tool_arches() {
        if let Some(dir) = bundled_tools_dir(&format!("{arch}/support")) {
            let candidate = dir.join("zsign");
            if candidate.is_file() {
                let path = candidate.to_string_lossy().into_owned();
                ensure_unix_executable(&path);
                return path;
            }
        }
    }
    which_or("zsign")
}

fn macos_ipatool_embeds_kbsync(hash: &str) -> bool {
    matches!(hash,
        "cbf9d2241688801b6d5658bdbe7fc60d0a961a5634bedc3743959bfbc6d62800"
        | "b8fdb16f42e3c88a4ae073267f7899a1aaf23fac8f345cbaa50721e46d12a886"
        | "849286b63009d967a5248bdcb9cf0dc4f08e4880fdb5d076d0d40528239c8196"
        | "0bf3ee336b86946520a2b4be33538de288c8b61c5e026bbc9c4b8dcf2adbd83f"
        | "8c8fc8984a33f88e4a4de915280801be3e486f9159fea2301ca9b707990cbefd"
        | "c95ba7a45b330074090064d8dca4fb323a6bdf46126532561dc51a2e36d077f7"
        | "fc034e14afc66418f9afd608fbc2b0425086bce7a9ed43fca284a34a37fb754a"
        | "9900a16212e0697b64094fa4dec0d9da367b76f8d01b84604b21d9808d86347e"
        | "2d6b4391fed1e4f3778af730a5b95f0aaee139f8f0aede4c3e62b6cd66ff2c6d"
        | "2a5f789bbbc94e78eac1ae31fd55b883374b4244a23c9c65bf1240fad8b29594"
    )
}

#[cfg(target_os = "macos")]
fn ensure_macos_kbsync_sidecars(ipatool: &str) -> Result<(), String> {
    if !is_cpp_ipatool(ipatool) {
        return Ok(());
    }
    if sha256_file_hex(Path::new(ipatool)).ok().is_some_and(|hash| macos_ipatool_embeds_kbsync(&hash)) {
        ipatool_debug_log("kbsync sidecars: verified embedded ipatool-cpp release");
        return Ok(());
    }
    let dest = Path::new(ipatool)
        .parent()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| format!("App Store engine directory is unavailable: {}", Path::new(ipatool).display()))?;
    let mut sources: Vec<PathBuf> = vec![dest.to_path_buf()];
    // Native slice first (arm64 on Apple Silicon), Intel as Rosetta fallback.
    for arch in macos_tool_arches() {
        if let Some(dir) = bundled_tools_dir(&format!("{arch}/v5")) {
            sources.push(dir);
        }
        if let Some(parent) = dest.parent().and_then(|v| v.parent()) {
            sources.push(parent.join(arch).join("v5"));
        }
    }
    const NEEDED: [&str; 3] = ["kbsync", "libunicorn.2.dylib", "assets.bin"];
    let Some(source) = sources
        .into_iter()
        .find(|dir| NEEDED.iter().all(|name| dir.join(name).is_file()))
    else {
        return Err("В комплекте движка App Store отсутствуют kbsync и его библиотеки.".into());
    };
    for name in NEEDED {
        let from = source.join(name);
        let to = dest.join(name);
        if from != to {
            let same = fs::read(&to).ok().zip(fs::read(&from).ok()).is_some_and(|(a, b)| a == b);
            if !same {
                fs::copy(&from, &to).map_err(|error| {
                    format!("Не удалось подготовить компонент App Store {name}: {error}")
                })?;
            }
        }
        // Скопированный файл теряет право на исполнение — без этого macOS
        // откажется его запускать, а причина будет выглядеть как «нет kbsync».
        if name == "kbsync" {
            ensure_unix_executable(&to.to_string_lossy());
        }
        if !to.is_file() || fs::metadata(&to).map(|meta| meta.len() == 0).unwrap_or(true) {
            return Err(format!("Компонент Apple App Store {name} отсутствует или пуст."));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn ensure_macos_sap_sidecar(ipatool: &str) {
    // ipatool-cpp has the signer compiled in; a `sapsigner` beside it is dead
    // weight that also makes sap_helper_path_for() misreport the engine.
    if is_cpp_ipatool(ipatool) {
        return;
    }
    if !Path::new(ipatool)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().contains("ipatool"))
    {
        return;
    }
    let Some(dest_dir) = Path::new(ipatool).parent() else {
        return;
    };
    if !dest_dir.is_dir() {
        return;
    }
    let dest = dest_dir.join("sapsigner");
    if dest.is_file() {
        ensure_unix_executable(&dest.to_string_lossy());
        return;
    }
    // Native slice first; an Apple Silicon Mac runs the Intel signer under
    // Rosetta 2 rather than going without one.
    let source = macos_tool_arches()
        .into_iter()
        .filter_map(|arch| bundled_tools_dir(&format!("{arch}/v2")))
        .map(|dir| dir.join("sapsigner"))
        .find(|path| path.is_file());
    let Some(source) = source else {
        ipatool_debug_log(&format!("sap signer not bundled for {ipatool}"));
        return;
    };
    if fs::copy(&source, &dest).is_ok() {
        ensure_unix_executable(&dest.to_string_lossy());
        ipatool_debug_log(&format!("copied sapsigner -> {}", dest.display()));
    } else {
        ipatool_debug_log(&format!("sapsigner copy failed -> {}", dest.display()));
    }
}

#[cfg(not(target_os = "macos"))]
fn ensure_macos_sap_sidecar(_ipatool: &str) {}

/// Разложить Go-движок вместе с подписчиком в папку, куда можно писать.
///
/// Копируются все файлы из папки движка (рядом могут лежать его библиотеки)
/// и `sapsigner`. Возвращает путь к движку в новой папке.
///
/// Копируем содержимое, а не файл целиком: `fs::copy` на macOS переносит и
/// расширенные атрибуты, в том числе карантин с только что скачанного .app, а
/// помеченный карантином исполняемый файл вне пакета система может не
/// запустить.
///
/// Повторно не копирует: рядом кладётся список «имя, размер, время изменения»
/// исходных файлов, и пока он совпадает, папка считается готовой. Новая версия
/// приложения приносит другие файлы — список расходится, папка обновляется.
#[cfg(any(target_os = "macos", test))]
fn stage_engine_with_signer(
    engine: &Path,
    signer: &Path,
    stage_dir: &Path,
) -> Result<PathBuf, String> {
    // Две операции разом (вход и проверка каталога) не должны писать одни и
    // те же файлы наперегонки.
    static STAGING: StdMutex<()> = StdMutex::new(());
    let _serial = STAGING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let engine_name = engine
        .file_name()
        .ok_or_else(|| format!("у движка нет имени файла: {}", engine.display()))?;
    let source_dir = engine
        .parent()
        .ok_or_else(|| format!("у движка нет папки: {}", engine.display()))?;
    let mut wanted: Vec<(std::ffi::OsString, PathBuf)> = Vec::new();
    for entry in fs::read_dir(source_dir).map_err(|e| format!("{}: {e}", source_dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && entry.file_name() != "sapsigner" {
            wanted.push((entry.file_name(), path));
        }
    }
    wanted.push(("sapsigner".into(), signer.to_path_buf()));
    wanted.sort();

    let mut listing = String::new();
    for (name, path) in &wanted {
        let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|age| age.as_secs())
            .unwrap_or(0);
        listing.push_str(&format!("{}\t{}\t{modified}\n", name.to_string_lossy(), meta.len()));
    }

    let marker = stage_dir.join(".staged");
    let same_size = |name: &std::ffi::OsString, source: &PathBuf| {
        fs::metadata(stage_dir.join(name))
            .ok()
            .zip(fs::metadata(source).ok())
            .is_some_and(|(staged, original)| staged.is_file() && staged.len() == original.len())
    };
    let ready = fs::read_to_string(&marker).ok().as_deref() == Some(listing.as_str())
        && wanted.iter().all(|(name, source)| same_size(name, source));
    if !ready {
        fs::create_dir_all(stage_dir).map_err(|e| format!("{}: {e}", stage_dir.display()))?;
        // Список пишется последним: оборванное копирование не выдаст
        // недописанную папку за готовую.
        let _ = fs::remove_file(&marker);
        for (name, source) in &wanted {
            let target = stage_dir.join(name);
            let mut temp_name = name.clone();
            temp_name.push(format!(".{}.tmp", std::process::id()));
            let temp = stage_dir.join(temp_name);
            let copied = (|| -> std::io::Result<()> {
                let mut from = fs::File::open(source)?;
                let mut to = fs::File::create(&temp)?;
                std::io::copy(&mut from, &mut to)?;
                to.flush()?;
                drop(to);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&temp, fs::Permissions::from_mode(0o755))?;
                }
                fs::rename(&temp, &target)
            })();
            if let Err(error) = copied {
                let _ = fs::remove_file(&temp);
                return Err(format!("{} -> {}: {error}", source.display(), target.display()));
            }
        }
        fs::write(&marker, &listing).map_err(|e| format!("{}: {e}", marker.display()))?;
    }
    Ok(stage_dir.join(engine_name))
}

/// Можно ли создать файл в этой папке. Ответ запоминается на время работы.
///
/// Права в `metadata` тут не помогут: при App Translocation и при запуске из
/// образа .dmg они выглядят обычными, а том смонтирован только для чтения.
/// Узнать это можно одним способом — попробовать.
#[cfg(target_os = "macos")]
fn macos_dir_accepts_files(dir: &Path) -> bool {
    static KNOWN: OnceLock<StdMutex<HashMap<PathBuf, bool>>> = OnceLock::new();
    let known = KNOWN.get_or_init(|| StdMutex::new(HashMap::new()));
    if let Some(answer) = known.lock().unwrap().get(dir) {
        return *answer;
    }
    let probe = dir.join(format!(".signum-write-probe-{}", std::process::id()));
    let writable = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    let _ = fs::remove_file(&probe);
    known.lock().unwrap().insert(dir.to_path_buf(), writable);
    writable
}

/// Go-движок (v4) с подписчиком `sapsigner` рядом — где бы ни лежал Signum.
///
/// Подписчик должен лежать в одной папке с движком. Обычно его докладывает
/// туда `ensure_macos_sap_sidecar`, но папка движка — внутри Signum.app, а
/// писать туда можно не всегда:
///
/// * приложение запущено прямо из «Загрузок» или из образа .dmg — macOS
///   показывает его из копии только для чтения (App Translocation, путь вида
///   `/private/var/folders/…/AppTranslocation/…/Signum.app`);
/// * приложение поставил администратор, а работает обычный пользователь.
///
/// В отчётах это выглядело как `sapsigner copy failed`, а человеку вход
/// отвечал «не найден компонент подписи… переустановите Signum» — хотя
/// переустановка тут ничего не меняла. В таких случаях движок и подписчик
/// раскладываются в Application Support и запускаются оттуда — так же, как
/// живёт движок, приехавший обновлением по воздуху.
#[cfg(target_os = "macos")]
fn macos_go_engine_ready(app: &AppHandle, engine: String) -> String {
    if engine.is_empty() || is_cpp_ipatool(&engine) {
        return engine;
    }
    let engine_path = PathBuf::from(&engine);
    let Some(engine_dir) = engine_path.parent().filter(|dir| dir.is_dir()) else {
        return engine;
    };
    if !engine_path.is_file()
        || sap_helper_path_for(&engine).is_some()
        || macos_dir_accepts_files(engine_dir)
    {
        // Подписчик уже рядом либо его можно положить рядом — прежний путь.
        return engine;
    }
    let Some(signer) = resolve_store_sap_helper(app) else {
        ipatool_debug_log(&format!(
            "sap signer is not bundled; {engine} stays where it is"
        ));
        return engine;
    };
    let Ok(tools_dir) = managed_tools_dir(app) else {
        return engine;
    };
    let leaf = engine_dir
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "v4".into());
    let stage_dir = tools_dir.join("staged").join(leaf);
    match stage_engine_with_signer(&engine_path, Path::new(&signer), &stage_dir) {
        Ok(staged) => {
            ipatool_debug_log(&format!(
                "engine folder is read-only ({}); running {} with its signer",
                engine_dir.display(),
                staged.display()
            ));
            staged.to_string_lossy().into_owned()
        }
        Err(error) => {
            ipatool_debug_log(&format!(
                "could not stage the engine out of a read-only folder: {error}"
            ));
            engine
        }
    }
}

/// Unicorn 2.1.4 (v4 engine) normally downloads libunicorn.dll from GitHub,
/// which is slow/unreachable from some networks (seen: i/o timeout on
/// release-assets.githubusercontent.com). The DLL ships next to ipatool.exe
/// in the bundle, so pre-stage the exact cache layout upstream uses:
/// %LOCALAPPDATA%\ipatool\unicorn\2.1.4\<sha256>\libunicorn.dll.
/// The folder name must be the SHA-256 of the DLL contents.
#[cfg(target_os = "windows")]
const V4_UNICORN_VERSION: &str = "2.1.4";
#[cfg(target_os = "windows")]
const V4_UNICORN_SHA256: &str = "d8f9a89222ffa74493a1d47090e17f8e1db8ac171a3128c6a76a4ea09de11469";

#[cfg(target_os = "windows")]
fn ensure_v4_unicorn_cache(app: &AppHandle) {
    if let Some(cache) = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|local| {
            local
                .join("ipatool")
                .join("unicorn")
                .join(V4_UNICORN_VERSION)
                .join(V4_UNICORN_SHA256)
                .join("libunicorn.dll")
        })
    {
        if sha256_file_hex(&cache)
            .map(|actual| actual.eq_ignore_ascii_case(V4_UNICORN_SHA256))
            .unwrap_or(false)
        {
            return;
        }
        let source = app
            .path()
            .resolve(
                "resources/apple-tools/windows-x64/v4/libunicorn.dll",
                BaseDirectory::Resource,
            )
            .ok()
            .filter(|path| path.is_file())
            .or_else(|| {
                dev_candidate("apple-tools/windows-x64/v4/libunicorn.dll").filter(|p| p.is_file())
            });
        let Some(source) = source else {
            ipatool_debug_log("v4 unicorn: bundled libunicorn.dll not found");
            return;
        };
        if let Some(parent) = cache.parent() {
            if fs::create_dir_all(parent).is_ok() && fs::copy(&source, &cache).is_ok() {
                ipatool_debug_log("v4 unicorn: staged local runtime cache");
            }
        }
        unblock_windows_file(&cache);
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_v4_unicorn_cache(_app: &AppHandle) {}

/// ipatool-cpp v3 starts its companion as the bare name `anisette.exe`. With
/// managed updates the binary gets a content-hashed file name like
/// `anisette-b1151e3fc1b550b1.exe`, so the lookup fails with
/// "anisette exited with code 1 / не является внутренней или внешней командой".
/// Guarantee the plain name next to the ipatool binary being spawned.
#[cfg(target_os = "windows")]
fn ensure_anisette_basename(ipatool: &str, anisette: Option<&str>) {
    // cpp needs no anisette companion at all.
    if is_cpp_ipatool(ipatool) {
        return;
    }
    let Some(anisette) = anisette.map(str::trim).filter(|p| !p.is_empty()) else {
        return;
    };
    let source = windows_plain_path(Path::new(anisette));
    if !source.is_file() {
        return;
    }
    let Some(dir) = Path::new(ipatool).parent().filter(|p| p.is_dir()) else {
        return;
    };
    let dest = dir.join("anisette.exe");
    if dest.is_file() {
        unblock_windows_file(&dest);
        return;
    }
    if source != dest && fs::copy(&source, &dest).is_ok() {
        ipatool_debug_log(&format!("copied anisette.exe -> {}", dest.display()));
    }
    unblock_windows_file(&dest);
}

#[cfg(not(target_os = "windows"))]
fn ensure_anisette_basename(_ipatool: &str, _anisette: Option<&str>) {}

fn is_transient_ipatool_timeout(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if lower.contains("connection reset")
        || lower.contains("broken pipe")
        || lower.contains("unexpected eof")
        || lower.contains("failed to write file")
        || lower.contains("cannot remove")
        || lower.contains(".ipa.tmp")
        || lower.contains("minizip: failed to open output ipa for append")
    {
        return true;
    }
    (lower.contains("download error") || lower.contains("http request failed"))
        && (lower.contains("timeout")
            || lower.contains("timed out")
            || lower.contains("timeout was reached"))
}

fn resolve_tool_paths(app: &AppHandle) -> Result<ToolPaths, String> {
    remember_bundled_tools_root(app);
    let chosen = load_settings(app)
        .map(|settings| normalize_ipatool_engine(&settings.ipatool_engine))
        .unwrap_or_else(|_| default_ipatool_engine());
    // On "auto" follow the engine that actually wrote the session: v2 (Go
    // FileBackend) keeps it in ~/.ipatool/keyring/account, v3 in
    // ~/.ipatool/account. Reading with the other engine reports
    // "failed to get account" even though the login succeeded.
    let active_engine = active_ipatool_engine(app);
    let engine = if chosen == "auto" {
        active_engine.unwrap_or(chosen)
    } else {
        chosen
    };
    #[cfg(target_os = "windows")]
    let paths = {
        let v5 = engine_binary_or_empty(
            app,
            "ipatool-v5",
            "resources/apple-tools/windows-x64/v5/ipatool.exe",
            "apple-tools/windows-x64/v5/ipatool.exe",
        );
        let v4 = engine_binary_or_empty(
            app,
            "ipatool-v4",
            "resources/apple-tools/windows-x64/v4/ipatool.exe",
            "apple-tools/windows-x64/v4/ipatool.exe",
        );
        // Путь движка v2 больше не выбирается, но по нему находится папка —
        // источник, откуда спутники (sapsigner, kbsync, assets.bin, обе
        // библиотеки Unicorn) раскладываются рядом с рабочим движком.
        let sidecar_anchor = managed_or(
            app,
            "ipatool-v2",
            "resources/apple-tools/windows-x64/v2/ipatool.exe",
            "apple-tools/windows-x64/v2/ipatool.exe",
            "ipatool.exe",
        );
        ensure_windows_sap_sidecars(&sidecar_anchor);
        // v4 finds its Unicorn runtime in the user cache instead of downloading
        // it from GitHub, which is unreachable from some networks.
        ensure_v4_unicorn_cache(app);
        let ipatool = match engine.as_str() {
            "v4" if tool_available(&v4) => v4.clone(),
            "v5" | "v6" if tool_available(&v5) => v5.clone(),
            // v5 (ipatool-cpp) несёт подпись SAP внутри себя и не требует
            // спутников; v4 подписывает локально. Больше выбирать не из чего.
            "auto" if tool_available(&v5) => v5.clone(),
            "auto" if tool_available(&v4) => v4.clone(),
            // Выбранный движок отсутствует — тот же порядок.
            _ if tool_available(&v5) => v5.clone(),
            _ => v4.clone(),
        };
        // Any engine path here may run outside the login flow (catalog probes,
        // downloads). v3 invokes its companion as the bare name `anisette.exe`,
        // so guarantee that name next to the binary about to be spawned.
        let anisette_path = managed_or(
            app,
            "anisette",
            "resources/apple-tools/windows-x64/v3-legacy/anisette.exe",
            "apple-tools/windows-x64/v3-legacy/anisette.exe",
            "anisette.exe",
        );
        ensure_anisette_basename(&ipatool, Some(&anisette_path));
        // Движок v5 порождает kbsync.exe рядом с собой, а тому нужны
        // libunicorn.dll, ucworker.dll и assets.bin. В установщике они едут
        // рядом, но обновление по воздуху приносит только бинари — тогда
        // библиотеки надо доложить, иначе kbsync падает с системной ошибкой
        // «не обнаружила libunicorn.dll». Здесь, а не во входе: движок
        // запускают и для каталога, и для загрузок, вход при этом не нужен.
        ensure_windows_kbsync_sidecars(&ipatool);
        log_engine_environment(&ipatool);
        ToolPaths {
            ipatool,
            anisette: {
                tool_available(&anisette_path).then_some(anisette_path)
            },
            ideviceinfo: bundled_or(
                app,
                "resources/apple-tools/windows-x64/support/ideviceinfo.exe",
                "apple-tools/windows-x64/support/ideviceinfo.exe",
                "ideviceinfo.exe",
            ),
            ideviceinstaller: bundled_or(
                app,
        "resources/apple-tools/windows-x64/support/ideviceinstaller.exe",
        "apple-tools/windows-x64/support/ideviceinstaller.exe",
        "ideviceinstaller.exe",
    ),
            idevicepair: bundled_or(
                app,
                "resources/apple-tools/windows-x64/support/idevicepair.exe",
                "apple-tools/windows-x64/support/idevicepair.exe",
                "idevicepair.exe",
            ),
            idevice_id: Some(bundled_or(
                app,
    "resources/apple-tools/windows-x64/support/idevice_id.exe",
                "apple-tools/windows-x64/support/idevice_id.exe",
                "idevice_id.exe",
            )).filter(|path| tool_available(path)),
            zsign: Some(bundled_or(
                app,
                "resources/apple-tools/windows-x64/support/zsign.exe",
                "apple-tools/windows-x64/support/zsign.exe",
                "zsign.exe",
            )),
        }
    };
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let paths = {
        let v2 = managed_or(
            app,
            "ipatool-v2",
            "resources/apple-tools/macos-arm64/v2/ipatool",
            "apple-tools/macos-arm64/v2/ipatool",
            "ipatool",
        );
        let v3 = managed_or(
            app,
            "ipatool-v3",
            "resources/apple-tools/macos-arm64/v3-legacy/ipatool",
            "apple-tools/macos-arm64/v3-legacy/ipatool",
            "ipatool",
        );
        let v4 = engine_binary_or_empty(
            app,
            "ipatool-v4",
            "resources/apple-tools/macos-arm64/v4/ipatool",
            "apple-tools/macos-arm64/v4/ipatool",
        );
        let v5 = engine_binary_or_empty(
            app,
            "ipatool-v5",
            "resources/apple-tools/macos-arm64/v5/ipatool",
            "apple-tools/macos-arm64/v5/ipatool",
        );
        let ipatool = match engine.as_str() {
            "v4" if tool_available(&v4) => v4,
            "v5" | "v6" if tool_available(&v5) => v5,
            "auto" if tool_available(&v5) => v5,
            "auto" if tool_available(&v4) => v4,
            _ if tool_available(&v5) => v5,
            _ => v4,
        };
        // Из Signum.app, открытого только для чтения, Go-движок переезжает в
        // Application Support вместе с подписчиком (см. macos_go_engine_ready).
        let ipatool = macos_go_engine_ready(app, ipatool);
        let paths = ToolPaths {
            ipatool,
            anisette: None,
            ideviceinfo: macos_device_tool(app, "ideviceinfo"),
            ideviceinstaller: macos_device_tool(app, "ideviceinstaller"),
            idevicepair: macos_device_tool(app, "idevicepair"),
    idevice_id: Some(macos_device_tool(app, "idevice_id"))
                .filter(|path| tool_available(path)),
            // Свой zsign рядом с приложением: раньше искали только в PATH, где у
            // обычного человека его нет, и подпись своим сертификатом на маке
            // просто не работала.
            zsign: Some(macos_bundled_zsign()),
        };
        ensure_macos_idevice_tools(&paths);
        paths
    };
    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    let paths = {
        let v2 = managed_or(
            app,
            "ipatool-v2",
            "resources/apple-tools/macos-x64/v2/ipatool",
            "apple-tools/macos-x64/v2/ipatool",
            "ipatool",
        );
        let v3 = managed_or(
            app,
            "ipatool-v3",
            "resources/apple-tools/macos-x64/v3-legacy/ipatool",
            "apple-tools/macos-x64/v3-legacy/ipatool",
            "ipatool",
        );
        let v4 = engine_binary_or_empty(
            app,
            "ipatool-v4",
            "resources/apple-tools/macos-x64/v4/ipatool",
            "apple-tools/macos-x64/v4/ipatool",
        );
        let v5 = engine_binary_or_empty(
            app,
            "ipatool-v5",
            "resources/apple-tools/macos-x64/v5/ipatool",
            "apple-tools/macos-x64/v5/ipatool",
        );
        let ipatool = match engine.as_str() {
            "v4" if tool_available(&v4) => v4,
            "v5" | "v6" if tool_available(&v5) => v5,
            "auto" if tool_available(&v5) => v5,
            "auto" if tool_available(&v4) => v4,
            _ if tool_available(&v5) => v5,
            _ => v4,
        };
        // Из Signum.app, открытого только для чтения, Go-движок переезжает в
        // Application Support вместе с подписчиком (см. macos_go_engine_ready).
        let ipatool = macos_go_engine_ready(app, ipatool);
        let paths = ToolPaths {
            ipatool,
            anisette: None,
            ideviceinfo: macos_device_tool(app, "ideviceinfo"),
            ideviceinstaller: macos_device_tool(app, "ideviceinstaller"),
            idevicepair: macos_device_tool(app, "idevicepair"),
            idevice_id: Some(macos_device_tool(app, "idevice_id"))
                .filter(|path| tool_available(path)),
            // Свой zsign рядом с приложением: раньше искали только в PATH, где у
            // обычного человека его нет, и подпись своим сертификатом на маке
            // просто не работала.
            zsign: Some(macos_bundled_zsign()),
        };
        ensure_macos_idevice_tools(&paths);
        paths
    };
    // Linux (and other Unix): prefer bundled tools, then PATH (apt/brew installs).
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    let paths = ToolPaths {
        ipatool: linux_ipatool(
            app,
            &engine,
            "resources/linux-arm64/ipatool",
            "linux_arm64/ipatool",
            "resources/linux-arm64/ipatool-v4",
            "linux_arm64/ipatool-v4",
            "resources/linux-arm64/ipatool-v5",
            "linux_arm64/ipatool-v5",
        ),
        anisette: None,
            ideviceinfo: which_or("ideviceinfo"),
            ideviceinstaller: which_or("ideviceinstaller"),
            idevicepair: which_or("idevicepair"),
            idevice_id: Some(bundled_or(app, "resources/linux-arm64/idevice_id", "linux_arm64/idevice_id", "idevice_id"))
                .filter(|path| tool_available(path)),
        zsign: Some(bundled_or(
            app,
            "resources/linux-arm64/zsign",
            "linux_arm64/zsign",
            "zsign",
        )),
    };
    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    let paths = ToolPaths {
        ipatool: linux_ipatool(
            app,
            &engine,
            "resources/linux-x64/ipatool",
            "linux_x64/ipatool",
            "resources/linux-x64/ipatool-v4",
            "linux_x64/ipatool-v4",
            "resources/linux-x64/ipatool-v5",
            "linux_x64/ipatool-v5",
        ),
        anisette: None,
        ideviceinfo: bundled_or(app, "resources/linux-x64/ideviceinfo", "linux_x64/ideviceinfo", "ideviceinfo"),
        ideviceinstaller: bundled_or(app, "resources/linux-x64/ideviceinstaller", "linux_x64/ideviceinstaller", "ideviceinstaller"),
        idevicepair: which_or("idevicepair"),
        idevice_id: Some(bundled_or(app, "resources/linux-x64/idevice_id", "linux_x64/idevice_id", "idevice_id"))
            .filter(|path| tool_available(path)),
        zsign: Some(bundled_or(
            app,
            "resources/linux-x64/zsign",
            "linux_x64/zsign",
            "zsign",
        )),
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let paths = ToolPaths {
        ipatool: "ipatool".into(),
        anisette: None,
        ideviceinfo: "ideviceinfo".into(),
        ideviceinstaller: "ideviceinstaller".into(),
        idevicepair: "idevicepair".into(),
        idevice_id: Some("idevice_id".into()),
        zsign: Some("zsign".into()),
    };

    // A bundled binary that arrived without its executable bit spawns with
    // EACCES; repairing it here covers every engine, not just the device tools.
    #[cfg(target_os = "macos")]
    if !is_cpp_ipatool(&paths.ipatool) {
        if let Some(source) = managed_tool_path(app, "sapsigner") {
            if let Some(parent) = Path::new(&paths.ipatool).parent() {
                let destination = parent.join("sapsigner");
                if source != destination && !destination.is_file() {
                    let _ = fs::copy(&source, &destination);
                }
                if destination.is_file() {
                    ensure_unix_executable(&destination.to_string_lossy());
                }
            }
        }
    }
    #[cfg(unix)]
    {
        ensure_unix_executable(&paths.ipatool);
        if let Some(helper) = sap_helper_path_for(&paths.ipatool) {
            ensure_unix_executable(&helper);
        }
        if let Some(zsign) = paths.zsign.as_deref() {
            ensure_unix_executable(zsign);
        }
        for tool in [&paths.ideviceinfo, &paths.ideviceinstaller, &paths.idevicepair] {
            ensure_unix_executable(tool);
        }
        if let Some(tool) = paths.idevice_id.as_deref() {
            ensure_unix_executable(tool);
        }
    }

    // Absolute path or name resolved via PATH (`which` style on Unix).
    let ipatool_ok = Path::new(&paths.ipatool).is_file()
        || (!paths.ipatool.contains('/')
            && !paths.ipatool.contains('\\')
            && command_on_path(&paths.ipatool));
    if !ipatool_ok {
        return Err(if cfg!(target_os = "linux") {
            "Не найден ipatool. Положите бинарь в resources/linux-x64/ipatool \
                 или установите ipatool в PATH. Переустановите Signum при необходимости."
                .into()
        } else {
            "Не хватает внутренних файлов Signum. Переустановите приложение и повторите попытку."
                .into()
        });
    }
    if let Some(anisette) = paths.anisette.as_deref() {
        if !Path::new(anisette).is_file() {
            return Err(
                "Не удалось подготовить вход в Apple ID. Переустановите Signum и повторите попытку."
                    .into(),
            );
        }
    }
    #[cfg(target_os = "macos")]
    ensure_macos_kbsync_sidecars(&paths.ipatool)?;
    Ok(paths)
}

/// Anisette helper used by the native App Store client. Independent of the
/// selected ipatool engine — Windows catalog restore needs these headers even
/// when GSA login went through ipatool-cpp.
fn resolve_anisette_exe(app: &AppHandle) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let path = managed_or(
            app,
            "anisette",
            "resources/apple-tools/windows-x64/v3-legacy/anisette.exe",
            "apple-tools/windows-x64/v3-legacy/anisette.exe",
            "anisette.exe",
        );
        if tool_available(&path) {
            Some(path)
        } else {
            None
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        None
    }
}

/// Helper that lets the built-in `apple_store` client SAP-sign version-label
/// requests. Windows uses the anisette exe (the sapsigner sits next to it); macOS
/// uses the native CommerceKit `sapsigner` bundled in apple-tools — passing it
/// here makes `helper_available`/`version_display` work so labels show `v___`
/// instead of the raw externalVersionId.
fn resolve_store_sap_helper(app: &AppHandle) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(path) = managed_tool_path(app, "sapsigner") {
            let path = path.to_string_lossy().into_owned();
            ensure_unix_executable(&path);
            return Some(path);
        }
        for arch in macos_tool_arches() {
            let resource = format!("resources/apple-tools/{arch}/v2/sapsigner");
            let dev = format!("apple-tools/{arch}/v2/sapsigner");
            let path = bundled_or(app, &resource, &dev, "sapsigner");
            if Path::new(&path).is_file() {
                // The bundle is copied as-is; a signer that arrived without the
                // executable bit spawns with EACCES, Store requests go out
                // unsigned and every version label comes back as a build number.
                ensure_unix_executable(&path);
                return Some(path);
            }
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(target_os = "linux")]
        {
            if let Some(custom) = std::env::var_os("IPATOOL_SAP_HELPER") {
                let path = PathBuf::from(custom);
                if path.is_file() {
                    ensure_unix_executable(&path.to_string_lossy());
                    return Some(path.to_string_lossy().into_owned());
                }
            }
            let names = if cfg!(target_arch = "aarch64") {
                ["resources/linux-arm64/sapsigner", "linux_arm64/sapsigner", "sapsigner"]
            } else {
                ["resources/linux-x64/sapsigner", "linux_x64/sapsigner", "sapsigner"]
            };
            for name in names {
                let path = bundled_or(app, name, name, "sapsigner");
                if Path::new(&path).is_file() {
                    ensure_unix_executable(&path);
                    return Some(path);
                }
            }
            for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
                let path = dir.join("sapsigner");
                if path.is_file() {
                    ensure_unix_executable(&path.to_string_lossy());
                    return Some(path.to_string_lossy().into_owned());
                }
            }
            return None;
        }
        resolve_anisette_exe(app)
    }
}

/// True when a tool path is a real binary or resolvable on PATH.
fn distinct_login_tool_paths(candidates: Vec<(String, ToolPaths)>) -> Vec<(String, ToolPaths)> {
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|(engine, tools)| {
            tool_available(&tools.ipatool) && seen.insert((engine.clone(), tools.ipatool.clone()))
        })
        .collect()
}

/// Движок, сохранённый в настройках («auto» | «v4» | «v5»).
fn preferred_ipatool_engine(app: &AppHandle) -> String {
    load_settings(app)
        .map(|settings| normalize_ipatool_engine(&settings.ipatool_engine))
        .unwrap_or_else(|_| default_ipatool_engine())
}

/// Explicit choices lead. Auto starts with the engine that owns the session,
/// then falls back through the platform order (v5, v4).
fn engine_to_try_first(chosen: &str, session_owner: Option<&str>) -> Option<String> {
    match normalize_ipatool_engine(chosen).as_str() {
        "auto" => session_owner.map(normalize_ipatool_engine).filter(|engine| engine != "auto"),
        explicit => Some(explicit.to_string()),
    }
}

/// Reorder engine names for a login attempt. The incoming order is the
/// platform fallback chain. Explicit selection or the session owner moves
/// to front without dropping any fallback candidates.
fn promote_login_engine(
    order: Vec<String>,
    chosen: &str,
    session_owner: Option<&str>,
) -> Vec<String> {
    let mut order = order;
    let Some(first) = engine_to_try_first(chosen, session_owner) else {
        return order;
    };
    if let Some(index) = order.iter().position(|name| name == &first) {
        let promoted = order.remove(index);
        order.insert(0, promoted);
    }
    order
}

fn order_login_candidates(
    app: &AppHandle,
    candidates: Vec<(String, ToolPaths)>,
) -> Vec<(String, ToolPaths)> {
    let chosen = preferred_ipatool_engine(app);
    let owner = active_ipatool_engine(app);
    let chosen = if cfg!(target_os = "macos") && chosen == "auto" {
        owner.as_deref().unwrap_or("auto")
    } else {
        chosen.as_str()
    };
    let names = candidates
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let wanted = promote_login_engine(names, chosen, owner.as_deref());
    let mut ordered = candidates;
    ordered.sort_by_key(|(name, _)| {
        wanted
            .iter()
            .position(|target| target == name)
            .unwrap_or(usize::MAX)
    });
    ordered
}

fn login_tool_paths(app: &AppHandle) -> Vec<(String, ToolPaths)> {
    #[cfg(target_os = "windows")]
    {
        let base = match resolve_tool_paths(app) {
            Ok(paths) => paths,
            Err(_) => return Vec::new(),
        };
        let v4 = ToolPaths {
            ipatool: engine_binary_or_empty(
                app,
                "ipatool-v4",
                "resources/apple-tools/windows-x64/v4/ipatool.exe",
                "apple-tools/windows-x64/v4/ipatool.exe",
            ),
            // v4 signs Apple actions itself; it needs no anisette sidecar.
            anisette: None,
            ..base.clone()
        };
        let v5 = ToolPaths {
            ipatool: engine_binary_or_empty(
                app,
                "ipatool-v5",
                "resources/apple-tools/windows-x64/v5/ipatool.exe",
                "apple-tools/windows-x64/v5/ipatool.exe",
            ),
            // ipatool-cpp carries CoreFP/storeagent inside the binary.
            anisette: None,
            ..base
        };
        ensure_v4_unicorn_cache(app);
        let order = vec![("v5".into(), v5), ("v4".into(), v4)];
        return order_login_candidates(app, distinct_login_tool_paths(order));
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let base = match resolve_tool_paths(app) {
            Ok(paths) => paths,
            Err(_) => return Vec::new(),
        };
        let v4 = ToolPaths {
            // Тот же путь, что выберет resolve_tool_paths: иначе вход и
            // последующие операции запускали бы движок из разных папок.
            ipatool: macos_go_engine_ready(
                app,
                engine_binary_or_empty(
                    app,
                    "ipatool-v4",
                    "resources/apple-tools/macos-arm64/v4/ipatool",
                    "apple-tools/macos-arm64/v4/ipatool",
                ),
            ),
            anisette: None,
            ..base.clone()
        };
        let v5 = ToolPaths {
            ipatool: engine_binary_or_empty(
                app,
                "ipatool-v5",
                "resources/apple-tools/macos-arm64/v5/ipatool",
                "apple-tools/macos-arm64/v5/ipatool",
            ),
            anisette: None,
            ..base
        };
        let order = vec![("v5".into(), v5), ("v4".into(), v4)];
        return order_login_candidates(
            app,
            distinct_login_tool_paths(order),
        );
    }

    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        let base = match resolve_tool_paths(app) {
            Ok(paths) => paths,
            Err(_) => return Vec::new(),
        };
        let v4 = ToolPaths {
            // Тот же путь, что выберет resolve_tool_paths: иначе вход и
            // последующие операции запускали бы движок из разных папок.
            ipatool: macos_go_engine_ready(
                app,
                engine_binary_or_empty(
                    app,
                    "ipatool-v4",
                    "resources/apple-tools/macos-x64/v4/ipatool",
                    "apple-tools/macos-x64/v4/ipatool",
                ),
            ),
            anisette: None,
            ..base.clone()
        };
        let v5 = ToolPaths {
            ipatool: engine_binary_or_empty(
                app,
                "ipatool-v5",
                "resources/apple-tools/macos-x64/v5/ipatool",
                "apple-tools/macos-x64/v5/ipatool",
            ),
            anisette: None,
            ..base
        };
        let order = vec![("v5".into(), v5), ("v4".into(), v4)];
        return order_login_candidates(
            app,
            distinct_login_tool_paths(order),
        );
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let base = resolve_tool_paths(app).ok();
        let Some(base) = base else { return Vec::new(); };
        let mut out = Vec::new();
        let mut add = |name: &str, path: String| {
            if tool_available(&path) {
                out.push((name.to_string(), ToolPaths { ipatool: path, ..base.clone() }));
            }
        };
        let v5 = if cfg!(target_arch = "aarch64") {
            engine_binary_or_empty(app, "ipatool-v5", "resources/linux-arm64/ipatool-v5", "linux_arm64/ipatool-v5")
        } else {
            engine_binary_or_empty(app, "ipatool-v5", "resources/linux-x64/ipatool-v5", "linux_x64/ipatool-v5")
        };
        let v4 = if cfg!(target_arch = "aarch64") {
            engine_binary_or_empty(app, "ipatool-v4", "resources/linux-arm64/ipatool-v4", "linux_arm64/ipatool-v4")
        } else {
            engine_binary_or_empty(app, "ipatool-v4", "resources/linux-x64/ipatool-v4", "linux_x64/ipatool-v4")
        };
        let v2 = base.ipatool.clone();
        add("v5", v5);
        add("v4", v4);
        add("auto", v2);
        distinct_login_tool_paths(out)
    }
}

/// ipatool for Linux. v5 (ipatool-cpp) has the SAP signer compiled in and wins
/// whenever present, then v4 which signs Store requests itself; the sapfix v2
/// build stays as the fallback for installs that have not picked them up yet.
#[cfg(target_os = "linux")]
fn linux_ipatool(
    app: &AppHandle,
    engine: &str,
    v2_resource: &str,
    v2_dev: &str,
    v4_resource: &str,
    v4_dev: &str,
    v5_resource: &str,
    v5_dev: &str,
) -> String {
    let v2 = managed_or(app, "ipatool-v2", v2_resource, v2_dev, "ipatool");
    let v4 = engine_binary_or_empty(app, "ipatool-v4", v4_resource, v4_dev);
    let v5 = engine_binary_or_empty(app, "ipatool-v5", v5_resource, v5_dev);
    let chosen = match engine {
        "v2" if tool_available(&v2) => v2,
        "v4" if tool_available(&v4) => v4,
        "v5" | "v6" if tool_available(&v5) => v5,
        _ if tool_available(&v5) => v5,
        _ if tool_available(&v4) => v4,
        _ if tool_available(&v2) => v2,
        _ => which_or("ipatool"),
    };
    // Движок, приехавший обновлением по воздуху, приходит без права на запуск,
    // а tool_available() проверяет только существование файла. Без этой строки
    // выбранный движок падал с отказом в доступе, и выглядело это как будто
    // его нет вовсе.
    ensure_unix_executable(&chosen);
    chosen
}

fn tool_available(path: &str) -> bool {
    // An empty path means "engine not installed" (see engine_binary_or_empty).
    // Without this guard `Path::new("")` and a PATH lookup for "" could be
    // misread as an available tool.
    if path.trim().is_empty() {
        return false;
    }
    let p = Path::new(path);
    if p.is_file() {
        return true;
    }
    // Bare command name (no slashes) → check PATH.
    if !path.contains('/') && !path.contains('\\') {
        return command_on_path(path);
    }
    false
}

fn missing_idevice_tools_message() -> String {
    "Не удалось подготовить работу с iPhone. Переустановите Signum, подключите устройство кабелем и подтвердите доверие."
        .into()
}

fn require_idevice_tools(tools: &ToolPaths) -> Result<(), String> {
    if tool_available(&tools.ideviceinfo)
        && tool_available(&tools.ideviceinstaller)
        && tool_available(&tools.idevicepair)
    {
        return Ok(());
    }
    Err(missing_idevice_tools_message())
}

/// True when text looks like anisette / Apple device-auth helper failure.
fn is_anisette_failure_text(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("anisette")
        || lowered.contains("x-apple-i-md")
        || (lowered.contains("adi")
            && (lowered.contains("fail")
                || lowered.contains("error")
                || lowered.contains("invalid")
                || lowered.contains("not found")))
}

/// Remove Windows "downloaded file" mark so helper EXEs are not blocked by SmartScreen.
#[cfg(target_os = "windows")]
fn unblock_windows_file(path: &Path) {
    if !path.is_file() {
        return;
    }
    // Zone.Identifier ADS: "file.exe:Zone.Identifier"
    let mut ads = path.as_os_str().to_os_string();
    ads.push(":Zone.Identifier");
    let _ = fs::remove_file(Path::new(&ads));
}

#[cfg(not(target_os = "windows"))]
fn unblock_windows_file(_path: &Path) {}

/// Start Apple Mobile Device Service if present but stopped (helps anisette on Windows).
#[cfg(target_os = "windows")]
fn ensure_apple_mobile_device_service() {
    let _ = StdCommand::new("sc")
        .args(["start", "Apple Mobile Device Service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .status();
}

#[cfg(not(target_os = "windows"))]
fn ensure_apple_mobile_device_service() {}

/// Whether Apple's usbmuxd (Apple Mobile Device Service) answers. Without it
/// libimobiledevice only says "No device found" even with the iPhone plugged in.
#[cfg(target_os = "windows")]
fn usbmuxd_reachable() -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 27015));
    let probe = || std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(800)).is_ok();
    if probe() {
        return true;
    }
    ensure_apple_mobile_device_service();
    std::thread::sleep(Duration::from_millis(1500));
    probe()
}

#[cfg(not(target_os = "windows"))]
fn usbmuxd_reachable() -> bool {
    true
}

fn usbmuxd_missing_message() -> String {
    "Windows не видит iPhone: не работает служба Apple Mobile Device.\n\n\
     Поставьте драйвер («Настройки» → «Драйвер Apple для iPhone» → «Переустановить») или «Устройства Apple» \
     (Apple Devices) из Microsoft Store, перезагрузите компьютер и переподключите iPhone. \
     Если «Устройства Apple» уже стоят — откройте их один раз."
        .into()
}

/// Best-effort clear of broken Apple ADI cache used by anisette helpers.
#[cfg(target_os = "windows")]
fn clear_stale_apple_adi_cache() {
    let candidates = [
        std::env::var_os("LOCALAPPDATA").map(|p| {
            PathBuf::from(p)
                .join("Apple Computer")
                .join("iTunes")
                .join("adi")
        }),
        std::env::var_os("APPDATA").map(|p| {
            PathBuf::from(p)
                .join("Apple Computer")
                .join("iTunes")
                .join("adi")
        }),
        Some(PathBuf::from(r"C:\ProgramData\Apple Computer\iTunes\adi")),
    ];
    for dir in candidates.into_iter().flatten() {
        if dir.is_dir() {
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn clear_stale_apple_adi_cache() {}

/// Unblock helpers, wake Apple service, pre-run anisette once before login.
async fn prepare_apple_auth_helpers(tools: &ToolPaths) -> Result<(), String> {
    unblock_windows_file(Path::new(&tools.ipatool));
    // macOS: the Go engines sign Store actions with a `sapsigner` that has to sit
    // next to the binary. An engine delivered over the air lands in Application
    // Support, so the signer is copied there — and when that copy silently failed
    // the login went out unsigned and Apple never sent a 2FA code at all. The
    // user saw "the code never arrives" with nothing in the interface to explain
    // it, so make the missing signer an error instead of a debug-log line.
    // Движок v5 порождает kbsync соседним процессом — кладём рядом то, без чего
    // он не запустится. Иначе снятые с продажи приложения молча перестают
    // качаться, а в логе видно только «не удалось получить список версий».
    #[cfg(target_os = "macos")]
    ensure_macos_kbsync_sidecars(&tools.ipatool)?;
    #[cfg(target_os = "macos")]
    log_engine_environment(&tools.ipatool);

    #[cfg(target_os = "macos")]
    if !is_cpp_ipatool(&tools.ipatool) {
        ensure_macos_sap_sidecar(&tools.ipatool);
        if sap_helper_path_for(&tools.ipatool).is_none() {
            ipatool_debug_log(&format!(
                "sap signer missing next to {} — refusing an unsigned login",
                tools.ipatool
            ));
            return Err(
                "Не найден компонент подписи запросов Apple (sapsigner).\n\n\
                 Без него Apple не присылает код подтверждения. \
                 Переустановите Signum — файл входит в поставку."
                    .into(),
            );
        }
    }
    if let Some(anisette) = tools.anisette.as_deref() {
        unblock_windows_file(Path::new(anisette));
    }
    ensure_apple_mobile_device_service();

    #[cfg(target_os = "windows")]
    let sap_ready = ipatool_signs_store_requests(&tools.ipatool);
    #[cfg(target_os = "windows")]
    if let Some(anisette) = tools.anisette.as_deref() {
        // Warm-up: anisette must emit Apple device headers. Retry once after a short pause.
        // Local FairPlay SAP login does not require anisette — do not skip v2 if it flakes.
        for attempt in 0..2u8 {
            match run_process_raw_error(anisette, &[], Duration::from_secs(25), None).await {
                Ok(out) if out.to_ascii_uppercase().contains("X-APPLE-I-MD") => {
                    ipatool_debug_log(&format!("anisette warmup ok attempt={attempt}"));
                    return Ok(());
                }
                Ok(out) | Err(out) if attempt == 0 => {
                    ipatool_debug_log(&format!(
                        "anisette warmup retry after {}",
                        technical_excerpt(&out, 400)
                    ));
                    clear_stale_apple_adi_cache();
                    ensure_apple_mobile_device_service();
                    tokio::time::sleep(Duration::from_millis(800)).await;
                }
                Ok(out) | Err(out) => {
                    if sap_ready {
                        ipatool_debug_log(&format!(
                            "anisette warmup failed; continuing with SAP helper {}",
                            technical_excerpt(&out, 400)
                        ));
                        break;
                    }
                    return Err(
                        "Не удалось подготовить вход в Apple ID. Полностью закройте Signum и войдите снова. \
                         Если не поможет — переустановите приложение."
                            .into(),
                    );
                }
            }
        }
    }
    Ok(())
}

fn default_signum_exports_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .ok_or_else(|| "Не удалось определить домашнюю папку".to_string())?;
    Ok(home.join("Downloads").join("Signum"))
}

fn configured_exports_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let settings = load_settings(app)?;
    let path = settings.exports_dir.trim();
    if path.is_empty() {
        default_signum_exports_dir()
    } else {
        Ok(PathBuf::from(path))
    }
}

fn sanitize_export_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "export".into()
    } else {
        trimmed.into()
    }
}

fn move_ipa_to_exports(
    app: &AppHandle,
    ipa: &Path,
    title: &str,
    version: &str,
) -> Result<PathBuf, String> {
    let exports = configured_exports_dir(app)?;
    fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
    let base = sanitize_export_filename(title);
    let ver = sanitize_export_filename(version);
    let filename = if ver.is_empty() || ver == "export" {
        format!("{base}.ipa")
    } else {
        format!("{base}_{ver}.ipa")
    };
    let mut dest = exports.join(&filename);
    if dest.exists() {
        let stamp = Uuid::new_v4();
        dest = exports.join(format!(
            "{}_{stamp}.ipa",
            dest.file_stem().and_then(|v| v.to_str()).unwrap_or(&base)
        ));
    }
    // Temp and Downloads may be on different volumes. Keep the source until
    // the complete copy has been flushed successfully.
    let mut source = fs::File::open(ipa).map_err(|e| e.to_string())?;
    let mut target = fs::OpenOptions::new().write(true).create_new(true)
        .open(&dest).map_err(|e| format!("Не удалось сохранить файл приложения: {e}"))?;
    let copied = std::io::copy(&mut source, &mut target).and_then(|_| target.sync_all());
    drop(target);
    drop(source);
    if let Err(error) = copied {
        let _ = fs::remove_file(&dest);
        return Err(format!("Не удалось сохранить файл приложения: {error}"));
    }
    fs::remove_file(ipa).map_err(|e| format!("Файл сохранён, но не удалось удалить временную копию: {e}"))?;
    Ok(dest)
}

fn default_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let downloads = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("downloads");
    fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let exports = default_signum_exports_dir()?;
    fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
    Ok(AppSettings {
        background_enabled: false,
        proxy_enabled: false,
        proxy_kind: default_proxy_kind(),
        proxy_host: String::new(),
        proxy_port: 0,
        proxy_user: String::new(),
        proxy_password: String::new(),
        downloads_dir: downloads.to_string_lossy().into_owned(),
        exports_dir: exports.to_string_lossy().into_owned(),
        theme: "dark".into(),
        auto_update: true,
        auto_purchase_new_apps: false,
        lite_design: false,
        detailed_errors: false,
        auto_diag: true,
        lite_static_loaders: true,
        lite_reduce_motion: true,
        lite_simple_scrolling: true,
        lite_disable_blur: true,
        lite_disable_shadows: true,
        lite_flat_backgrounds: true,
        ipatool_keychain_mode: default_ipatool_keychain_mode(),
        ipatool_keychain_passphrase: None,
        selected_device_udid: None,
        selected_device_connection: None,
        keep_history_session: true,
        ipatool_engine: default_ipatool_engine(),
        favorite_app_ids: Vec::new(),
        auto_resign_before_expiry: false,
    })
}

fn normalize_ipatool_keychain_mode(mode: &str) -> String {
    match mode.trim().to_lowercase().as_str() {
        "custom" => "custom".into(),
        _ => "random".into(),
    }
}

fn ipatool_passphrase_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("Signum", "ipatool_keychain_passphrase").map_err(|e| e.to_string())
}

/// Disk backup when Windows Credential Manager is flaky — one random key per machine.
fn ipatool_passphrase_file_path() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(|home| {
        PathBuf::from(home)
            .join(".signum")
            .join("keychain_passphrase")
    })
}

fn is_usable_passphrase(value: &str) -> bool {
    let t = value.trim();
    !t.is_empty() && t != LEGACY_BUILTIN_IPATOOL_KEYCHAIN_PASSPHRASE
}

fn generate_random_passphrase() -> String {
    Uuid::new_v4().to_string()
}

fn read_passphrase_from_file() -> Option<String> {
    let path = ipatool_passphrase_file_path()?;
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim().to_string();
    is_usable_passphrase(&trimmed).then_some(trimmed)
}

fn write_passphrase_to_file(passphrase: &str) -> Result<(), String> {
    let path = ipatool_passphrase_file_path()
        .ok_or_else(|| "Не удалось определить папку для ключа".to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, passphrase.as_bytes()).map_err(|e| e.to_string())
}

fn persist_random_passphrase(passphrase: &str) -> Result<(), String> {
    // File first — survives even if Credential Manager fails.
    write_passphrase_to_file(passphrase)?;
    if let Ok(entry) = ipatool_passphrase_entry() {
        let _ = entry.set_password(passphrase);
    }
    Ok(())
}

fn cache_ipatool_passphrase(value: String) -> String {
    if let Ok(mut guard) = CACHED_IPATOOL_PASSPHRASE.lock() {
        *guard = Some(value.clone());
    }
    value
}

/// Load existing random key or create a new UUID once and persist it.
/// Never uses the legacy hardcoded `signum-ipatool-keychain-v1`.
fn get_or_create_random_passphrase() -> Result<String, String> {
    if let Ok(guard) = CACHED_IPATOOL_PASSPHRASE.lock() {
        if let Some(cached) = guard.as_ref() {
            if is_usable_passphrase(cached) {
                return Ok(cached.clone());
            }
        }
    }

    // 1) Windows Credential Manager / keyring
    if let Ok(entry) = ipatool_passphrase_entry() {
        if let Ok(existing) = entry.get_password() {
            if is_usable_passphrase(&existing) {
                let trimmed = existing.trim().to_string();
                // Keep file in sync if keyring was the only source.
                let _ = write_passphrase_to_file(&trimmed);
                return Ok(cache_ipatool_passphrase(trimmed));
            }
        }
    }

    // 2) File fallback (same key after keyring wipe / UAC issues)
    if let Some(from_file) = read_passphrase_from_file() {
        if let Ok(entry) = ipatool_passphrase_entry() {
            let _ = entry.set_password(&from_file);
        }
        return Ok(cache_ipatool_passphrase(from_file));
    }

    // 3) First start or legacy builtin found → brand-new random key
    let passphrase = generate_random_passphrase();
    persist_random_passphrase(&passphrase)?;
    Ok(cache_ipatool_passphrase(passphrase))
}

fn migrate_ipatool_settings(raw: &str, settings: &mut AppSettings) -> Result<(), String> {
    settings.ipatool_engine = normalize_ipatool_engine(&settings.ipatool_engine);
    remember_proxy(&settings);
    if raw.contains("ipatool_keychain_mode") {
        settings.ipatool_keychain_mode =
            normalize_ipatool_keychain_mode(&settings.ipatool_keychain_mode);
        // Ensure random mode has a real UUID, not the legacy fixed string.
        if settings.ipatool_keychain_mode == "random" {
            let _ = get_or_create_random_passphrase();
        }
        return Ok(());
    }

    let legacy = serde_json::from_str::<JsonValue>(raw).unwrap_or(JsonValue::Null);
    let legacy_custom = json_bool(legacy.get("custom_ipatool_keychain"), false);
    settings.ipatool_keychain_mode = if legacy_custom {
        "custom".into()
    } else {
        let _ = get_or_create_random_passphrase();
        "random".into()
    };
    Ok(())
}

fn strip_utf8_bom(raw: &str) -> &str {
    raw.strip_prefix('\u{feff}').unwrap_or(raw)
}

fn load_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(app)?;
    if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let raw = strip_utf8_bom(&raw);
        let mut settings: AppSettings = serde_json::from_str(raw).map_err(|e| {
            format!(
                "Повреждён файл настроек ({}): {e}",
                path.display()
            )
        })?;
        remember_proxy(&settings);
        if settings.exports_dir.trim().is_empty() {
            settings.exports_dir = default_signum_exports_dir()?.to_string_lossy().into_owned();
        }
        settings.theme = normalize_theme(&settings.theme);
        migrate_ipatool_settings(&raw, &mut settings)?;
        if sanitize_saved_device_udid(&mut settings) {
            let _ = fs::write(&path, serde_json::to_string_pretty(&settings).unwrap_or_default());
        }
        DETAILED_ERRORS_ENABLED.store(settings.detailed_errors, Ordering::Relaxed);
        Ok(settings)
    } else {
        let settings = default_settings(app)?;
        DETAILED_ERRORS_ENABLED.store(settings.detailed_errors, Ordering::Relaxed);
        Ok(settings)
    }
}

fn normalize_theme(theme: &str) -> String {
    match theme.trim().to_lowercase().as_str() {
        "light" => "light".into(),
        "system" => "system".into(),
        _ => "dark".into(),
    }
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    load_settings(&app)
}

#[tauri::command]
fn get_app_version_info() -> AppVersionInfo {
    AppVersionInfo {
        version: APP_VERSION,
        build: APP_BUILD,
        beta: false,
    }
}

#[tauri::command]
fn get_ipatool_random_passphrase(app: AppHandle) -> Result<String, String> {
    let settings = load_settings(&app)?;
    if settings.ipatool_keychain_mode != "random" {
        return Err("Случайный ключ доступен только в режиме «Рандом»".into());
    }
    get_or_create_random_passphrase()
}

#[tauri::command]
fn regenerate_ipatool_random_passphrase(app: AppHandle) -> Result<String, String> {
    let settings = load_settings(&app)?;
    if settings.ipatool_keychain_mode != "random" {
        return Err("Новый случайный ключ можно создать только в режиме «Рандом»".into());
    }
    let passphrase = generate_random_passphrase();
    persist_random_passphrase(&passphrase)?;
    // Old Apple sessions were encrypted with the previous key — drop local store.
    reset_ipatool_local_store();
    Ok(cache_ipatool_passphrase(passphrase))
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let mut settings = settings;
    if settings.exports_dir.trim().is_empty() {
        settings.exports_dir = default_signum_exports_dir()?.to_string_lossy().into_owned();
    }
    settings.theme = normalize_theme(&settings.theme);
    settings.ipatool_keychain_mode =
        normalize_ipatool_keychain_mode(&settings.ipatool_keychain_mode);
    settings.ipatool_engine = normalize_ipatool_engine(&settings.ipatool_engine);
    // Прокси применяем сразу при сохранении. Раньше он подхватывался только
    // при чтении настроек, и человек, нажав «Сохранить», продолжал ходить в
    // сеть по-старому — настройка отставала на одно сохранение.
    remember_proxy(&settings);
    if settings.ipatool_keychain_mode == "custom"
        && settings
            .ipatool_keychain_passphrase
            .as_ref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err("Укажите свой ключ или выберите режим «Рандом»".into());
    }
    if settings.ipatool_keychain_mode == "random" {
        settings.ipatool_keychain_passphrase = None;
        // Never overwrite an existing random key on save — only ensure one exists.
        get_or_create_random_passphrase()?;
    } else if settings.ipatool_keychain_mode == "custom" {
        // Switching to custom: clear random cache so we don't mix keys.
        if let Ok(mut guard) = CACHED_IPATOOL_PASSPHRASE.lock() {
            *guard = None;
        }
    }
    DETAILED_ERRORS_ENABLED.store(settings.detailed_errors, Ordering::Relaxed);
    // Session toggle only drops RAP cookies — NEVER password or purchase-list cache.
    // Password lives in history-secrets independently so history still works offline
    // of the cookie session (user re-auth via password / 2FA without retyping from scratch).
    if !settings.keep_history_session {
        let cache = load_cache(&app);
        for acc in &cache.apple_accounts {
            history_secrets::delete_history_session_cookies_only(&acc.email);
        }
        if let Some(email) = cache.active_account_email.as_deref() {
            history_secrets::delete_history_session_cookies_only(email);
        }
    }
    fs::create_dir_all(&settings.downloads_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&settings.exports_dir).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(&app)?, raw).map_err(|e| e.to_string())?;
    set_autostart(settings.background_enabled)?;
    if settings.background_enabled {
        start_background_refresh(&app);
    }
    refresh_v6_identity(&app, None);
    Ok(())
}

fn http_status_message(status: reqwest::StatusCode, action: &str) -> String {
    match status.as_u16() {
        401 | 403 => {
            format!("{action}: сервис отклонил запрос. Обновите Signum и повторите попытку.")
        }
        404 => format!("{action}: запрошенные данные пока недоступны."),
        408 => format!("{action}: сервис не ответил вовремя. Повторите попытку."),
        426 => "Эта версия Signum устарела. Обновите приложение.".into(),
        429 => format!("{action}: слишком много запросов. Подождите немного и повторите попытку."),
        500..=599 => format!("{action}: сервис временно недоступен. Повторите попытку позже."),
        code => format!("{action}: сервис вернул ошибку (код {code})."),
    }
}

fn http_transport_message(error: &reqwest::Error, action: &str) -> String {
    let message = if error.is_timeout() {
        format!("{action}: сервис не ответил вовремя. Проверьте интернет и повторите попытку.")
    } else if error.is_connect() {
        format!(
            "{action}: не удалось подключиться к сервису. Проверьте интернет и повторите попытку."
        )
    } else if error.is_decode() {
        format!("{action}: сервис вернул некорректные данные. Повторите попытку позже.")
    } else {
        format!("{action}: произошла сетевая ошибка. Проверьте интернет и повторите попытку.")
    };
    if DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed) {
        format!("{message}\n\nПодробный лог:\n{error}")
    } else {
        message
    }
}
/// Shared HTTP client for the Signum API. Plain rustls WebPKI — no pinning:
/// pinned leaf keys break whenever the cert rotates or a VPN/proxy re-issues
/// the chain, which took the whole catalog offline for those users.
pub(crate) fn api_http_client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    match CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("HTTP client: {e}"))
    }) {
        Ok(client) => Ok(client),
        Err(e) => Err(e.clone()),
    }
}

pub(crate) fn api_urls(endpoint: &str) -> Vec<String> {
    std::iter::once(DEFAULT_API)
        .chain(FALLBACK_API)
        .filter_map(|base| {
            let url = format!("{}/{}", base.trim_end_matches('/'), endpoint.trim_start_matches('/'));
            let parsed = reqwest::Url::parse(&url).ok()?;
            let expected = reqwest::Url::parse(base).ok()?;
            (parsed.scheme() == expected.scheme() && parsed.host_str() == expected.host_str())
                .then_some(url)
        })
        .collect()
}

/// Keep the supplied source first, then try the same path on the other API.
/// External Apple/Microsoft URLs are left as-is.
pub(crate) fn api_download_urls(url: &str) -> Vec<String> {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return vec![url.to_string()];
    };
    for base in std::iter::once(DEFAULT_API).chain(FALLBACK_API) {
        let Ok(expected) = reqwest::Url::parse(base) else { continue };
        if parsed.origin() != expected.origin()
            || !parsed.path().starts_with(&format!("{}/", expected.path().trim_end_matches('/')))
        {
            continue;
        }
        let mut endpoint = parsed.path()
            .strip_prefix(expected.path().trim_end_matches('/'))
            .unwrap().to_string();
        if let Some(query) = parsed.query() {
            endpoint.push('?');
            endpoint.push_str(query);
        }
        return std::iter::once(url.to_string())
            .chain(api_urls(&endpoint).into_iter().filter(|candidate| candidate != url))
            .collect();
    }
    vec![url.to_string()]
}

fn retry_api_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || matches!(status.as_u16(), 404 | 408 | 429)
}

async fn api_get_json(
    client: &reqwest::Client,
    urls: &[String],
    query: &[(&str, &str)],
    endpoint: &str,
) -> Result<(reqwest::StatusCode, JsonValue), String> {
    let mut last_error = "Не удалось получить данные".to_string();
    for url in urls {
        let response = match client.get(url).query(query).send().await {
            Ok(response) => response,
            Err(error) => {
                diag_log::http_err("GET", url, &error.to_string());
                last_error = http_transport_message(&error, "Не удалось получить данные");
                continue;
            }
        };
        let status = response.status();
        if retry_api_status(status) {
            last_error = http_status_message(status, "Не удалось получить данные");
            diag_log::note("api", &format!("API returned HTTP {}; trying next server", status.as_u16()));
            continue;
        }
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                last_error = http_transport_message(&error, "Не удалось прочитать ответ сервиса");
                continue;
            }
        };
        diag_log::http("GET", url, status.as_u16(), "", &bytes);
        match serde_json::from_slice::<JsonValue>(&bytes) {
            Ok(value) if value.is_object() && (!status.is_success() || !matches!(endpoint, "main.php" | "info.php") || valid_catalog_response(endpoint, &value)) => return Ok((status, value)),
            _ if status.as_u16() == 426 => return Ok((status, serde_json::json!({"error":"client_outdated", "message": http_status_message(status, "Не удалось получить данные")}))),
            _ => last_error = "Не удалось обработать ответ сервиса".into(),
        }
    }
    Err(last_error)
}

fn update_platform() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        "unknown"
    }
}

async fn api_get(
    _app: &AppHandle,
    endpoint: &str,
    extra: &[(&str, &str)],
) -> Result<JsonValue, String> {
    let mut query = vec![
        ("vers", APP_VERSION),
        ("build", APP_BUILD),
        ("platform", update_platform()),
        ("devkey", ""),
        // Compression is opt-in server side so builds older than this one keep
        // getting a plain body. reqwest transparently inflates the response.
        ("type", "gzip"),
    ];
    for &(key, value) in extra {
        if let Some(existing) = query.iter_mut().find(|(name, _)| *name == key) {
            existing.1 = value;
        } else {
            query.push((key, value));
        }
    }
    let reply = api_get_json(api_http_client()?, &api_urls(endpoint), &query, endpoint).await;
    let cache_file = if matches!(endpoint, "main.php" | "info.php") {
        catalog_response_path(_app, endpoint, extra).ok()
    } else { None };
    let (status, value) = catalog_reply_with_cache(cache_file.as_deref(), endpoint, reply)?;
    if !status.is_success() {
        // API min-client gate (common.php → signum_require_min_client)
        let err_code = value.get("error").and_then(|v| v.as_str()).unwrap_or("");
        if status.as_u16() == 426
            || err_code == "client_outdated"
            || err_code.eq_ignore_ascii_case("client_outdated")
        {
            let msg = value
                .get("message")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Приложение недоступно. Обновите Signum.")
                .to_string();
            let min = value
                .get("min_client_version")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let latest = value
                .get("latest_version")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let download_url = value
                .get("download_url")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let sha256 = value
                .get("sha256")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let _ = _app.emit(
                "client-outdated",
                ClientOutdatedEvent {
                    message: msg.clone(),
                    min_client_version: min,
                    latest_version: latest,
                    download_url,
                    sha256,
                },
            );
            return Err(msg);
        }
        if let Some(message) = value
            .get("message")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return Err(message.to_string());
        }
        if let Some(message) = value
            .get("error")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty() && *v != "client_outdated")
        {
            // Prefer human message; skip raw error codes.
            if message.contains(' ') || message.chars().any(|c| c > '\u{7f}') {
                return Err(message.to_string());
            }
        }
        return Err(http_status_message(status, "Не удалось получить данные"));
    }
    Ok(value)
}

#[derive(Serialize, Deserialize)]
struct CatalogResponseCache {
    saved_at: u64,
    value: JsonValue,
}

fn catalog_response_path(app: &AppHandle, endpoint: &str, extra: &[(&str, &str)]) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?.join("catalog-cache");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let key = serde_json::to_vec(&(APP_VERSION, update_platform(), endpoint, extra)).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("{:x}.json", Sha256::digest(&key))))
}

fn valid_catalog_response(endpoint: &str, value: &JsonValue) -> bool {
    if value.get("ok").and_then(JsonValue::as_bool) == Some(false) { return false; }
    if endpoint == "main.php" {
        value.get("apps").or_else(|| value.get("data")).and_then(JsonValue::as_array)
            .is_some_and(|apps| apps.iter().all(|app| app.get("id").and_then(JsonValue::as_str).is_some()))
    } else { value.get("data").unwrap_or(value).is_object() }
}

fn save_catalog_response(path: &Path, value: &JsonValue) -> Result<(), String> {
    let raw = serde_json::to_vec(&CatalogResponseCache { saved_at: now(), value: value.clone() })
        .map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.part", Uuid::new_v4()));
    fs::write(&temp, raw).map_err(|e| e.to_string())?;
    let result = fs::rename(&temp, path).map_err(|e| e.to_string());
    if result.is_err() { let _ = fs::remove_file(temp); }
    result
}

fn catalog_reply_with_cache(
    path: Option<&Path>, endpoint: &str,
    reply: Result<(reqwest::StatusCode, JsonValue), String>,
) -> Result<(reqwest::StatusCode, JsonValue), String> {
    match reply {
        Ok((status, value)) => {
            if status.is_success() && valid_catalog_response(endpoint, &value) {
                if let Some(path) = path {
                    if let Err(error) = save_catalog_response(path, &value) { diag_log::note("catalog-cache", &error); }
                }
            }
            Ok((status, value))
        }
        Err(error) => {
            let cached = path.and_then(|path| fs::read(path).ok())
                .and_then(|raw| serde_json::from_slice::<CatalogResponseCache>(&raw).ok())
                .filter(|cached| valid_catalog_response(endpoint, &cached.value));
            let Some(mut cached) = cached else { return Err(error) };
            cached.value["_signum_from_cache"] = JsonValue::Bool(true);
            cached.value["_signum_cached_at"] = cached.saved_at.into();
            Ok((reqwest::StatusCode::OK, cached.value))
        }
    }
}

fn managed_tool_target() -> Option<&'static str> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return Some("windows-x64");
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Some("macos-arm64");
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return Some("macos-x64");
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Some("linux-arm64");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return Some("linux-x64");
    }
    #[allow(unreachable_code)]
    None
}

fn managed_tool_extension() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        ".exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        ""
    }
}

fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_managed_tool_file_name(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn managed_tools_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let target = managed_tool_target().ok_or_else(|| "Эта платформа пока не поддерживает обновление компонентов.".to_string())?;
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("tools")
        .join(target);
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось подготовить компоненты: {e}"))?;
    Ok(dir)
}

fn managed_tool_state_path(app: &AppHandle, slot: &str) -> Option<PathBuf> {
    if !MANAGED_TOOL_NAMES.contains(&slot) {
        return None;
    }
    Some(managed_tools_dir(app).ok()?.join(format!("{slot}.json")))
}

fn load_managed_tool_state(app: &AppHandle, slot: &str) -> Option<ManagedToolState> {
    let path = managed_tool_state_path(app, slot)?;
    let raw = fs::read_to_string(path).ok()?;
    let state = serde_json::from_str::<ManagedToolState>(&raw).ok()?;
    if !valid_sha256_hex(&state.sha256) || !valid_managed_tool_file_name(&state.file) {
        return None;
    }
    Some(state)
}

fn managed_tool_path(app: &AppHandle, slot: &str) -> Option<PathBuf> {
    let state = load_managed_tool_state(app, slot)?;
    // Обновление по воздуху не должно откатывать встроенный движок: у кого
    // раньше приехал ipatool-cpp 1.0.7, новая сборка иначе работала бы на нём,
    // пока на сервере не сменят манифест.
    if slot == "ipatool-v5" && compare_versions(&state.version, KBSYNC_UPSTREAM_VERSION) < 0 {
        return None;
    }
    let path = managed_tools_dir(app).ok()?.join(&state.file);
    if !path.is_file() || fs::metadata(&path).ok()?.len() == 0 {
        return None;
    }
    let actual = sha256_file_hex(&path).ok()?;
    if !actual.eq_ignore_ascii_case(&state.sha256) {
        return None;
    }
    Some(path)
}

fn managed_or(app: &AppHandle, slot: &str, resource: &str, dev: &str, fallback: &str) -> String {
    managed_tool_path(app, slot)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| bundled_or(app, resource, dev, fallback))
}

/// Resolve an ipatool engine binary that must exist as a real file.
///
/// Engine slots must NOT fall back to a bare `ipatool` name on PATH: every
/// engine would resolve to the same unrelated binary, `distinct_login_tool_paths`
/// would dedupe them into a single candidate, and "auto" could end up running a
/// tool Signum never shipped. Returns an empty string when the engine is absent;
/// `tool_available` treats that as unavailable.
fn engine_binary_or_empty(app: &AppHandle, slot: &str, resource: &str, dev: &str) -> String {
    if let Some(path) = managed_tool_path(app, slot) {
        return path.to_string_lossy().into_owned();
    }
    if let Ok(path) = app.path().resolve(resource, BaseDirectory::Resource) {
        if path.is_file() {
            return path.to_string_lossy().into_owned();
        }
    }
    dev_candidate(dev)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn sha256_file_hex(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validated_tool_download_path(target: &str, slot: &str, path: &str) -> Option<String> {
    if !MANAGED_TOOL_NAMES.contains(&slot)
        || !matches!(target, "windows-x64" | "macos-x64" | "macos-arm64" | "linux-x64" | "linux-arm64")
    {
        return None;
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 4
        || parts[0] != "files"
        || parts[1] != "tools"
        || parts[2] != target
        || !valid_managed_tool_file_name(parts[3])
    {
        return None;
    }
    Some(parts.join("/"))
}

async fn download_managed_tool(
    app: &AppHandle,
    target: &str,
    slot: &str,
    release: &ToolRelease,
) -> Result<PathBuf, String> {
    if release.version.trim().is_empty()
        || release.size == 0
        || release.size > MAX_MANAGED_TOOL_BYTES
        || !valid_sha256_hex(&release.sha256)
    {
        return Err("Сервис вернул некорректные данные обновления.".into());
    }
    let relative = validated_tool_download_path(target, slot, &release.path)
        .ok_or_else(|| "Сервис вернул недопустимый адрес обновления.".to_string())?;
    let mut last_error = "Не удалось загрузить обновление".to_string();
    for url in api_urls(&relative) {
        match download_managed_tool_once(app, slot, release, &url).await {
            Ok(path) => return Ok(path),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

async fn download_managed_tool_once(
    app: &AppHandle,
    slot: &str,
    release: &ToolRelease,
    url: &str,
) -> Result<PathBuf, String> {
    let expected_sha = release.sha256.to_ascii_lowercase();
    let short_sha = &expected_sha[..16];
    let file_name = format!("{slot}-{short_sha}{}", managed_tool_extension());
    let dir = managed_tools_dir(app)?;
    let final_path = dir.join(&file_name);
    if final_path.is_file()
        && fs::metadata(&final_path).map(|meta| meta.len()).unwrap_or_default() == release.size
        && sha256_file_hex(&final_path).ok().is_some_and(|hash| hash == expected_sha)
    {
        return Ok(final_path);
    }

    let response = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .https_only(true)
        .build()
        .map_err(|error| error.to_string())?
        .get(url)
        .header("Accept-Encoding", "identity")
        .send()
        .await
        .map_err(|error| http_transport_message(&error, "Не удалось загрузить обновление"))?;
    if !response.status().is_success() {
        return Err(http_status_message(response.status(), "Не удалось загрузить обновление"));
    }
    if response.content_length().is_some_and(|value| value != release.size) {
        return Err("Размер обновления не совпадает с данными сервиса.".into());
    }

    let temp_path = dir.join(format!(".{file_name}.{}.part", Uuid::new_v4()));
    let result = async {
        let mut file = fs::File::create(&temp_path).map_err(|e| format!("Не удалось сохранить обновление: {e}"))?;
        let mut written = 0u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| http_transport_message(&error, "Не удалось загрузить обновление"))?;
            written = written
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| "Размер обновления недопустим.".to_string())?;
            if written > release.size {
                return Err("Размер обновления не совпадает с данными сервиса.".into());
            }
            file.write_all(&chunk)
                .map_err(|e| format!("Не удалось сохранить обновление: {e}"))?;
        }
        file.sync_all()
            .map_err(|e| format!("Не удалось сохранить обновление: {e}"))?;
        drop(file);

        if written != release.size
            || sha256_file_hex(&temp_path)
                .ok()
                .map_or(true, |hash| !hash.eq_ignore_ascii_case(&expected_sha))
        {
            return Err("Проверка обновления не пройдена. Текущая версия сохранена.".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("Не удалось подготовить обновление: {e}"))?;
        }
        fs::rename(&temp_path, &final_path)
            .map_err(|e| format!("Не удалось применить обновление: {e}"))?;
        Ok(final_path)
    }.await;
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

async fn refresh_managed_tools(app: &AppHandle) -> Result<ToolUpdateReport, String> {
    let Some(target) = managed_tool_target() else {
        return Ok(ToolUpdateReport::default());
    };
    let state = app.state::<AppState>();
    let _guard = state.tool_update_lock.lock().await;
    let value = api_get(app, "tools.php", &[("target", target)]).await?;
    let manifest = serde_json::from_value::<ToolManifestResponse>(value)
        .map_err(|_| "Сервис вернул некорректные данные обновления.".to_string())?;
    if !manifest.ok || manifest.schema_version != TOOLS_MANIFEST_SCHEMA || manifest.target != target {
        return Err("Сервис вернул некорректные данные обновления.".into());
    }

    let mut report = ToolUpdateReport {
        checked: true,
        updated: Vec::new(),
    };
    for slot in MANAGED_TOOL_NAMES {
        let Some(release) = manifest.tools.get(*slot) else {
            continue;
        };
        let local = load_managed_tool_state(app, slot);
        let local_path = managed_tool_path(app, slot);
        let needs_update = match (&local, &local_path) {
            (Some(state), Some(_)) => {
                let version_order = compare_versions(&release.version, &state.version);
                version_order > 0
                    || (version_order == 0 && !release.sha256.eq_ignore_ascii_case(&state.sha256))
            }
            _ => true,
        };
        if !needs_update {
            continue;
        }

        let path = download_managed_tool(app, target, slot, release).await?;
        let state = ManagedToolState {
            version: release.version.trim().to_string(),
            sha256: release.sha256.to_ascii_lowercase(),
            file: path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "Не удалось сохранить обновление.".to_string())?
                .to_string(),
        };
        let raw = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
        fs::write(
            managed_tool_state_path(app, slot).ok_or_else(|| "Не удалось сохранить обновление.".to_string())?,
            raw,
        )
        .map_err(|e| format!("Не удалось сохранить обновление: {e}"))?;
        report.updated.push((*slot).to_string());
    }
    Ok(report)
}

async fn startup_managed_tool_update(app: AppHandle) {
    match refresh_managed_tools(&app).await {
        Ok(report) if !report.updated.is_empty() => {
            eprintln!("Signum: updated managed tools: {}", report.updated.join(", "));
        }
        Ok(_) => {}
        // Network or release errors must never block a working local installation.
        Err(error) => eprintln!("Signum: managed-tool update skipped: {error}"),
    }
}

#[tauri::command]
async fn check_tool_updates(app: AppHandle) -> Result<ToolUpdateReport, String> {
    refresh_managed_tools(&app).await
}

#[derive(Debug, Clone, Serialize)]
struct TbActivationResult {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qr_payload: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Live T-Business activation QR from API tb.php?id=…
/// Pass force=true to mint a new QR even inside the 10-minute cache window.
#[tauri::command]
async fn fetch_tb_activation(
    app: AppHandle,
    id: String,
    force: Option<bool>,
) -> Result<TbActivationResult, String> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("Не указан id активации (tb.php?id=…)".into());
    }
    let force_flag = if force.unwrap_or(false) { "1" } else { "0" };
    let value = api_get(
        &app,
        "tb.php",
        &[("id", id.as_str()), ("force", force_flag)],
    )
    .await?;
    let ok = json_bool(value.get("ok"), false);
    let qr_payload = value
        .get("qr_payload")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    if !ok || qr_payload.is_none() {
        let error = value
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Не удалось сгенерировать QR активации")
            .to_string();
        return Err(error);
    }
    Ok(TbActivationResult {
        ok: true,
        id: value
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or(Some(id)),
        name: value
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        qr_payload,
        mode: value
            .get("mode")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| Some("deeplink".into())),
        hint: value
            .get("hint")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        expires_in: value.get("expires_in").and_then(|v| v.as_u64()),
        ttl_seconds: value.get("ttl_seconds").and_then(|v| v.as_u64()),
        from_cache: value.get("from_cache").and_then(|v| v.as_bool()),
        error: None,
    })
}

fn json_bool(value: Option<&JsonValue>, default: bool) -> bool {
    match value {
        Some(JsonValue::Bool(v)) => *v,
        Some(JsonValue::Number(v)) => v.as_i64().unwrap_or(0) != 0,
        Some(JsonValue::String(v)) => matches!(
            v.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "active" | "available"
        ),
        _ => default,
    }
}

fn latest_version_entry(info: &JsonValue) -> Option<&JsonValue> {
    let versions = info.get("versions")?.as_array()?;
    versions
        .iter()
        .find(|version| json_bool(version.get("is_latest"), false))
        .or_else(|| versions.first())
}

fn latest_version_string(info: &JsonValue, field: &str) -> String {
    latest_version_entry(info)
        .and_then(|version| version.get(field))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

fn latest_version_bool(info: &JsonValue, field: &str, default: bool) -> bool {
    latest_version_entry(info)
        .map(|version| json_bool(version.get(field), default))
        .unwrap_or(default)
}

fn version_is_store_offer(version: &JsonValue) -> bool {
    json_bool(version.get("in_store"), false)
}

fn version_sort_score(version: &JsonValue) -> (i8, i8, i8) {
    let store = i8::from(version_is_store_offer(version));
    let latest = i8::from(json_bool(version.get("is_latest"), false));
    let version_name = version
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let recency = compare_versions(version_name, "0.0.0");
    (store, latest, recency)
}

fn sorted_catalog_versions(versions: &[JsonValue]) -> Vec<&JsonValue> {
    let mut items: Vec<_> = versions.iter().collect();
    items.sort_by(|left, right| {
        let left_score = version_sort_score(left);
        let right_score = version_sort_score(right);
        right_score
            .0
            .cmp(&left_score.0)
            .then(right_score.1.cmp(&left_score.1))
            .then(right_score.2.cmp(&left_score.2))
    });
    items
}

/// Apple Store passwordToken expired — auth info may still show email, but
/// list-versions / download need a fresh `auth login` (often with 2FA).
fn is_password_token_expired_error(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("password token is expired")
        || lowered.contains("passwordtoken is expired")
        || lowered.contains("password token expired")
        || lowered.contains("token is expired")
        || (lowered.contains("passwordtoken") && lowered.contains("expir"))
        || lowered.contains("mzfinance.authenticate") && lowered.contains("expir")
}

fn password_token_expired_message() -> String {
    "Сессия Apple ID для App Store устарела (password token).\n\n\
     Откройте «Аккаунт» → выйдите из Apple ID и войдите снова \
     (Apple может запросить код 2FA). После этого список версий появится."
        .into()
}

/// Extract complete top-level JSON objects from CLI output that may also contain
/// log lines or pretty-printed JSON. Braces inside JSON strings are ignored.
fn ipatool_json_objects(text: &str) -> Vec<&str> {
    let mut objects = Vec::new();
    let mut start = None;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;

    for (offset, ch) in text.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' if start.is_some() => quoted = true,
            '{' => {
                if start.is_none() {
                    start = Some(offset);
                }
                depth += 1;
            }
            '}' if start.is_some() => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(begin) = start.take() {
                        objects.push(&text[begin..offset + ch.len_utf8()]);
                    }
                }
            }
            _ => {}
        }
    }
    objects
}

fn version_id_string(value: &JsonValue) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|id| id.to_string()))
        .or_else(|| value.as_u64().map(|id| id.to_string()))
        .or_else(|| {
            value
                .get("externalVersionId")
                .or_else(|| value.get("external_version_id"))
                .or_else(|| value.get("externalVersionIdentifier"))
                .or_else(|| value.get("id"))
                .and_then(version_id_string)
        })
}

fn parse_ipatool_list_versions(text: &str) -> Option<Vec<String>> {
    let clean = strip_ansi(text);
    let lowered = clean.to_ascii_lowercase();
    // Must re-login — do not treat as «no versions on this account».
    // A pending 2FA code or an unreadable keychain is the same kind of
    // "cannot answer": ipatool never reached the Store, so ownership is unknown.
    if is_password_token_expired_error(&clean)
        || lowered.contains("not logged in")
        || lowered.contains("failed to decrypt account")
        || is_two_factor_error(&clean)
        || is_keychain_missing_item_error(&clean)
        || is_keychain_integrity_error(&clean)
        || is_keychain_passphrase_required_error(&clean)
    {
        return None;
    }
    // IPA Downloader / ipatool-cpp: unpurchased or empty history.
    if is_not_purchased_store_error(&clean) {
        return Some(Vec::new());
    }

    let mut saw_success = false;
    // Accept a bare JSON object, JSON after log lines, and multi-line JSON.
    // `ipatool_json_objects` prevents unrelated log text from being parsed as IDs.
    for object in ipatool_json_objects(&clean) {
        let Ok(value) = serde_json::from_str::<JsonValue>(object) else {
            continue;
        };
        if !json_bool(value.get("success"), false) {
            // Never let a failed JSON response fall through to text parsing: it may
            // still echo numeric identifiers that do not prove Apple ID ownership.
            return if is_region_locked_owned_app_error(&clean) {
                Some(Vec::new())
            } else {
                None
            };
        }
        saw_success = true;
        let ids = value
            .get("externalVersionIdentifiers")
            .or_else(|| value.get("external_version_identifiers"))
            .or_else(|| value.get("externalVersionIds"))
            .or_else(|| value.get("versions"))
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(version_id_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if value.get("externalVersionIdentifiers").is_some()
            || value.get("external_version_identifiers").is_some()
            || value.get("externalVersionIds").is_some()
            || value.get("versions").is_some()
            || !ids.is_empty()
        {
            return Some(ids);
        }
    }
    if saw_success {
        return Some(Vec::new());
    }

    // IPA_Downloader text mode: only parse quoted numeric IDs after an explicit
    // list-versions label. This avoids mistaking unrelated numeric log fields for
    // proof that an app belongs to the Apple ID.
    let marker = [
        "externalversionidentifiers",
        "external_version_identifiers",
        "externalversionids",
    ]
    .iter()
    .filter_map(|marker| lowered.find(marker))
    .min();
    static QUOTED_IDS: OnceLock<Regex> = OnceLock::new();
    let re = QUOTED_IDS.get_or_init(|| Regex::new(r#""(\d{4,})""#).unwrap());
    let mut ids: Vec<String> = marker
        .map(|start| {
            re.captures_iter(&clean[start..])
                .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    if !ids.is_empty() {
        return Some(ids);
    }
    // Bare error from ipatool without JSON — empty list only when not a session failure.
    if lowered.contains("error:") {
        if is_password_token_expired_error(&clean)
            || lowered.contains("not logged in")
            || lowered.contains("failed to decrypt")
        {
            return None;
        }
        return Some(Vec::new());
    }
    None
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchAppStoreVersionsInput {
    adam_id: String,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    limit: u32,
    #[serde(default)]
    refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppStoreVersion {
    version: String,
    external_version_id: String,
    is_latest: bool,
    already_owned: bool,
    /// Minimum iOS of this exact build, once it has been read. `None` means
    /// "not looked up yet", never "runs anywhere".
    #[serde(skip_serializing_if = "Option::is_none")]
    min_ios: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppStoreVersionsPage {
    versions: Vec<AppStoreVersion>,
    has_more: bool,
    total: usize,
    offset: usize,
    /// Empty list because the app is owned but absent from this storefront —
    /// worth saying out loud, since "no versions" reads like a failure.
    region_blocked: bool,
}

fn json_string_field(value: &JsonValue, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(|v| {
            v.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| v.as_f64().map(|n| n.to_string()))
                .or_else(|| v.as_i64().map(|n| n.to_string()))
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        }) {
            return Some(found);
        }
    }
    None
}

/// Human-readable marketing version from `get-version-metadata`.
/// ipatool-cpp JSON: `{"displayVersion":"1.0.0","externalVersionID":"…","success":true}`
fn ipatool_metadata_version(output: &str, fallback: &str) -> String {
    let clean = strip_ansi(output);

    // Text mode: `INF displayVersion=1.0.0 externalVersionID=883… success=true`
    static DISPLAY_RE: OnceLock<Regex> = OnceLock::new();
    let re = DISPLAY_RE.get_or_init(|| {
        Regex::new(r#"(?i)displayVersion["\s:=]+([0-9]+(?:\.[0-9]+){0,3}[a-zA-Z0-9._-]*)"#).unwrap()
    });
    if let Some(caps) = re.captures(&clean) {
        if let Some(m) = caps.get(1) {
            let ver = m.as_str().trim();
            if !ver.is_empty() && ver != fallback {
                return ver.to_string();
            }
        }
    }

    let value = std::iter::once(clean.trim())
        .chain(clean.lines().map(str::trim))
        .filter(|line| line.starts_with('{'))
        .find_map(|line| serde_json::from_str::<JsonValue>(line).ok());
    let Some(value) = value.as_ref() else {
        return fallback.to_string();
    };

    // Prefer marketing version (1.0.1), not externalVersionID.
    let keys = [
        "displayVersion",
        "display_version",
        "bundleShortVersionString",
        "shortVersionString",
        "versionString",
        "version",
        "bundleVersion",
        "CFBundleShortVersionString",
        "CFBundleVersion",
    ];
    let candidates = [
        value,
        value.get("data").unwrap_or(value),
        value
            .pointer("/data/metadata")
            .or_else(|| value.pointer("/metadata"))
            .unwrap_or(value),
        value
            .pointer("/data/item")
            .or_else(|| value.pointer("/item"))
            .unwrap_or(value),
    ];
    for node in candidates {
        if let Some(ver) = json_string_field(node, &keys) {
            if ver.chars().all(|c| c.is_ascii_digit()) && ver.len() >= 8 {
                // Likely still an external id — keep looking for a dotted version.
                continue;
            }
            return ver;
        }
    }
    for node in candidates {
        if let Some(ver) = json_string_field(node, &keys) {
            return ver;
        }
    }
    fallback.to_string()
}

async fn load_owned_version_ids(
    app: &AppHandle,
    adam_id: &str,
) -> Result<VersionPickerList, String> {
    if let Some(ids) = app.state::<AppState>().version_probe_cache.lock().unwrap().get(adam_id).cloned() {
        let ids = take_recent_version_ids(ids, None, usize::MAX);
        return Ok(VersionPickerList {
            latest_id: ids.first().cloned(), ids, latest_display: None, region_blocked: false,
        });
    }
    let anisette = resolve_store_sap_helper(app);
    if let Some(email) = active_account_email(app) {
        if apple_store::helper_available(anisette.as_deref()) {
            match apple_store::list_versions(anisette.as_deref(), &email, adam_id).await {
                Ok(native) if !native.external_ids.is_empty() => {
                    let latest_display = native.latest_display.filter(|display| {
                        !(display.chars().all(|c| c.is_ascii_digit()) && display.len() >= 8)
                    });
                    let ids = take_recent_version_ids(
                        native.external_ids,
                        native.latest_id.as_deref(),
                        usize::MAX,
                    );
                    return Ok(VersionPickerList {
                        ids,
                        latest_id: native.latest_id,
                        latest_display,
                        region_blocked: false,
                    });
                }
                Ok(_) => {}
                Err(error) if error.starts_with("SIGNUM_2FA_REQUIRED") => {
                    return Err(password_token_expired_message());
                }
                Err(_) => {}
            }
        }
    }

    let tools = resolve_tool_paths(app)?;
    let keychain = resolve_ipatool_keychain(app)?;
    let mut list_args = Vec::new();
    append_ipatool_keychain_args(&mut list_args, &keychain);
    list_args.extend([
        "--format".into(),
        "json".into(),
        "list-versions".into(),
        "-i".into(),
        adam_id.to_string(),
    ]);
    cleanup_stale_cookie_lock(true);
    let output = run_process_raw_error(
        &tools.ipatool,
        &list_args,
        Duration::from_secs(ACCOUNT_VERSION_PROBE_TIMEOUT_SECS),
        None,
    )
    .await
    .unwrap_or_else(|error| error);
    if is_password_token_expired_error(&output)
        || output.to_ascii_lowercase().contains("not logged in")
    {
        return Err(password_token_expired_message());
    }
    let ids = match parse_ipatool_list_versions(&output) {
        Some(ids) => ids,
        None if is_password_token_expired_error(&output)
            || output.to_ascii_lowercase().contains("not logged in")
            || output.to_ascii_lowercase().contains("failed to decrypt") =>
        {
            return Err(password_token_expired_message());
        }
        None if is_two_factor_error(&output) => {
            return Err(two_factor_required_login_error());
        }
        None => {
            // Never surface the raw tool output here: it carries JSON, keychain
            // and base64 wording. Keep the log technical, the message plain.
            ipatool_debug_log(&format!(
                "list-versions unreadable adam={adam_id} {}",
                technical_excerpt(&output, 400)
            ));
            // Сюда попадаем, только когда ответ движка не разобрался. Протухший
            // вход и запрос кода перехвачены выше — значит вход в порядке, и совет
            // «выйдите и войдите заново» лишь отнял бы рабочую сессию и код.
            return Err(with_technical_detail(
                "Apple не прислала список версий. Повторите через минуту — текущую версию можно установить уже сейчас.",
                &output,
            ));
        }
    };
    let region_blocked = ids.is_empty() && is_region_locked_owned_app_error(&output);
    let ids = take_recent_version_ids(ids, None, usize::MAX);
    let latest_id = ids.first().cloned();
    Ok(VersionPickerList {
        ids,
        latest_id,
        latest_display: None,
        region_blocked,
    })
}

fn valid_min_ios(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| {
            !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u32>().is_ok()
        })
        && value.split('.').next().and_then(|part| part.parse::<u32>().ok()).is_some_and(|major| major > 0)
}

fn require_known_min_ios(value: Option<String>) -> Result<String, String> {
    value.filter(|value| valid_min_ios(value)).ok_or_else(|| {
        "Не удалось определить минимальную iOS одной из сборок. Повторите подбор позже или выберите версию вручную.".into()
    })
}

/// Only accept a successful answer for the requested build and iOS platform.
/// A successful metadata command can still omit the best-effort OS field.
fn ipatool_metadata_min_ios(output: &str, external_version_id: &str) -> Option<String> {
    let clean = strip_ansi(output);
    for object in ipatool_json_objects(&clean) {
        let Ok(value) = serde_json::from_str::<JsonValue>(object) else { continue; };
        if value.get("success").and_then(JsonValue::as_bool) != Some(true)
            || value.get("externalVersionID").and_then(version_id_string).as_deref() != Some(external_version_id)
        {
            continue;
        }
        if let Some(platforms) = value.get("supportedPlatforms") {
            if !platforms.as_array()?.iter().any(|platform| platform.as_str() == Some("iPhoneOS")) {
                return None;
            }
        }
        let required = value.get("minimumOSVersion")?.as_str()?.trim();
        return valid_min_ios(required).then(|| required.to_string());
    }
    None
}

async fn run_ipatool_version_metadata(
    program: &str,
    keychain: &IpatoolKeychain,
    adam_id: &str,
    external_version_id: &str,
    timeout: Duration,
) -> Result<String, String> {
    let mut args = Vec::new();
    append_ipatool_keychain_args(&mut args, keychain);
    args.extend([
        "--format".into(), "json".into(), "get-version-metadata".into(),
        "-i".into(), adam_id.to_string(),
        "--external-version-id".into(), external_version_id.to_string(),
    ]);
    run_process_raw_error(program, &args, timeout, None).await
}

async fn min_ios_with_reserve<P, R, F>(primary: P, reserve: R) -> Option<String>
where
    P: std::future::Future<Output = Option<String>>,
    R: FnOnce() -> F,
    F: std::future::Future<Output = Option<String>>,
{
    if let Some(value) = primary.await.filter(|value| valid_min_ios(value)) {
        return Some(value);
    }
    reserve().await.filter(|value| valid_min_ios(value))
}

// The CLI may make several CDN range requests, each with a 60-second timeout.
// This lookup runs separately from the version list and streams each result.
const IPATOOL_MIN_IOS_TIMEOUT_SECS: u64 = 180;

/// Caller holds ipatool_lock: all engines share account files and cookies.
async fn resolve_build_min_ios(app: &AppHandle, adam_id: &str, ext: &str) -> Option<String> {
    if let Some(known) = apple_store::cached_version_min_ios(adam_id, &[ext.to_string()])
        .remove(ext).filter(|value| valid_min_ios(value))
    {
        return Some(known);
    }
    let value = min_ios_with_reserve(async {
        let tools = resolve_tool_paths(app).ok()?;
        let keychain = resolve_ipatool_keychain(app).ok()?;
        let output = match run_ipatool_version_metadata(
            &tools.ipatool, &keychain, adam_id, ext,
            Duration::from_secs(IPATOOL_MIN_IOS_TIMEOUT_SECS),
        ).await {
            Ok(output) => output,
            Err(error) => {
                ipatool_debug_log(&format!("min-ios ipatool failed {adam_id}:{ext}: {}", technical_excerpt(&error, 300)));
                return None;
            }
        };
        let required = ipatool_metadata_min_ios(&output, ext)?;
        let label = ipatool_metadata_version(&output, ext);
        if label != ext {
            apple_store::remember_version_label(adam_id, ext, &label);
        }
        ipatool_debug_log(&format!("min-ios source=ipatool {adam_id}:{ext} ios={required}"));
        Some(required)
    }, || async {
        ipatool_debug_log(&format!("min-ios source=signum-reserve {adam_id}:{ext}"));
        let email = active_account_email(app)?;
        if !apple_store::load_session(&email).is_some_and(|session| session.is_ready()) {
            if let Ok(IpatoolKeychain::Passphrase(passphrase)) = resolve_ipatool_keychain(app) {
                apple_store::load_ipatool_session(&email, &passphrase);
            }
        }
        let helper = resolve_store_sap_helper(app);
        apple_store::version_min_ios(helper.as_deref(), &email, adam_id, ext).await
    }).await?;
    apple_store::remember_version_min_ios(adam_id, ext, &value);
    Some(value)
}

/// Shorter budget for marketing labels; minimum-iOS lookups run separately.
const IPATOOL_LABEL_TIMEOUT_SECS: u64 = 20;
/// Seconds the whole fallback may take before the page is shown with whatever
/// labels arrived. The picker must never feel stuck behind label lookups.
const IPATOOL_LABEL_BUDGET_SECS: u64 = 30;
/// The engines share one cookie jar; concurrent writers corrupt the session.
const IPATOOL_LABEL_CONCURRENCY: usize = 1;

/// Versions ipatool could not label either, so a page revisit does not re-probe
/// them. Session-scoped on purpose — a re-login may well change the answer.
fn unlabelable_versions() -> &'static StdMutex<HashSet<String>> {
    static SET: OnceLock<StdMutex<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(|| StdMutex::new(HashSet::new()))
}

/// Marketing labels via `ipatool get-version-metadata`.
///
/// The native Store client needs a SAP signature to get labels back; when its
/// signer cannot run — a macOS bundle whose `sapsigner` lost the executable
/// bit, an engine that ships none — Apple answers with an empty songList and
/// every row in the picker degrades to «сборка». ipatool already holds this
/// account's session and answers the same question without the signer, so it
/// is the fallback rather than a second guess.
async fn ipatool_version_labels(
    app: &AppHandle,
    adam_id: &str,
    ids: &[String],
) -> HashMap<String, String> {
    let mut pending: Vec<String> = Vec::new();
    {
        let Ok(skip) = unlabelable_versions().lock() else {
            return HashMap::new();
        };
        for ext in ids {
            let ext = ext.trim();
            if !ext.is_empty() && !skip.contains(&format!("{adam_id}:{ext}")) {
                pending.push(ext.to_string());
            }
        }
    }
    if pending.is_empty() {
        return HashMap::new();
    }
    let (Ok(tools), Ok(keychain)) = (resolve_tool_paths(app), resolve_ipatool_keychain(app)) else {
        return HashMap::new();
    };
    let program = tools.ipatool.clone();
    let found: Arc<StdMutex<HashMap<String, String>>> = Arc::new(StdMutex::new(HashMap::new()));
    let misses: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
    let work = {
        let found = Arc::clone(&found);
        let misses = Arc::clone(&misses);
        stream::iter(pending).for_each_concurrent(IPATOOL_LABEL_CONCURRENCY, move |ext| {
            let program = program.clone();
            let keychain = keychain.clone();
            let adam = adam_id.to_string();
            let found = Arc::clone(&found);
            let misses = Arc::clone(&misses);
            async move {
                let output = match run_ipatool_version_metadata(
                    &program, &keychain, &adam, &ext,
                    Duration::from_secs(IPATOOL_LABEL_TIMEOUT_SECS),
                ).await {
                    Ok(output) => output,
                    Err(_) => return,
                };
                // 1.2.0 may already have read Info.plist while resolving the
                // label. Reuse it instead of asking the CDN for it again.
                if let Some(required) = ipatool_metadata_min_ios(&output, &ext) {
                    apple_store::remember_version_min_ios(&adam, &ext, &required);
                }
                let display = ipatool_metadata_version(&output, &ext);
                let usable = display != ext
                    && !display.trim().is_empty()
                    && !(display.chars().all(|c| c.is_ascii_digit()) && display.len() >= 8);
                if usable {
                    apple_store::remember_version_label(&adam, &ext, &display);
                    if let Ok(mut map) = found.lock() {
                        map.insert(ext, display);
                    }
                } else if let Ok(mut list) = misses.lock() {
                    list.push(ext);
                }
            }
        })
    };
    // Whatever finished inside the budget is kept; the rest simply stays a
    // build number for this visit.
    let _ = tokio::time::timeout(Duration::from_secs(IPATOOL_LABEL_BUDGET_SECS), work).await;

    let labels = found
        .lock()
        .map(|map| map.clone())
        .unwrap_or_default();
    for (ext, display) in &labels {
        apple_store::remember_version_label(adam_id, ext, display);
    }
    if let (Ok(mut skip), Ok(list)) = (unlabelable_versions().lock(), misses.lock()) {
        for ext in list.iter() {
            skip.insert(format!("{adam_id}:{ext}"));
        }
    }
    labels
}

async fn label_version_page(
    app: &AppHandle,
    adam_id: &str,
    list: &VersionPickerList,
    page_ids: Vec<String>,
    resolve_labels: bool,
) -> Vec<AppStoreVersion> {
    let anisette = resolve_store_sap_helper(app);
    let email = active_account_email(app);
    let native_labels = resolve_labels && apple_store::helper_available(anisette.as_deref()) && email.is_some();
    let latest_id = list.latest_id.clone();
    let latest_display = list.latest_display.clone();
    // Resolve every label for this page in one batch. Asking one at a time made
    // the requests queue behind a single session lock, which is what made a
    // long-history app take minutes to open its version list.
    let mut batch = match (native_labels, email.as_deref()) {
        (true, Some(mail)) => apple_store::version_displays(anisette.as_deref(), mail, adam_id, &page_ids).await,
        _ => apple_store::cached_version_labels(adam_id, &page_ids),
    };
    // Whatever the Store client could not name is handed to ipatool, which
    // carries its own Apple session and keeps working when the native one has
    // expired — the usual case being a page where older builds answer from
    // cache and the newest one, the single label a user actually looks at,
    // stays a raw build number.
    //
    // Only the leftovers are asked for, never the whole page. Cost stays
    // bounded by the budget and concurrency inside `ipatool_version_labels`,
    // and ids it fails on are remembered so a long history does not re-probe
    // them on every visit — which is what made this slow when it was tried
    // per item before.
    let unlabelled: Vec<String> = page_ids
        .iter()
        .filter(|ext| !batch.contains_key(*ext))
        .cloned()
        .collect();
    if resolve_labels && !unlabelled.is_empty() {
        batch.extend(ipatool_version_labels(app, adam_id, &unlabelled).await);
    }
    let min_ios = apple_store::cached_version_min_ios(adam_id, &page_ids);
    // Nothing async is left per item — the labels are already resolved above.
    let mut versions: Vec<AppStoreVersion> = page_ids
        .into_iter()
        .map(|external_version_id| {
            let is_latest = latest_id
                .as_deref()
                .map(|id| id == external_version_id)
                .unwrap_or(false);
            if is_latest {
                if let Some(display) = latest_display.clone() {
                    return AppStoreVersion {
                        version: display,
                        is_latest: true,
                        external_version_id,
                        already_owned: true,
                        min_ios: None,
                    };
                }
            }
            if let Some(human) = batch.get(&external_version_id) {
                if !(human.chars().all(|c| c.is_ascii_digit()) && human.len() >= 8) {
                    return AppStoreVersion {
                        version: human.clone(),
                        is_latest,
                        external_version_id,
                        already_owned: true,
                        min_ios: None,
                    };
                }
            }
            AppStoreVersion {
                version: external_version_id.clone(),
                is_latest,
                external_version_id,
                already_owned: true,
                min_ios: None,
            }
        })
        .collect();
    versions.sort_by(|a, b| {
        // Последняя сборка — всегда первой. Иначе она тонула под старыми версиями
        // всякий раз, когда Apple не назвала её маркетинговую версию:
        // безымянные уходят в конец, и «Последняя» оказывалась внизу.
        b.is_latest
            .cmp(&a.is_latest)
            .then_with(|| cmp_marketing_version_desc(&a.version, &b.version))
            .then_with(|| b.external_version_id.cmp(&a.external_version_id))
    });
    for item in &mut versions {
        item.min_ios = min_ios.get(&item.external_version_id).cloned();
    }
    versions
}

/// How many builds one minimum-iOS request may resolve. Matches the picker page
/// so a visible list is covered by a single call.
const MIN_IOS_PAGE: usize = 12;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppStoreVersionsIosInput {
    adam_id: String,
    external_version_ids: Vec<String>,
}

#[tauri::command]
async fn resolve_app_store_versions_labels(
    app: AppHandle,
    input: AppStoreVersionsIosInput,
    on_update: tauri::ipc::Channel<HashMap<String, String>>,
) -> Result<HashMap<String, String>, String> {
    require_auth(&app)?;
    let adam_id = input.adam_id.trim().to_string();
    if adam_id.is_empty() || !adam_id.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("Не указан корректный идентификатор App Store".into());
    }
    let expected = active_account_email(&app).ok_or_else(|| "Войдите в Apple ID".to_string())?;
    let mut labels = HashMap::new();
    for id in input.external_version_ids.into_iter().take(MIN_IOS_PAGE) {
        let state = app.state::<AppState>();
        let _guard = state.ipatool_lock.lock().await;
        let actual = ensure_active_ipatool_session(&app).await?;
        if actual != expected { return Err("Apple ID изменился. Откройте приложение заново.".into()); }
        let rows = label_version_page(&app, &adam_id, &VersionPickerList::default(), vec![id.clone()], true).await;
        if let Some(row) = rows.first().filter(|row| row.version != id) {
            let label = row.version.clone();
            labels.insert(id.clone(), label.clone());
            let _ = on_update.send(HashMap::from([(id, label)]));
        }
    }
    Ok(labels)
}

/// Read the minimum iOS for the builds the picker is showing.
///
/// Deliberately split from `fetch_app_store_versions`: labels come back in one
/// round trip, while each minimum iOS needs the build's own Info.plist. Keeping
/// them apart means the list and the install button never wait for this.
#[tauri::command]
async fn resolve_app_store_versions_ios(
    app: AppHandle,
    input: AppStoreVersionsIosInput,
    on_update: tauri::ipc::Channel<HashMap<String, String>>,
) -> Result<HashMap<String, String>, String> {
    require_auth(&app)?;
    let adam_id = input.adam_id.trim().to_string();
    let ids: Vec<String> = input
        .external_version_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| normalized_app_store_id(Some(id)).is_some())
        .take(MIN_IOS_PAGE)
        .collect();
    if normalized_app_store_id(Some(&adam_id)).is_none() || ids.is_empty() || DEMO_MODE {
        return Ok(HashMap::new());
    }
    let expected = active_account_email(&app).ok_or_else(|| "Войдите в Apple ID".to_string())?;
    let mut known = apple_store::cached_version_min_ios(&adam_id, &ids);
    known.retain(|_, value| valid_min_ios(value));
    if !known.is_empty() { let _ = on_update.send(known.clone()); }
    for ext in ids {
        if known.contains_key(&ext) { continue; }
        let state = app.state::<AppState>();
        let _guard = state.ipatool_lock.lock().await;
        if active_account_email(&app).as_deref() != Some(expected.as_str()) {
            return Err("Apple ID изменился. Откройте приложение заново.".into());
        }
        // The fallback has its own native session; an unavailable CLI session
        // must not prevent Signum from trying that reserve.
        let _ = ensure_active_ipatool_session(&app).await;
        if let Some(value) = resolve_build_min_ios(&app, &adam_id, &ext).await {
            let _ = on_update.send(HashMap::from([(ext.clone(), value.clone())]));
            known.insert(ext, value);
        }
    }
    Ok(known)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FitVersionInput {
    adam_id: String,
    device_ios: String,
}

/// Newest build this iPhone can actually run.
///
/// Bisect the build list and cache each requirement. An unknown requirement
/// aborts selection instead of silently treating a network error as "too new".
#[tauri::command]
async fn find_app_store_version_for_device(
    app: AppHandle,
    state: State<'_, AppState>,
    input: FitVersionInput,
) -> Result<Option<AppStoreVersion>, String> {
    require_auth(&app)?;
    let adam_id = input.adam_id.trim().to_string();
    let device_ios = input.device_ios.trim().to_string();
    if normalized_app_store_id(Some(&adam_id)).is_none() || !valid_min_ios(&device_ios) {
        return Err("Подключите iPhone, чтобы подобрать версию".into());
    }
    if DEMO_MODE {
        return Ok(None);
    }
    let anisette = resolve_store_sap_helper(&app);
    let Some(email) = active_account_email(&app) else {
        return Err("Войдите в Apple ID, чтобы подобрать версию".into());
    };

    let _guard = state.ipatool_lock.lock().await;
    let _ = ensure_active_ipatool_session(&app).await;

    let list = {
        let cached = state
            .version_picker_cache
            .lock()
            .unwrap()
            .get(&adam_id)
            .cloned();
        match cached {
            Some(list) => list,
            None => {
                let loaded = load_owned_version_ids(&app, &adam_id).await?;
                state
                    .version_picker_cache
                    .lock()
                    .unwrap()
                    .insert(adam_id.clone(), loaded.clone());
                loaded
            }
        }
    };
    persist_account_catalog_cache(&app);
    if list.ids.is_empty() {
        return Ok(None);
    }

    // ids run newest first, so the answer is the smallest index that fits.
    let (mut low, mut high) = (0usize, list.ids.len());
    let mut found: Option<(String, String)> = None;
    while low < high {
        let mid = (low + high) / 2;
        let ext = list.ids[mid].clone();
        let min_ios = require_known_min_ios(resolve_build_min_ios(&app, &adam_id, &ext).await)?;
        if device_supports_min_ios(Some(&device_ios), Some(&min_ios)) == Some(true) {
            found = Some((ext, min_ios));
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    let Some((external_version_id, min_ios)) = found else {
        return Ok(None);
    };
    let labels = apple_store::version_displays(
        anisette.as_deref(),
        &email,
        &adam_id,
        std::slice::from_ref(&external_version_id),
    )
    .await;
    Ok(Some(AppStoreVersion {
        version: labels
            .get(&external_version_id)
            .cloned()
            .unwrap_or_else(|| external_version_id.clone()),
        is_latest: list.latest_id.as_deref() == Some(external_version_id.as_str()),
        already_owned: true,
        min_ios: Some(min_ios),
        external_version_id,
    }))
}

#[tauri::command]
async fn fetch_app_store_versions(
    app: AppHandle,
    state: State<'_, AppState>,
    input: FetchAppStoreVersionsInput,
) -> Result<AppStoreVersionsPage, String> {
    require_auth(&app)?;
    let adam_id = input.adam_id.trim();
    if adam_id.is_empty() || !adam_id.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("Не указан корректный идентификатор App Store".into());
    }
    let offset = input.offset as usize;
    let limit = if input.limit == 0 {
        ACCOUNT_VERSION_PICKER_LIMIT
    } else {
        (input.limit as usize).clamp(1, 40)
    };
    let _guard = state.ipatool_lock.lock().await;
    ensure_active_ipatool_session(&app).await?;
    if DEMO_MODE {
        if adam_id == HAPP_APP_STORE_ID {
            return Ok(AppStoreVersionsPage {
                versions: Vec::new(),
                has_more: false,
                total: 0,
                offset,
                region_blocked: false,
            });
        }
        let versions = demo_app_store_versions();
        let total = versions.len();
        return Ok(AppStoreVersionsPage {
            versions,
            has_more: false,
            total,
            offset: 0,
            region_blocked: false,
        });
    }

    let page_key = format!("{adam_id}:{offset}:{limit}");
    if input.refresh {
        state.version_page_cache.lock().unwrap().retain(|key, _| !key.starts_with(&format!("{adam_id}:")));
    } else if let Some(page) = state.version_page_cache.lock().unwrap().get(&page_key).cloned() {
        return Ok(page);
    }
    let cached = if input.refresh {
        state.version_probe_cache.lock().unwrap().remove(adam_id);
        None
    } else {
        state
            .version_picker_cache
            .lock()
            .unwrap()
            .get(adam_id)
            .cloned()
    };
    let list = if let Some(list) = cached {
        list
    } else {
        let loaded = load_owned_version_ids(&app, adam_id).await?;
        state
            .version_picker_cache
            .lock()
            .unwrap()
            .insert(adam_id.to_string(), loaded.clone());
        loaded
    };
    persist_account_catalog_cache(&app);

    let total = list.ids.len();
    if total == 0 {
        return Ok(AppStoreVersionsPage {
            versions: Vec::new(),
            has_more: false,
            total: 0,
            offset,
            region_blocked: list.region_blocked,
        });
    }
    let start = offset.min(total);
    let end = (start + limit).min(total);
    let page_ids = list.ids[start..end].to_vec();
    let has_more = end < total;
    let mut versions = label_version_page(&app, adam_id, &list, page_ids, false).await;
    if start == 0 && !versions.iter().any(|item| item.is_latest) {
        if let Some(first) = versions.first_mut() {
            first.is_latest = true;
        }
    }
    let page = AppStoreVersionsPage {
        versions,
        has_more,
        total,
        offset: start,
        region_blocked: list.region_blocked,
    };
    state.version_page_cache.lock().unwrap().insert(page_key, page.clone());
    persist_account_catalog_cache(&app);
    Ok(page)
}

/// Keep the latest plus the newest remaining external IDs, newest first.
fn take_recent_version_ids(
    ids: Vec<String>,
    latest: Option<&str>,
    limit: usize,
) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    if let Some(latest) = latest.map(str::trim).filter(|value| !value.is_empty()) {
        if seen.insert(latest.to_string()) {
            out.push(latest.to_string());
        }
    }
    let mut rest: Vec<(u64, String)> = ids
        .into_iter()
        .filter(|id| {
            let id = id.trim();
            !id.is_empty() && seen.insert(id.to_string())
        })
        .map(|id| {
            let key = id.trim().parse::<u64>().unwrap_or(0);
            (key, id.trim().to_string())
        })
        .collect();
    rest.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (_, id) in rest {
        if out.len() >= limit {
            break;
        }
        out.push(id);
    }
    out
}

/// Numeric version parts for `4.9` / `5.12.0` / `v1.0.1`.
fn marketing_version_parts(version: &str) -> Vec<u64> {
    let cleaned = version.trim().trim_start_matches(['v', 'V']);
    // Pure long digit string = external id, not marketing version.
    if cleaned.chars().all(|c| c.is_ascii_digit()) && cleaned.len() >= 8 {
        return Vec::new();
    }
    cleaned
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

/// Descending compare: 5.12.0 > 4.9. Unknown/external-id-like versions sort last.
fn cmp_marketing_version_desc(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let pa = marketing_version_parts(a);
    let pb = marketing_version_parts(b);
    match (pa.is_empty(), pb.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater, // a unknown → after b
        (false, true) => Ordering::Less,    // b unknown → a first
        (false, false) => {
            let len = pa.len().max(pb.len());
            for i in 0..len {
                let x = pa.get(i).copied().unwrap_or(0);
                let y = pb.get(i).copied().unwrap_or(0);
                match y.cmp(&x) {
                    // desc: larger first
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            Ordering::Equal
        }
    }
}

fn version_external_id(version: &JsonValue) -> Option<String> {
    version
        .get("external_version_id")
        .or_else(|| version.get("externalVersionId"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn version_available_on_account(version: &JsonValue, external_ids: &[String]) -> bool {
    // Empty identifier list means the account does not own this App Store ID.
    if external_ids.is_empty() {
        return false;
    }
    if let Some(required) = version_external_id(version) {
        return external_ids.iter().any(|value| value == &required);
    }
    // Catalog cover apps usually omit external_version_id. A successful non-empty
    // list-versions response for that exact App Store ID/bundle proves ownership.
    true
}

/// An empty string passes `all(is_ascii_digit)`, but is not a CLI app ID.
fn normalized_app_store_id(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|id| {
        !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Key for the list-versions cache: adamId or bundle (same App Store listing).
fn version_probe_cache_key(bundle_id: &str, app_store_id: Option<&str>) -> String {
    normalized_app_store_id(app_store_id)
        .map(|id| id.to_string())
        .unwrap_or_else(|| format!("bundle:{}", bundle_id.trim()))
}

/// The same key built straight from a catalog row.
fn catalog_version_probe_cache_key(version: &JsonValue) -> String {
    let bundle_id = version
        .get("bundle_id")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let app_store_id = version.get("app_store_id").and_then(|value| value.as_str());
    version_probe_cache_key(bundle_id, app_store_id)
}

/// `Some(true)` owned, `Some(false)` definitely not, `None` could not be
/// checked (broken session, 2FA, Apple silent). The three are not the same
/// thing to a user, so they must not collapse into one boolean.
/// Below this the ipatool cookie jar holds nothing usable — a valid one is
/// kilobytes, a broken one is a couple of bytes of empty JSON.
const MIN_STORE_COOKIE_BYTES: u64 = 16;

/// Signed in, but with no Store cookies to send.
///
/// `auth info` keeps answering with the account name, so nothing looks wrong,
/// while every ownership question comes back empty. Treat that as "cannot
/// tell", never as "not owned".
fn ipatool_session_looks_broken() -> bool {
    let Some(dir) = ipatool_data_dir() else {
        return false;
    };
    let signed_in = dir.join("account").is_file() || dir.join("keyring").join("account").is_file();
    if !signed_in {
        return false;
    }
    fs::metadata(dir.join("cookies"))
        .map(|meta| meta.len())
        .unwrap_or(0)
        < MIN_STORE_COOKIE_BYTES
}

async fn find_app_id_on_current_account(app: &AppHandle, app_store_id: &str) -> Result<bool, String> {
    require_auth(app).map_err(|_| "Приложение не найдено в AppMagic и iTunes. Войдите в Apple ID, чтобы проверить его в покупках.".to_string())?;
    let state = app.state::<AppState>();
    let _guard = state.ipatool_lock.lock().await;
    ensure_active_ipatool_session(app).await?;
    probe_version_on_account_for_catalog_version(app, "", Some(app_store_id), &JsonValue::Null)
        .await
        .ok_or_else(|| "Не удалось проверить приложение в покупках Apple ID. Повторите поиск.".to_string())
}

async fn probe_version_on_account_for_catalog_version(
    app: &AppHandle,
    bundle_id: &str,
    app_store_id: Option<&str>,
    version: &JsonValue,
) -> Option<bool> {
    if DEMO_MODE {
        let happ_hit = app_store_id.map(|id| id.trim() == HAPP_APP_STORE_ID).unwrap_or(false)
            || bundle_id.trim().eq_ignore_ascii_case(DEMO_HAPP_BUNDLE_ID);
        return Some(!happ_hit);
    }
    let cache_key = version_probe_cache_key(bundle_id, app_store_id);

    // Reuse list-versions external IDs; re-evaluate per catalog row (external_version_id).
    {
        let state = app.state::<AppState>();
        let cache = state.version_probe_cache.lock().unwrap();
        if let Some(ids) = cache.get(&cache_key) {
            return Some(version_available_on_account(version, ids));
        }
    }

    let tools = match resolve_tool_paths(app) {
        Ok(value) => value,
        Err(_) => return None,
    };
    let keychain = match resolve_ipatool_keychain(app) {
        Ok(value) => value,
        Err(_) => return None,
    };
    let anisette = resolve_store_sap_helper(app);
    if !ipatool_signs_store_requests(&tools.ipatool)
        && apple_store::helper_available(anisette.as_deref())
    {
        if let Some(email) = active_account_email(app) {
            let mut adam = normalized_app_store_id(app_store_id)
                .map(|s| s.to_string());
            if adam.is_none() && !bundle_id.trim().is_empty() {
                adam = apple_store::lookup_adam_id(bundle_id).await;
            }
            if let Some(adam_id) = adam {
                match apple_store::list_versions(anisette.as_deref(), &email, &adam_id).await {
                    Ok(native) if !native.external_ids.is_empty() => {
                        let ids = native.external_ids;
                        let result = version_available_on_account(version, &ids);
                        {
                            let state = app.state::<AppState>();
                            let mut cache = state.version_probe_cache.lock().unwrap();
                            cache.insert(cache_key, ids);
                        }
                        return Some(result);
                    }
                    _ => {}
                }
            }
        }
    }
    // Match IPA_Downloader: `ipatool list-versions -i <id>` (+ keychain when encrypted).
    // Avoid --non-interactive here; keep only passphrase. Prefer App Store ID over bundle.
    let mut args = Vec::new();
    append_ipatool_keychain_args(&mut args, &keychain);
    args.push("list-versions".into());
    if let Some(id) = normalized_app_store_id(app_store_id) {
        args.extend(["-i".into(), (*id).to_string()]);
    } else if !bundle_id.trim().is_empty() {
        args.extend(["-b".into(), bundle_id.trim().to_string()]);
    } else {
        return None;
    }
    let ipatool = tools.ipatool.clone();
    let timeout = Duration::from_secs(ACCOUNT_VERSION_PROBE_TIMEOUT_SECS);
    let output = match run_process_raw_error(&ipatool, &args, timeout, None).await {
        Ok(output) => output,
        Err(error) if is_cookie_lock_error(&error) => {
            cleanup_stale_cookie_lock(true);
            // Brief backoff so parallel workers do not all re-hit the lock together.
            tokio::time::sleep(Duration::from_millis(40 + (fastrand_u64() % 120))).await;
            match run_process_raw_error(&ipatool, &args, timeout, None).await {
                Ok(output) => output,
                Err(error) => error, // raw message still parseable (empty songList, etc.)
            }
        }
        // ipatool exits 1 on empty songList — still parse the message.
        Err(error) => error,
    };
    // None = ipatool could not answer (broken session, 2FA, keychain). That is
    // NOT proof the Apple ID lacks the app, so it must not be cached as a
    // negative — otherwise one hiccup hides an owned app until restart.
    let Some(ids) = parse_ipatool_list_versions(&output) else {
        // Отдельная ветка: «срок входа истёк» — не загадка, а названная Apple
        // причина. Молча вернуть «не выяснили» значит оставить человека с
        // пустым списком версий и без единой подсказки, что делать.
        if is_password_token_expired_error(&output) {
            note_session_expired(app, "опрос версий");
        }
        ipatool_debug_log(&format!(
            "version-probe inconclusive key={cache_key} {}",
            technical_excerpt(&output, 300)
        ));
        return None;
    };
    // An empty answer from a session with no Store cookies proves nothing: the
    // login still reports success, yet Apple returns an empty songList for
    // every app, including ones the Apple ID certainly owns. Saying «not in
    // your purchases» there sends people looking for the wrong problem.
    if ids.is_empty() && ipatool_session_looks_broken() {
        ipatool_debug_log(&format!("version-probe empty on broken session key={cache_key}"));
        return None;
    }
    // «License was granted but download was blocked» is Apple confirming the
    // purchase in the same breath it refuses the listing: the app is owned, it
    // simply is not in this storefront. Reading that as "not owned" made Signum
    // move the Apple ID to another country for an app that already downloads,
    // so answer the ownership question with the licence and leave the
    // per-version question unanswered — only the latest build is reachable.
    if ids.is_empty() && is_region_locked_owned_app_error(&output) {
        ipatool_debug_log(&format!(
            "version-probe owned but storefront-blocked key={cache_key}"
        ));
        return match version_external_id(version) {
            Some(_) => None,
            None => Some(true),
        };
    }
    let result = version_available_on_account(version, &ids);

    {
        let state = app.state::<AppState>();
        let mut cache = state.version_probe_cache.lock().unwrap();
        cache.insert(cache_key, ids);
    }
    Some(result)
}

fn fastrand_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}

/// Returns the owned rows plus how many could not be checked at all, so the
/// UI can tell "you do not own this" apart from "we could not ask Apple".
///
/// `stop_after` caps the work: opening a card only needs the build to install
    /// immediately, while a catalog
/// entry can carry seventy covers. Probing all of them cost a spawn each and is
/// what made the offer sheet sit there thinking.
async fn available_catalog_versions_on_account(
    app: &AppHandle,
    versions: &[JsonValue],
    exclude_primary: bool,
    stop_after: Option<usize>,
) -> (Vec<JsonValue>, usize) {
    // Drop a stale lock once before a large batch (e.g. T-Bank 70+ covers).
    cleanup_stale_cookie_lock(true);

    // Probe every catalog row (Store + REMOVED). Only those with a non-empty
    // list-versions / owned license are returned — never dump the whole catalog.
    let candidates: Vec<_> = sorted_catalog_versions(versions)
        .into_iter()
        .filter(|version| !exclude_primary || !version_is_primary_catalog_offer(version, versions))
        .cloned()
        .collect();

    // All bundled engines share one cookie jar. Serialize probes so concurrent
    // truncating writers cannot corrupt the session between requests.
    let probe_concurrency = ACCOUNT_VERSION_PROBE_CONCURRENCY;

    // Один раз до серии опросов убеждаемся, что у аккаунта есть KBSync.
    // Пересоздавать готовый blob здесь нельзя (это и подвешивало каталог раз в
    // час), а вот пустой файл аккаунта надо заполнить заранее: иначе каждый
    // list-versions порождает blob сам и на медленной машине не успевает.
    if !candidates.is_empty() {
        ensure_kbsync_present(app).await;
    }

    let mut owned: Vec<JsonValue> = Vec::new();
    let mut unchecked = 0usize;
    let mut probed = 0usize;
    for chunk in candidates.chunks(probe_concurrency) {
        // Человек нажал «Выйти» или входит в другой Apple ID. Команда ждёт
        // общий замок, который держит эта проверка, а ответы про прежний
        // аккаунт уже никому не нужны: остаток помечаем непроверенным и
        // отпускаем замок, не доходя до конца списка (по 75 с на строку).
        if kbsync_cancel_requested() {
            unchecked += candidates.len() - probed;
            ipatool_debug_log(&format!(
                "version-probe остановлен: меняется вход в Apple ID, не проверено {}",
                candidates.len() - probed
            ));
            break;
        }
        probed += chunk.len();
    let chunk_concurrency = probe_concurrency;
    let results = stream::iter(chunk.to_vec().into_iter().enumerate())
        .map(|(index, version)| async move {
            // Stagger starts slightly so 6 workers do not all spawn at once.
            if index > 0 && chunk_concurrency > 1 {
                tokio::time::sleep(Duration::from_millis(
                    ((index % chunk_concurrency) as u64) * 35,
                ))
                .await;
            }
            let bundle_id = version
                .get("bundle_id")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let app_store_id = version
                .get("app_store_id")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            if bundle_id.is_empty() && app_store_id.is_none() {
                return (None, false);
            }
            match probe_version_on_account_for_catalog_version(
                app,
                &bundle_id,
                app_store_id.as_deref(),
                &version,
            )
            .await
            {
                Some(true) => (Some((index, version)), false),
                Some(false) => (None, false),
                None => (None, true),
            }
        })
        .buffer_unordered(probe_concurrency)
        .collect::<Vec<_>>()
        .await;

    unchecked += results.iter().filter(|(_, missed)| *missed).count();
    let mut hits: Vec<(usize, JsonValue)> =
        results.into_iter().filter_map(|(hit, _)| hit).collect();

    // Parallel probes finish in arbitrary order; restore the catalog order so
    // callers still prefer the newest matching version.
    hits.sort_by_key(|(index, _)| *index);
    owned.extend(hits.into_iter().map(|(_, version)| version));
    if stop_after.is_some_and(|want| owned.len() >= want) {
        break;
    }
    }
    (owned, unchecked)
}

/// Re-apply fresh catalog metadata (icon, title, min_ios, …) onto cached owned
/// rows so «Обновить» updates icons/names without a full re-probe.
fn refresh_owned_catalog_metadata(
    owned: &[JsonValue],
    catalog_versions: &[JsonValue],
) -> Vec<JsonValue> {
    owned
        .iter()
        .map(|cached| {
            let identity = catalog_version_identity(cached);
            if identity.is_empty() {
                return cached.clone();
            }
            catalog_versions
                .iter()
                .find(|version| catalog_version_identity(version) == identity)
                .cloned()
                .unwrap_or_else(|| cached.clone())
        })
        .collect()
}

/// Drop the cached list-versions answers for one app's catalog rows.
fn forget_version_probe_cache(state: &AppState, versions: &[JsonValue]) {
    let mut cache = state.version_probe_cache.lock().unwrap();
    let mut pickers = state.version_picker_cache.lock().unwrap();
    let mut pages = state.version_page_cache.lock().unwrap();
    for version in versions {
        let key = catalog_version_probe_cache_key(version);
        cache.remove(&key);
        pickers.remove(&key);
        pages.retain(|page_key, _| !page_key.starts_with(&format!("{key}:")));
    }
}

/// Probe once per app_id (unless force), reuse for offer + «другие версии».
///
/// `full` decides how much work this is allowed to do. The offer sheet asks for
/// the quick answer and its partial result is deliberately **not** cached, so
/// opening «другие версии» later still gets the complete list — by then most
/// rows are already answered from the per-build probe cache.
async fn probe_owned_catalog_versions_cached(
    app: &AppHandle,
    state: &AppState,
    app_id: &str,
    versions: &[JsonValue],
    force: bool,
    full: bool,
) -> (Vec<JsonValue>, usize) {
    if force {
        // Clearing account_version_cache alone is not enough: the probe below
        state.account_version_cache.lock().unwrap().remove(app_id);
        // answers from version_probe_cache first, so «Обновить список» would
        // replay the old verdict and an app bought after the first probe would
        // stay invisible until the whole catalog is reloaded.
        forget_version_probe_cache(state, versions);
    } else if let Some(cached) = get_cached_account_versions(state, app_id) {
        // Keep ownership result, but always merge latest icons/titles from API.
        return (refresh_owned_catalog_metadata(&cached, versions), 0);
    }
    // Always probe the full restore set (no exclude_primary) so the cache can
    // serve both the primary offer and the version picker.
    let stop_after = if full { None } else { Some(1) };
    let (owned, unchecked) =
        available_catalog_versions_on_account(app, versions, false, stop_after).await;
    if full && unchecked == 0 {
        set_cached_account_versions(state, app_id, owned.clone());
    }
    persist_account_catalog_cache(app);
    // Return metadata-fresh copies (owned may still equal catalog rows).
    (refresh_owned_catalog_metadata(&owned, versions), unchecked)
}

fn owned_versions_to_restore_items(
    app_id: &str,
    info: &JsonValue,
    app_title: &str,
    owned: &[JsonValue],
    device_ios: Option<&str>,
    exclude: Option<&JsonValue>,
    installed: &HashMap<String, InstalledApp>,
) -> Vec<AccountRestoreVersion> {
    let exclude_id = exclude.map(catalog_version_identity).unwrap_or_default();
    let mut items = Vec::new();
    let mut seen = HashSet::<String>::new();
    for version in owned {
        let identity = catalog_version_identity(version);
        if !exclude_id.is_empty() && identity == exclude_id {
            continue;
        }
        if !identity.is_empty() && !seen.insert(identity) {
            continue;
        }
        if let Some(item) =
            catalog_version_to_restore_item(app_id, info, version, app_title, device_ios, installed)
        {
            items.push(item);
        }
    }
    // Keep catalog/version order. Installed apps stay put — only show «Установлено» badge.
    items.sort_by(|left, right| {
        compare_versions(&right.version, &left.version)
            .cmp(&0)
            .then(left.title.cmp(&right.title))
    });
    items
}

#[derive(Serialize)]
struct CatalogInstallOffer {
    app_id: String,
    app_title: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    version: String,
    bundle_id: String,
    app_store_id: Option<String>,
    min_ios: Option<String>,
    icon_url: Option<String>,
    in_store: bool,
    source: String,
    message: String,
    show_account_versions: bool,
    /// Already present on the connected iPhone (same bundle_id).
    #[serde(default)]
    installed_on_device: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_version: Option<String>,
    /// None = unknown (no device / no min_ios).
    #[serde(skip_serializing_if = "Option::is_none")]
    device_compatible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_ios: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveCatalogInput {
    app_id: String,
    #[serde(default)]
    restore_only: bool,
}

fn catalog_has_restore_versions(versions: &[JsonValue]) -> bool {
    versions
        .iter()
        .any(|version| !version_is_store_offer(version))
}

/// True if catalog has another archived build besides the selected one.
/// Multiple live App Store variants are offers, not restore alternatives.
fn catalog_has_alternative_versions(versions: &[JsonValue], selected: &JsonValue) -> bool {
    let selected_identity = catalog_version_identity(selected);
    if selected_identity.is_empty() {
        return versions.len() > 1;
    }
    versions.iter().any(|version| {
        let id = catalog_version_identity(version);
        !version_is_store_offer(version) && !id.is_empty() && id != selected_identity
    })
}

fn best_store_catalog_version<'a>(versions: &'a [JsonValue]) -> Option<&'a JsonValue> {
    sorted_catalog_versions(versions)
        .into_iter()
        .find(|version| version_is_store_offer(version))
}

fn best_compatible_store_version<'a>(
    versions: &'a [JsonValue],
    device_ios: Option<&str>,
) -> Option<&'a JsonValue> {
    sorted_catalog_versions(versions)
        .into_iter()
        .filter(|version| version_is_store_offer(version))
        .filter(|version| {
            let min_ios = version.get("min_ios").and_then(|value| value.as_str());
            device_supports_min_ios(device_ios, min_ios).unwrap_or(true)
        })
        .next()
}

fn best_restore_catalog_version<'a>(
    versions: &'a [JsonValue],
    device_ios: Option<&str>,
) -> Option<&'a JsonValue> {
    let restore: Vec<&JsonValue> = sorted_catalog_versions(versions)
        .into_iter()
        .filter(|version| !version_is_store_offer(version))
        .collect();
    restore
        .iter()
        .copied()
        .find(|version| {
            device_supports_min_ios(device_ios, version.get("min_ios").and_then(|v| v.as_str()))
                != Some(false)
                && (version
                    .get("app_store_id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| normalized_app_store_id(Some(id)).is_some())
                    || version
                        .get("bundle_id")
                        .and_then(|v| v.as_str())
                        .is_some_and(|b| !b.trim().is_empty() && b != "NONE"))
        })
        .or_else(|| restore.first().copied())
}

fn catalog_version_identity(version: &JsonValue) -> String {
    if let Some(app_store_id) = version
        .get("app_store_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return format!("store:{app_store_id}");
    }
    let bundle_id = version
        .get("bundle_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("");
    if !bundle_id.is_empty() {
        return format!("bundle:{bundle_id}");
    }
    version
        .get("version")
        .and_then(|value| value.as_str())
        .map(|value| format!("version:{value}"))
        .unwrap_or_default()
}

fn primary_catalog_version<'a>(versions: &'a [JsonValue]) -> Option<&'a JsonValue> {
    best_store_catalog_version(versions).or_else(|| best_restore_catalog_version(versions, None))
}

fn version_is_primary_catalog_offer(version: &JsonValue, versions: &[JsonValue]) -> bool {
    primary_catalog_version(versions).is_some_and(|primary| {
        catalog_version_identity(version) == catalog_version_identity(primary)
    })
}

fn version_description(version: &JsonValue) -> Option<String> {
    version
        .get("description")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn build_catalog_offer(
    app_id: &str,
    app: &JsonValue,
    version: &JsonValue,
    source: &str,
    message: &str,
) -> Result<CatalogInstallOffer, String> {
    let bundle_id = version
        .get("bundle_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if bundle_id.is_empty() {
        return Err("В каталоге отсутствует bundle_id".into());
    }
    Ok(CatalogInstallOffer {
        app_id: app_id.to_string(),
        app_title: app
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("Приложение")
            .to_string(),
        title: version
            .get("title")
            .or_else(|| version.get("label"))
            .and_then(|value| value.as_str())
            .or_else(|| app.get("title").and_then(|value| value.as_str()))
            .unwrap_or("Приложение")
            .to_string(),
        description: version_description(version),
        version: version
            .get("version")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        bundle_id,
        app_store_id: version
            .get("app_store_id")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        min_ios: version
            .get("min_ios")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        icon_url: version
            .get("icon_url")
            .or_else(|| app.get("icon_url"))
            .and_then(|value| value.as_str())
            .map(str::to_string),
        in_store: version_is_store_offer(version),
        source: source.into(),
        message: message.into(),
        show_account_versions: false,
        installed_on_device: false,
        installed_version: None,
        device_compatible: None,
        device_ios: None,
    })
}

/// Cache-bust the app icon and every version icon in place.
/// Cache-buster appended to every catalogue icon URL.
///
/// This used to be the current millisecond, which made each URL unique per
/// load: the WebView re-fetched all catalogue icons from the CDN every time the
/// page opened, and no image ever came from cache. A day-granular token still
/// clears a replaced icon within a day — the reason busting exists — while
/// letting the same day's loads hit cache.
fn icon_cache_bust() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400).to_string())
        .unwrap_or_else(|_| "0".into())
}

fn bust_catalog_icons(info: &mut JsonValue, bust: &str) {
    let Some(obj) = info.as_object_mut() else {
        return;
    };
    if let Some(icon) = obj.get("icon_url").and_then(|v| v.as_str()).map(str::to_string) {
        obj.insert("icon_url".into(), JsonValue::String(cache_bust_url(&icon, bust)));
    }
    if let Some(JsonValue::Array(versions)) = obj.get_mut("versions") {
        for version in versions.iter_mut() {
            if let Some(vobj) = version.as_object_mut() {
                if let Some(icon) =
                    vobj.get("icon_url").and_then(|v| v.as_str()).map(str::to_string)
                {
                    vobj.insert("icon_url".into(), JsonValue::String(cache_bust_url(&icon, bust)));
                }
            }
        }
    }
}

/// Every catalogue app's versions in one round trip.
///
/// The per-app form of this call used to run in a sequential loop over the whole
/// catalogue. The payload was never the problem — the request count was: ~210
/// apps at ~0.28s of edge latency each is about a minute before the page shows
/// anything. `info.php?ids=` answers all of them at once.
async fn fetch_catalog_versions_batch(
    app: &AppHandle,
    app_ids: &[String],
) -> Result<HashMap<String, (JsonValue, Vec<JsonValue>)>, String> {
    let mut out: HashMap<String, (JsonValue, Vec<JsonValue>)> = HashMap::new();
    if app_ids.is_empty() {
        return Ok(out);
    }
    let bust = icon_cache_bust();

    // The server caps one request at 400 ids; stay under it and keep the URL sane.
    for chunk in app_ids.chunks(150) {
        let joined = chunk.join(",");
        let value = api_get(app, "info.php", &[("ids", joined.as_str())]).await?;
        let Some(map) = value.get("data").and_then(|v| v.as_object()) else {
            continue;
        };
        for (id, payload) in map {
            if !json_bool(value.get("_signum_from_cache"), false) {
                if let Ok(path) = catalog_response_path(app, "info.php", &[("id", id.as_str())]) {
                    let _ = save_catalog_response(&path, &serde_json::json!({"ok": true, "data": payload}));
                }
            }
            let mut info = payload.clone();
            bust_catalog_icons(&mut info, &bust);
            let versions = info
                .get("versions")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            // An app with no installable version is simply skipped here; the
            // single-app path still reports that as an error to the user.
            if !versions.is_empty() {
                out.insert(id.clone(), (info, versions));
            }
        }
    }
    Ok(out)
}

async fn fetch_catalog_versions(
    app: &AppHandle,
    app_id: &str,
) -> Result<(JsonValue, Vec<JsonValue>), String> {
    let value = api_get(app, "info.php", &[("id", app_id)]).await?;
    let mut info = value.get("data").cloned().unwrap_or(value);
    let bust = icon_cache_bust();
    // Bust CDN/WebView image cache for app + every version icon.
    bust_catalog_icons(&mut info, &bust);

    let versions = info
        .get("versions")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if versions.is_empty() {
        return Err("В каталоге нет версий для установки".into());
    }
    Ok((info, versions))
}

fn device_supports_min_ios(device_ios: Option<&str>, min_ios: Option<&str>) -> Option<bool> {
    match (device_ios, min_ios) {
        (Some(device), Some(required))
            if !device.trim().is_empty() && !required.trim().is_empty() =>
        {
            Some(compare_versions(device, required) >= 0)
        }
        _ => None,
    }
}

async fn installed_apps_map(app: &AppHandle) -> HashMap<String, InstalledApp> {
    match list_installed_apps_inner(app).await {
        Ok(apps) => apps
            .into_iter()
            .filter(|item| !item.is_placeholder)
            .map(|item| (item.bundle_id.clone(), item))
            .collect(),
        Err(_) => HashMap::new(),
    }
}

fn installed_lookup(
    installed: &HashMap<String, InstalledApp>,
    bundle_id: &str,
) -> (bool, Option<String>) {
    if bundle_id.is_empty() || bundle_id == "NONE" {
        return (false, None);
    }
    match installed.get(bundle_id) {
        Some(app) => (true, app.version.clone()),
        None => (false, None),
    }
}

fn decorate_offer_device_status(
    offer: &mut CatalogInstallOffer,
    installed: &HashMap<String, InstalledApp>,
    device_ios: Option<&str>,
) {
    let (on_device, installed_version) = installed_lookup(installed, &offer.bundle_id);
    offer.installed_on_device = on_device;
    offer.installed_version = installed_version;
    offer.device_ios = device_ios.map(str::to_string);
    offer.device_compatible = device_supports_min_ios(device_ios, offer.min_ios.as_deref());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountRestoreVersion {
    app_id: String,
    app_title: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    version: String,
    bundle_id: String,
    app_store_id: Option<String>,
    min_ios: Option<String>,
    icon_url: Option<String>,
    in_store: bool,
    device_compatible: Option<bool>,
    /// True if this bundle_id is already on the connected iPhone.
    #[serde(default)]
    installed_on_device: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_version: Option<String>,
}

#[derive(Serialize)]
struct AccountRestoreScan {
    device_ios: Option<String>,
    items: Vec<AccountRestoreVersion>,
    /// When items is empty, explains why (e.g. empty songList / session).
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingJobRecord {
    id: String,
    kind: String,
    title: String,
    subtitle: String,
    #[serde(default)]
    catalog_input: Option<CatalogInstallInput>,
    #[serde(default)]
    local_path: Option<String>,
    #[serde(default)]
    sign_input: Option<apple_signing::SignAndInstallInput>,
    created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActionHistoryEntry {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_udid: Option<String>,
    title: String,
    subtitle: String,
    status: String,
    kind: String,
    finished_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppCache {
    #[serde(default)]
    known_app_ids: Vec<String>,
    /// Apps signed and installed from here, so the signature can be renewed.
    #[serde(default)]
    signed_apps: Vec<SignedAppRecord>,
    /// Catalog app ids already auto-purchased (legacy; prefer store ids).
    #[serde(default)]
    auto_purchased_app_ids: Vec<String>,
    /// App Store ids already auto-purchased (check.php?type=auto_purchase keys).
    #[serde(default)]
    auto_purchased_store_ids: Vec<String>,
    /// Last fingerprint from check.php?type=auto_purchase (seed / diff).
    #[serde(default)]
    auto_purchase_fingerprint: Option<String>,
    #[serde(default)]
    auth_email: Option<String>,
    #[serde(default)]
    apple_accounts: Vec<AppleAccount>,
    #[serde(default)]
    active_account_email: Option<String>,
    /// Last successfully confirmed App Store country per Apple ID.
    #[serde(default)]
    store_region_cache: Vec<StoreRegionCacheEntry>,
    #[serde(default)]
    pending_jobs: Vec<PendingJobRecord>,
    #[serde(default)]
    action_history: Vec<ActionHistoryEntry>,
    /// Engine ("v2"/"v3") that actually wrote the session on this PC.
    /// Runtime fact, NOT a user choice: settings.ipatool_engine stays whatever
    /// the user picked (including "auto"). Used so a later `auth info` reads the
    /// same store the login wrote (v2 = keyring/account, v3 = account).
    #[serde(default)]
    active_ipatool_engine: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogGroup {
    id: String,
    title: String,
    #[serde(default)]
    priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogBanner {
    id: String,
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    style: String,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    action_label: Option<String>,
    #[serde(default)]
    priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CatalogFeed {
    apps: Vec<JsonValue>,
    groups: Vec<CatalogGroup>,
    banners: Vec<CatalogBanner>,
    #[serde(default)]
    from_cache: bool,
    #[serde(default)]
    cached_at: Option<u64>,
}

fn catalog_version_to_restore_item(
    app_id: &str,
    info: &JsonValue,
    version: &JsonValue,
    app_title: &str,
    device_ios: Option<&str>,
    installed: &HashMap<String, InstalledApp>,
) -> Option<AccountRestoreVersion> {
    let bundle_id = version
        .get("bundle_id")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let app_store_id = version
        .get("app_store_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "NONE");
    if (bundle_id.is_empty() || bundle_id == "NONE") && app_store_id.is_none() {
        return None;
    }
    let min_ios = version
        .get("min_ios")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (installed_on_device, installed_version) = installed_lookup(installed, bundle_id);
    Some(AccountRestoreVersion {
        app_id: app_id.to_string(),
        app_title: app_title.to_string(),
        title: version
            .get("title")
            .or_else(|| version.get("label"))
            .and_then(|value| value.as_str())
            .or_else(|| info.get("title").and_then(|value| value.as_str()))
            .unwrap_or("Приложение")
            .to_string(),
        description: version_description(version),
        version: version
            .get("version")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        bundle_id: bundle_id.to_string(),
        app_store_id: app_store_id.map(str::to_string),
        min_ios: min_ios.clone(),
        icon_url: version
            .get("icon_url")
            .or_else(|| info.get("icon_url"))
            .and_then(|value| value.as_str())
            .map(str::to_string),
        in_store: version_is_store_offer(version),
        device_compatible: device_supports_min_ios(device_ios, min_ios.as_deref()),
        installed_on_device,
        installed_version,
    })
}

/// Only versions from the catalog that ipatool `list-versions` confirms on this Apple ID.
async fn probe_account_restore_versions(
    app: &AppHandle,
    app_id: &str,
    info: &JsonValue,
    versions: &[JsonValue],
    app_title: &str,
    device_ios: Option<&str>,
    exclude_primary: bool,
    include_store: bool,
) -> Vec<AccountRestoreVersion> {
    let mut items = Vec::new();
    // Ownership via list-versions -i (same as IPA_Downloader version picker source).
    let (mut available, _unchecked) =
        available_catalog_versions_on_account(app, versions, exclude_primary, None).await;
    if include_store {
        for version in sorted_catalog_versions(versions) {
            if !version_is_store_offer(version) {
                continue;
            }
            if exclude_primary && version_is_primary_catalog_offer(version, versions) {
                continue;
            }
            let bundle_id = version
                .get("bundle_id")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            let app_store_id = version.get("app_store_id").and_then(|value| value.as_str());
            if probe_version_on_account_for_catalog_version(app, bundle_id, app_store_id, version)
                .await
                .unwrap_or(false)
            {
                available.push(version.clone());
            }
        }
    }

    let installed = installed_apps_map(app).await;
    let mut seen = HashSet::<String>::new();
    for version in available {
        let identity = catalog_version_identity(&version);
        if !identity.is_empty() && !seen.insert(identity) {
            continue;
        }
        if let Some(item) = catalog_version_to_restore_item(
            app_id, info, &version, app_title, device_ios, &installed,
        ) {
            items.push(item);
        }
    }

    // Keep version order. Do not push installed builds to the bottom.
    items.sort_by(|left, right| {
        compare_versions(&right.version, &left.version)
            .cmp(&0)
            .then(left.title.cmp(&right.title))
    });
    items
}

fn ipatool_session_store_ready() -> bool {
    current_ipatool_session_exists()
}

async fn probe_account_restore_catalog_inner(
    app: &AppHandle,
) -> Result<AccountRestoreScan, String> {
    require_auth(app)?;
    ensure_active_ipatool_session(app).await?;
    let device_ios = get_device_info_inner(app)
        .await
        .ok()
        .and_then(|device| device.product_version);
    let catalog = fetch_catalog(app.clone()).await?;
    let mut items = Vec::new();

    // One request for the whole catalogue instead of one per app in sequence.
    let app_ids: Vec<String> = catalog
        .iter()
        .filter_map(|item| item.get("id").and_then(|v| v.as_str()))
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    let mut details = fetch_catalog_versions_batch(app, &app_ids).await?;

    for app_item in catalog {
        let app_id = app_item
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if app_id.is_empty() {
            continue;
        }
        // Apps the batch skipped have nothing installable — same outcome as the
        // old per-app call erroring out, minus aborting the whole scan.
        let Some((info, versions)) = details.remove(&app_id) else {
            continue;
        };
        let app_title = catalog_item_title(&app_item);
        items.extend(
            probe_account_restore_versions(
                app,
                &app_id,
                &info,
                &versions,
                &app_title,
                device_ios.as_deref(),
                false,
                false,
            )
            .await,
        );
    }

    items.sort_by(|left, right| {
        let left_rank = i8::from(left.device_compatible.unwrap_or(false));
        let right_rank = i8::from(right.device_compatible.unwrap_or(false));
        right_rank
            .cmp(&left_rank)
            .then(left.app_title.cmp(&right.app_title))
            .then(compare_versions(&right.version, &left.version).cmp(&0))
    });

    Ok(AccountRestoreScan {
        device_ios,
        items,
        message: None,
    })
}

#[tauri::command]
async fn probe_account_restore_catalog(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AccountRestoreScan, String> {
    let _guard = state.ipatool_lock.lock().await;
    probe_account_restore_catalog_inner(&app).await
}

/// One app from full Apple ID purchase history (reportaproblem / bot `/history`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseHistoryApp {
    adam_id: String,
    name: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    artwork: String,
    #[serde(default)]
    price: String,
    #[serde(default)]
    date: String,
    /// Resolved via iTunes lookup when possible.
    #[serde(default)]
    bundle_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseHistoryResult {
    email: String,
    count: usize,
    apps: Vec<PurchaseHistoryApp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    /// Apple asked for 2FA on reportaproblem — send the same request with two_factor_code.
    #[serde(default)]
    needs_2fa: bool,
    /// More Apple batches remain — UI shows «Загрузить ещё».
    #[serde(default)]
    has_more: bool,
    /// Pass back as `batchId` on the next «Загрузить ещё» call.
    #[serde(skip_serializing_if = "Option::is_none")]
    next_batch_id: Option<String>,
    /// RAP cookie session is stored (no 2FA needed for next pages / reopen).
    #[serde(default)]
    session_saved: bool,
    /// Result came from local disk cache (instant reopen).
    #[serde(default)]
    from_cache: bool,
    /// Temporary HTTP debug log path (for diagnosing history login).
    #[serde(skip_serializing_if = "Option::is_none")]
    debug_log_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchPurchaseHistoryInput {
    /// Active Apple ID email (optional — defaults to signed-in account).
    email: Option<String>,
    /// Password for reportaproblem. Empty → use encrypted secret in history-secrets/.
    #[serde(default)]
    password: Option<String>,
    /// 6-digit Apple 2FA code. Accept camelCase (twoFactorCode) and snake_case.
    #[serde(default, alias = "two_factor_code")]
    two_factor_code: Option<String>,
    /// Explicit user action: resend SMS/push for history 2FA (uses pending session when possible).
    #[serde(default, alias = "resend_code")]
    resend_code: Option<bool>,
    /// Fetch next Apple batch using saved session + cache cursor (no 2FA).
    #[serde(default, alias = "load_more")]
    load_more: Option<bool>,
    /// Cursor from previous `nextBatchId` (preferred over disk cache).
    #[serde(default, alias = "batch_id")]
    batch_id: Option<String>,
    /// Ignore local apps cache and re-fetch first page from Apple (still uses RAP cookies).
    #[serde(default)]
    force_refresh: Option<bool>,
    /// Drain all remaining cursor pages in one sequential backend request.
    #[serde(default, alias = "load_all")]
    load_all: Option<bool>,
}

/// Resolve CFBundleIdentifier for an App Store adamId.
/// Tries several storefronts — delisted/region apps often return empty without `country`.
async fn itunes_lookup_bundle_id(adam_id: &str) -> Option<String> {
    let adam_id = adam_id.trim();
    if adam_id.is_empty() || !adam_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .ok()?;
    // RU first (common for this product), then defaults / other markets.
    for country in ["ru", "us", "kz", "by", "ua", "gb", "de", ""] {
        let url = if country.is_empty() {
            format!("https://itunes.apple.com/lookup?id={adam_id}&entity=software")
        } else {
            format!(
                "https://itunes.apple.com/lookup?id={adam_id}&country={country}&entity=software"
            )
        };
        let Ok(resp) = client.get(&url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(value) = resp.json::<JsonValue>().await else {
            continue;
        };
        if let Some(bundle) = value
            .get("results")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|app| app.get("bundleId"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return Some(bundle);
        }
    }
    None
}

fn is_placeholder_bundle_id(bundle_id: &str) -> bool {
    let b = bundle_id.trim();
    b.is_empty() || b.eq_ignore_ascii_case("NONE") || b.starts_with("app.store.")
}

/// Human-readable job subtitle: real bundle, else adamId (never show app.store.*).
fn catalog_job_subtitle(input: &CatalogInstallInput) -> String {
    let b = input.bundle_id.trim();
    if !is_placeholder_bundle_id(b) {
        return b.to_string();
    }
    if let Some(id) = input
        .app_store_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return id.to_string();
    }
    if b.starts_with("app.store.") {
        return b.trim_start_matches("app.store.").to_string();
    }
    b.to_string()
}

/// Fill real bundle id via iTunes when we only have adamId / synthetic placeholder.
async fn resolve_catalog_bundle_id(input: &mut CatalogInstallInput) {
    if !is_placeholder_bundle_id(&input.bundle_id) {
        return;
    }
    let Some(adam) = input
        .app_store_id
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
    else {
        return;
    };
    if let Some(bundle) = itunes_lookup_bundle_id(&adam).await {
        input.bundle_id = bundle;
    }
}

/// Full Apple ID purchase history — pure HTTP (no browser).
#[tauri::command]
async fn fetch_purchase_history(
    app: AppHandle,
    state: State<'_, AppState>,
    input: FetchPurchaseHistoryInput,
) -> Result<PurchaseHistoryResult, String> {
    if DEMO_MODE {
        let email = input
            .email
            .as_ref()
            .map(|s| normalize_account_email(s))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(demo_email);
        let apps = demo_history_apps();
        return Ok(PurchaseHistoryResult {
            count: apps.len(),
            email,
            apps,
            message: None,
            needs_2fa: false,
            has_more: false,
            next_batch_id: None,
            session_saved: true,
            from_cache: false,
            debug_log_path: None,
        });
    }
    let email = input
        .email
        .as_ref()
        .map(|s| normalize_account_email(s))
        .filter(|s| !s.is_empty())
        .or_else(|| active_account_email(&app))
        .ok_or_else(|| "Сначала войдите в Apple ID.".to_string())?;

    let settings = load_settings(&app).unwrap_or_else(|_| {
        default_settings(&app).unwrap_or(AppSettings {
            background_enabled: false,
        proxy_enabled: false,
            proxy_kind: default_proxy_kind(),
            proxy_host: String::new(),
            proxy_port: 0,
            proxy_user: String::new(),
            proxy_password: String::new(),
            downloads_dir: String::new(),
            exports_dir: String::new(),
            theme: "dark".into(),
            auto_update: true,
            auto_purchase_new_apps: false,
            lite_design: false,
            detailed_errors: false,
            auto_diag: true,
            lite_static_loaders: true,
            lite_reduce_motion: true,
            lite_simple_scrolling: true,
            lite_disable_blur: true,
            lite_disable_shadows: true,
            lite_flat_backgrounds: true,
            ipatool_keychain_mode: default_ipatool_keychain_mode(),
            ipatool_keychain_passphrase: None,
            selected_device_udid: None,
            selected_device_connection: None,
            keep_history_session: true,
            ipatool_engine: default_ipatool_engine(),
        favorite_app_ids: Vec::new(),
        auto_resign_before_expiry: false,
        })
    });
    let keep_session = settings.keep_history_session;
    let detailed = settings.detailed_errors;

    let provided = input.password.as_deref().unwrap_or("").trim().to_string();

    let two_factor = input
        .two_factor_code
        .as_ref()
        .map(|s| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
        .unwrap_or_default();
    let two_factor = if two_factor.len() >= 6 {
        Some(two_factor.chars().take(6).collect::<String>())
    } else {
        None
    };
    let resend = input.resend_code.unwrap_or(false);
    let load_more = input.load_more.unwrap_or(false);
    let force_refresh = input.force_refresh.unwrap_or(false);
    let load_all = input.load_all.unwrap_or(false);
    let client_batch = input
        .batch_id
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Serialize with ipatool ops so we don't hammer Apple from two places.
    let _guard = state.ipatool_lock.lock().await;

    // ---- Explicit resend of history 2FA (user button only) ----
    if resend && two_factor.is_none() {
        let pending = {
            let slot = state
                .history_2fa_pending
                .lock()
                .map_err(|_| "Внутренняя ошибка. Перезапустите Signum.".to_string())?;
            slot.clone()
        };
        if let Some(p) = pending {
            if p.matches_email(&email) && !p.is_expired() {
                match apple_history::resend_history_2fa(p).await {
                    Ok(new_pending) => {
                        if let Ok(mut slot) = state.history_2fa_pending.lock() {
                            *slot = Some(new_pending);
                        }
                        let debug_path = if detailed {
                            apple_history::last_debug_log_path()
                        } else {
                            None
                        };
                        let session_saved = history_secrets::has_history_session(&email);
                        return Ok(PurchaseHistoryResult {
                            email,
                            count: 0,
                            apps: vec![],
                            message: Some("Введите 6-значный код с iPhone.".into()),
                            needs_2fa: true,
                            has_more: false,
                            next_batch_id: None,
                            session_saved,
                            from_cache: false,
                            debug_log_path: debug_path,
                        });
                    }
                    Err(e) => {
                        if let Ok(mut slot) = state.history_2fa_pending.lock() {
                            *slot = None;
                        }
                        // Fall through to full login to obtain a fresh 2FA challenge.
                        let _ = e;
                    }
                }
            }
        }
        // No living pending → full SRP below will request a fresh code once.
    }

    // ---- Instant reopen: local apps cache (no network, no 2FA) ----
    // `load_all` is an explicit pagination action. It must bypass this early
    // return and continue at Path 0 with the persisted nextBatchId.
    if two_factor.is_none() && !resend && !load_more && !load_all && !force_refresh {
        if let Some(cache) = history_secrets::load_history_apps_cache(&email) {
            if !cache.apps.is_empty() {
                let result = history_result_from_cache(email.clone(), cache, detailed);
                // Best-effort: replace app.store.* / empty with real bundle ids via iTunes.
                return Ok(enrich_history_cache_bundle_ids(&email, result).await);
            }
        }
    }

    // ---- Path 0a: ipatool list-purchases (DAAP) — ОТКЛЮЧЁН ----
    // DAAP-метод медленнее RAP-сессии и не возвращает обложки (artwork пустой).
    // Старый idmsa-метод ниже остаётся основным. Код в apple_history.rs сохранён;
    // чтобы вернуть DAAP первым — поставь true здесь.
    const HISTORY_VIA_IPATOOL: bool = false;
    if HISTORY_VIA_IPATOOL && two_factor.is_none() && !resend && !load_more && !load_all {
        let via_ipatool = (|| async {
            let tools = resolve_tool_paths(&app).ok()?;
            let keychain = resolve_ipatool_keychain(&app).ok()?;
            if !current_ipatool_session_exists() {
                ipatool_debug_log("history-ipatool skip: no ipatool session");
                return None;
            }
            let debug = |msg: &str| ipatool_debug_log(msg);
            apple_history::fetch_history_via_ipatool(&tools.ipatool, &keychain, &debug).await
        })()
        .await;
        if let Some(apps) = via_ipatool {
            if !apps.is_empty() {
                ipatool_debug_log(&format!(
                    "history-ipatool ok count={} — using DAAP history",
                    apps.len()
                ));
                let success = apple_history::HistorySuccess {
                    apps,
                    cookies: HashMap::new(),
                    next_batch_id: None,
                    has_more: false,
                    rap_dsid: None,
                    rap_xsrf: None,
                };
                return finalize_purchase_history_result(
                    email,
                    success,
                    detailed,
                    false,
                    false,
                )
                .await;
            }
        }
    }

    // ---- Path 0: reuse saved RAP session (no password / no 2FA) ----
    // Skip cookie reuse when user explicitly asked to resend 2FA.
    if two_factor.is_none() && keep_session && !resend {
        if let Some(cookies) = history_secrets::load_history_session(&email) {
            let cache_snap = history_secrets::load_history_apps_cache(&email);
            // Cursor priority: client batchId → disk cache.
            let start_batch = if load_more || load_all {
                client_batch
                    .clone()
                    .or_else(|| cache_snap.as_ref().and_then(|c| c.next_batch_id.clone()))
            } else {
                None
            };
            if (load_more || load_all) && start_batch.is_none() {
                // No cursor → mark end of list (do NOT re-fetch page 1).
                if let Some(mut cache) = cache_snap {
                    cache.has_more = false;
                    cache.next_batch_id = None;
                    let _ = history_secrets::store_history_apps_cache(&cache);
                    return Ok(history_result_from_cache(email, cache, detailed));
                }
                return Err("Больше страниц нет.".into());
            }
            // Prefer living XSRF/dsid from the last successful RAP login (pagination).
            let rap_creds = cache_snap.as_ref().and_then(|c| {
                let d = c.rap_dsid.as_ref()?.trim();
                let x = c.rap_xsrf.as_ref()?.trim();
                if d.is_empty() || x.is_empty() {
                    None
                } else {
                    Some((d.to_string(), x.to_string()))
                }
            });
            match apple_history::fetch_history_with_cookies(
                &cookies,
                if load_all {
                    usize::MAX
                } else {
                    apple_history::HISTORY_PAGE_BATCHES
                },
                start_batch,
                rap_creds,
            )
            .await
            {
                Ok(success) => {
                    let _ = history_secrets::store_history_session(&email, &success.cookies);
                    return finalize_purchase_history_result(
                        email,
                        success,
                        detailed,
                        load_more || load_all,
                        false,
                    )
                    .await;
                }
                Err(e) => {
                    // Cookie session rejected (often 400 UNAUTHORIZED).
                    if load_more || load_all {
                        return Err(format!("Не удалось подгрузить ещё: {}", e.user_message()));
                    }
                    if force_refresh {
                        // Explicit «Обновить»: fall through to password re-auth
                        // (may ask for 2FA once). Soft reopen still uses cache below.
                    } else if let Some(cache) = history_secrets::load_history_apps_cache(&email) {
                        if !cache.apps.is_empty() {
                            // Keep cookies for a later retry; list stays available offline.
                            return Ok(history_result_from_cache(email, cache, detailed));
                        }
                    }
                    // No cache / force_refresh → password re-auth below.
                    let _ = e;
                }
            }
        } else if load_more || load_all {
            // No cookies but maybe cache + cursor from client
            if client_batch.is_some() {
                return Err(
                    "Сессия истории не сохранена. Нажмите «Обновить» или введите код один раз."
                        .into(),
                );
            }
            return Err("Больше страниц нет.".into());
        }
    }

    // force_refresh with living cache but dead cookies already returned above.
    // First-time / explicit code: password path.

    let password = history_secrets::resolve_history_password(&email, &provided)?;

    // Clone pending for 2FA resume — do NOT drop until success / hard failure.
    // Critical: if a code is already pending and the client reloads without a code,
    // keep the same idmsa session (do NOT re-SRP — that would request a NEW Apple code).
    let pending = {
        let mut slot = state
            .history_2fa_pending
            .lock()
            .map_err(|_| "Внутренняя ошибка. Перезапустите Signum.".to_string())?;
        if two_factor.is_some() {
            slot.clone()
        } else if let Some(p) = slot.as_ref() {
            if p.matches_email(&email) && !p.is_expired() {
                let session_saved = history_secrets::has_history_session(&email);
                return Ok(PurchaseHistoryResult {
                    email,
                    count: 0,
                    apps: vec![],
                    message: Some("Введите 6-значный код с iPhone.".into()),
                    needs_2fa: true,
                    has_more: false,
                    next_batch_id: None,
                    session_saved,
                    from_cache: false,
                    debug_log_path: None,
                });
            }
            *slot = None;
            None
        } else {
            None
        }
    };

    let result =
        apple_history::fetch_purchase_history(&email, &password, two_factor.as_deref(), pending)
            .await;

    let debug_path = if detailed {
        apple_history::last_debug_log_path()
    } else {
        None
    };

    let success = match result {
        Ok(apple_history::HistoryOutcome::Apps(success)) => {
            if let Ok(mut slot) = state.history_2fa_pending.lock() {
                *slot = None;
            }
            // Always persist password (even when keep_history_session is off).
            let _ = history_secrets::store_history_password(&email, &password);
            if keep_session {
                let _ = history_secrets::store_history_session(&email, &success.cookies);
            } else {
                // Cookies only — keep password + apps cache.
                history_secrets::delete_history_session_cookies_only(&email);
            }
            success
        }
        Ok(apple_history::HistoryOutcome::Needs2fa { pending }) => {
            if let Ok(mut slot) = state.history_2fa_pending.lock() {
                *slot = Some(pending);
            }
            // Password always saved so 2FA retry / later loads work without retype.
            let _ = history_secrets::store_history_password(&email, &password);
            let session_saved = history_secrets::has_history_session(&email);
            return Ok(PurchaseHistoryResult {
                email,
                count: 0,
                apps: vec![],
                message: Some("Введите 6-значный код с iPhone.".into()),
                needs_2fa: true,
                has_more: false,
                next_batch_id: None,
                session_saved,
                from_cache: false,
                debug_log_path: debug_path,
            });
        }
        Err(e) => {
            if e.keep_pending() {
                // Keep the same idmsa pending session — do NOT re-SRP.
                // Window stays open; Apple did not get a brand-new code request from us.
                let session_saved = history_secrets::has_history_session(&email);
                return Ok(PurchaseHistoryResult {
                    email,
                    count: 0,
                    apps: vec![],
                    message: Some(e.user_message()),
                    needs_2fa: true,
                    has_more: false,
                    next_batch_id: None,
                    session_saved,
                    from_cache: false,
                    debug_log_path: debug_path,
                });
            }
            if e.clear_pending() {
                if let Ok(mut slot) = state.history_2fa_pending.lock() {
                    *slot = None;
                }
            }
            if matches!(e, apple_history::HistoryError::WrongPassword) {
                history_secrets::delete_all_history_secrets(&email);
            }
            if matches!(e, apple_history::HistoryError::SessionExpired) {
                // Drop cookies only — keep password + local apps list.
                history_secrets::delete_history_session_cookies_only(&email);
            }
            let mut msg = e.user_message();
            if detailed {
                if let Some(p) = &debug_path {
                    if !msg.contains("Лог") {
                        msg = format!("{msg}\n\nЛог: {p}");
                    }
                }
            }
            return Err(msg);
        }
    };

    finalize_purchase_history_result(email, success, detailed, false, false).await
}

fn region_account_email(app: &AppHandle, explicit: Option<&str>) -> Result<String, String> {
    explicit
        .map(|s| normalize_account_email(s))
        .filter(|s| !s.is_empty())
        .or_else(|| active_account_email(app))
        .ok_or_else(|| "Сначала войдите в Apple ID.".to_string())
}

fn take_region_pending(
    state: &AppState,
    email: &str,
    two_factor: Option<&str>,
) -> Result<Option<apple_history::PendingHistoryAuth>, String> {
    let mut slot = state
        .region_2fa_pending
        .lock()
        .map_err(|_| "Внутренняя ошибка. Перезапустите Signum.".to_string())?;
    if two_factor.is_some() {
        return Ok(slot.clone());
    }
    if let Some(p) = slot.as_ref() {
        if p.matches_email(email) && !p.is_expired() {
            return Ok(slot.clone());
        }
    }
    *slot = None;
    Ok(None)
}

async fn resend_region_code(state: &AppState, email: &str) -> Result<(), String> {
    let pending = state
        .region_2fa_pending
        .lock()
        .map_err(|_| "Внутренняя ошибка. Перезапустите Signum.".to_string())?
        .clone()
        .filter(|pending| pending.matches_email(email) && !pending.is_expired())
        .ok_or_else(|| "Сессия подтверждения истекла. Запросите вход ещё раз.".to_string())?;
    let updated = apple_history::resend_region_2fa(pending)
        .await
        .map_err(|error| error.user_message())?;
    let mut slot = state
        .region_2fa_pending
        .lock()
        .map_err(|_| "Внутренняя ошибка. Перезапустите Signum.".to_string())?;
    *slot = Some(updated);
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreRegionCacheInput {
    #[serde(default)]
    email: Option<String>,
}

#[tauri::command]
fn get_cached_store_region(
    app: AppHandle,
    input: StoreRegionCacheInput,
) -> Result<Option<StoreRegionCacheEntry>, String> {
    if demo_active() {
        let email = input
            .email
            .as_ref()
            .map(|s| normalize_account_email(s))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(demo_email);
        let (country_code, country_name) = demo_region();
        return Ok(Some(StoreRegionCacheEntry {
            email,
            country_code,
            country_name,
            timestamp: now(),
        }));
    }
    let email = region_account_email(&app, input.email.as_deref())?;
    Ok(cached_store_region_for_email(&app, &email))
}

#[tauri::command]
fn list_store_regions() -> Vec<apple_region::StoreRegionOption> {
    apple_region::list_store_regions()
}

#[tauri::command]
fn preview_store_region_address(country: String) -> Result<apple_region::GeneratedAddress, String> {
    apple_region::generate_address(&country)
}

/// Storefronts Signum can switch an Apple ID to, with the two-letter code the
/// public App Store lookup wants. Same set as `apple_region::list_store_regions`.
const STOREFRONT_CODES: &[(&str, &str)] = &[
    ("ALB", "al"), ("ARE", "ae"), ("ARG", "ar"), ("ARM", "am"), ("AUS", "au"),
    ("AZE", "az"), ("BLR", "by"), ("BRA", "br"), ("CAN", "ca"), ("CHL", "cl"),
    ("CHN", "cn"), ("COL", "co"), ("EGY", "eg"), ("GBR", "gb"), ("GEO", "ge"),
    ("HKG", "hk"), ("IDN", "id"), ("IND", "in"), ("JPN", "jp"), ("KAZ", "kz"),
    ("KEN", "ke"), ("KGZ", "kg"), ("KOR", "kr"), ("MDA", "md"), ("MEX", "mx"),
    ("MNG", "mn"), ("MYS", "my"), ("NGA", "ng"), ("NZL", "nz"), ("PAN", "pa"),
    ("PER", "pe"), ("PHL", "ph"), ("RUS", "ru"), ("SGP", "sg"), ("SRB", "rs"),
    ("THA", "th"), ("TUR", "tr"), ("TWN", "tw"), ("UKR", "ua"), ("USA", "us"),
    ("UZB", "uz"), ("VNM", "vn"), ("ZAF", "za"),
];

/// Tried in this order when the app has to be fetched from somewhere else, so
/// the account lands in a large, ordinary storefront rather than a random one.
const STOREFRONT_PREFERENCE: &[&str] = &[
    "USA", "GBR", "TUR", "KAZ", "ARM", "GEO", "AZE", "UZB", "IND", "BRA",
];

const STOREFRONT_PROBE_CONCURRENCY: usize = 8;

fn storefront_iso2(iso3: &str) -> Option<&'static str> {
    STOREFRONT_CODES
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(iso3))
        .map(|(_, iso2)| *iso2)
}

fn storefront_name(iso3: &str) -> String {
    apple_region::list_store_regions()
        .into_iter()
        .find(|option| option.code.eq_ignore_ascii_case(iso3))
        .map(|option| option.name)
        .unwrap_or_else(|| iso3.to_string())
}

fn itunes_lookup_client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    match CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|e| e.to_string())
    }) {
        Ok(client) => Ok(client),
        Err(error) => Err(error.clone()),
    }
}

/// Is this app sold in that storefront at all? Public lookup, no Apple ID.
async fn app_sold_in_storefront(adam_id: &str, iso2: &str) -> bool {
    let Ok(client) = itunes_lookup_client() else {
        return false;
    };
    let url = format!("https://itunes.apple.com/lookup?id={adam_id}&country={iso2}");
    let Ok(response) = client.get(&url).send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    response
        .json::<JsonValue>()
        .await
        .ok()
        .and_then(|body| body.get("resultCount").and_then(|v| v.as_u64()))
        .is_some_and(|count| count > 0)
}

/// Which storefronts sell this app, skipping the one the account is already in.
///
/// Kept out of the command body on purpose: an async closure over a stream
/// inside a `#[tauri::command]` trips the lifetime checker.
async fn storefronts_selling(adam_id: String, skip_iso3: String) -> Vec<String> {
    let candidates: Vec<(String, String)> = STOREFRONT_CODES
        .iter()
        .filter(|(iso3, _)| !iso3.eq_ignore_ascii_case(&skip_iso3))
        .map(|(iso3, iso2)| ((*iso3).to_string(), (*iso2).to_string()))
        .collect();
    stream::iter(candidates)
        .map(|(iso3, iso2)| {
            let adam_id = adam_id.clone();
            async move { app_sold_in_storefront(&adam_id, &iso2).await.then_some(iso3) }
        })
        .buffer_unordered(STOREFRONT_PROBE_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// Emails whose storefront we already tried to read from Apple this run. The
/// lookup costs a round trip and stays useless until the user finishes the
/// Account page, so one failed attempt per session is enough.
fn storefront_probe_attempted() -> &'static StdMutex<HashSet<String>> {
    static TRIED: OnceLock<StdMutex<HashSet<String>>> = OnceLock::new();
    TRIED.get_or_init(|| StdMutex::new(HashSet::new()))
}

/// Storefront this Apple ID lives in, as an ISO-3 code.
///
/// Cached after the Account page reads it. When the cache is empty we ask the
/// saved commerce session — the very session a region switch would need — so a
/// user who can switch never has to tell Signum where they live.
async fn account_storefront(app: &AppHandle, email: &str) -> Option<String> {
    if let Some(entry) = cached_store_region_for_email(app, email) {
        let code = entry.country_code.trim().to_ascii_uppercase();
        if !code.is_empty() {
            return Some(code);
        }
    }
    {
        let mut tried = storefront_probe_attempted().lock().ok()?;
        if !tried.insert(normalize_account_email(email)) {
            return None;
        }
    }
    let address = apple_region::snapshot_current_region(email).await.ok()?;
    let code = address.country_code.trim().to_ascii_uppercase();
    if code.is_empty() {
        return None;
    }
    let _ = cache_store_region(app, email, &code, &address.country_name);
    Some(code)
}

/// The account's own storefront, but only when it does NOT sell this app — the
/// one case where borrowing another country can help. `None` means "nothing to
/// do": the app is on sale where the account lives, the id is not a store id,
/// or Signum cannot read the country and so could not switch either.
///
/// One lookup in the common case; the 43-storefront sweep is left to
/// `storefront_selling_preferred`, which only runs once this returns `Some`.
async fn storefront_missing_app(app: &AppHandle, email: &str, adam_id: &str) -> Option<String> {
    if adam_id.is_empty() || !adam_id.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let account_region = account_storefront(app, email).await?;
    let iso2 = storefront_iso2(&account_region)?;
    if app_sold_in_storefront(adam_id, iso2).await {
        return None;
    }
    Some(account_region)
}

/// A storefront that sells this app, preferring large ordinary ones over
/// whatever answers first, so the account does not land somewhere obscure.
async fn storefront_selling_preferred(adam_id: &str, skip_iso3: &str) -> Option<String> {
    let found = storefronts_selling(adam_id.to_string(), skip_iso3.to_string()).await;
    STOREFRONT_PREFERENCE
        .iter()
        .find(|wanted| found.iter().any(|iso3| iso3.eq_ignore_ascii_case(wanted)))
        .map(|wanted| (*wanted).to_string())
        .or_else(|| found.first().cloned())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreRegionPlan {
    /// owned — уже в покупках, ничего не делать.
    /// purchase — продаётся в текущей стране (или страну прочитать не вышло),
    ///            просто привязать; переезд подберёт сама задача установки.
    /// switch — продаётся в другой стране, нужен временный переезд.
    /// unavailable — нигде не нашли.
    action: String,
    account_region: Option<String>,
    account_region_name: Option<String>,
    target_region: Option<String>,
    target_region_name: Option<String>,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreRegionPlanInput {
    app_store_id: String,
    /// The version list came back non-empty, which only happens for an app the
    /// Apple ID already owns — even when the storefront says it is unavailable.
    #[serde(default)]
    owned_versions: bool,
}

/// Decide whether installing this app needs a purchase, a temporary move to
/// another storefront, or nothing at all.
#[tauri::command]
async fn plan_store_region_for_app(
    app: AppHandle,
    input: StoreRegionPlanInput,
) -> Result<StoreRegionPlan, String> {
    require_auth(&app)?;
    let adam_id = input.app_store_id.trim().to_string();
    if adam_id.is_empty() || !adam_id.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("Не указан корректный идентификатор App Store".into());
    }

    // Owning it beats everything: the account can install a build it already
    // has, even from a storefront that no longer sells the app.
    if input.owned_versions {
        return Ok(StoreRegionPlan {
            action: "owned".into(),
            account_region: None,
            account_region_name: None,
            target_region: None,
            target_region_name: None,
            message: "Приложение уже в покупках этого Apple ID — регион менять не нужно.".into(),
        });
    }

    let email = active_account_email(&app);
    let account_region = match email.as_deref() {
        Some(email) => account_storefront(&app, email).await,
        None => None,
    };
    // Country unreadable: switching needs the same session we just failed to
    // get, so there is nothing to offer the user. Say what will be attempted
    // and let the install job report whatever Apple actually answers.
    let Some(account_region) = account_region else {
        return Ok(StoreRegionPlan {
            action: "purchase".into(),
            account_region: None,
            account_region_name: None,
            target_region: None,
            target_region_name: None,
            message: "Signum привяжет приложение к Apple ID. Если в вашей стране его нет, страна магазина переключится на время установки и вернётся обратно."
                .into(),
        });
    };
    let account_region_name = storefront_name(&account_region);

    if let Some(iso2) = storefront_iso2(&account_region) {
        if app_sold_in_storefront(&adam_id, iso2).await {
            return Ok(StoreRegionPlan {
                action: "purchase".into(),
                account_region: Some(account_region),
                account_region_name: Some(account_region_name),
                target_region: None,
                target_region_name: None,
                message: "Приложение продаётся в вашей стране — Signum просто привяжет его к Apple ID.".into(),
            });
        }
    }

    // Not on sale where the account lives: find a storefront that has it.
    let found = storefronts_selling(adam_id.clone(), account_region.clone()).await;

    let target = STOREFRONT_PREFERENCE
        .iter()
        .find(|wanted| found.iter().any(|iso3| iso3.eq_ignore_ascii_case(wanted)))
        .map(|wanted| (*wanted).to_string())
        .or_else(|| found.first().cloned());

    Ok(match target {
        Some(target) => {
            let target_name = storefront_name(&target);
            StoreRegionPlan {
                action: "switch".into(),
                account_region: Some(account_region),
                account_region_name: Some(account_region_name.clone()),
                target_region: Some(target),
                target_region_name: Some(target_name.clone()),
                message: format!(
                    "В стране «{account_region_name}» это приложение не продаётся, а в «{target_name}» есть.                      Signum временно переключит Apple ID туда, привяжет приложение и сразу вернёт вашу страну обратно."
                ),
            }
        }
        None => StoreRegionPlan {
            action: "unavailable".into(),
            account_region: Some(account_region),
            account_region_name: Some(account_region_name),
            target_region: None,
            target_region_name: None,
            message: "Это приложение сейчас не продаётся ни в одной из стран, на которые Signum умеет переключаться.".into(),
        },
    })
}

fn temporary_region_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("temporary-region.json"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemporaryRegionRecord {
    email: String,
    original: apple_region::GeneratedAddress,
}

fn save_temporary_region(
    app: &AppHandle,
    email: &str,
    original: &apple_region::GeneratedAddress,
) -> Result<(), String> {
    let record = TemporaryRegionRecord {
        email: normalize_account_email(email),
        original: original.clone(),
    };
    let raw = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    fs::write(temporary_region_path(app)?, raw).map_err(|e| e.to_string())
}

fn load_temporary_region(app: &AppHandle) -> Option<TemporaryRegionRecord> {
    let path = temporary_region_path(app).ok()?;
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

fn clear_temporary_region(app: &AppHandle) {
    if let Ok(path) = temporary_region_path(app) {
        let _ = fs::remove_file(path);
    }
}

async fn recover_temporary_region(app: AppHandle) {
    let Some(record) = load_temporary_region(&app) else {
        return;
    };
    let state = app.state::<AppState>();
    let _guard = state.ipatool_lock.lock().await;
    if apple_region::restore_temporary_region(&record.email, &record.original)
        .await
        .is_ok()
    {
        clear_temporary_region(&app);
    }
}

#[tauri::command]
fn list_store_region_addresses(
    country: String,
) -> Result<Vec<apple_region::GeneratedAddress>, String> {
    apple_region::list_region_addresses(&country)
}

#[tauri::command]
async fn get_store_region(
    app: AppHandle,
    state: State<'_, AppState>,
    input: apple_region::RegionQueryInput,
) -> Result<apple_region::StoreRegionStatus, String> {
    if demo_active() {
        let (code, name) = demo_region();
        return Ok(apple_region::StoreRegionStatus {
            country_code: code,
            country_name: name,
            needs_2fa: false,
            message: None,
        });
    }
    let email = region_account_email(&app, input.email.as_deref())?;
    let two_factor = input
        .two_factor_code
        .as_ref()
        .map(|s| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
        .filter(|s| s.len() >= 6);
    if input.resend_code.unwrap_or(false) {
        let _guard = state.ipatool_lock.lock().await;
        resend_region_code(&state, &email).await?;
        return Ok(apple_region::StoreRegionStatus {
            country_code: String::new(),
            country_name: String::new(),
            needs_2fa: true,
            message: Some("Новый код отправлен на доверенное устройство.".into()),
        });
    }
    let pending = take_region_pending(&state, &email, two_factor.as_deref())?;
    if two_factor.is_none() {
        if let Some(p) = pending.as_ref() {
            if p.matches_email(&email) && !p.is_expired() {
                return Ok(apple_region::StoreRegionStatus {
                    country_code: String::new(),
                    country_name: String::new(),
                    needs_2fa: true,
                    message: Some("Введите 6-значный код с iPhone.".into()),
                });
            }
        }
    }
    let _guard = state.ipatool_lock.lock().await;
    match apple_region::load_store_region(
        &email,
        input.password.as_deref(),
        two_factor.as_deref(),
        pending,
    )
    .await?
    {
        Ok(status) => {
            if let Ok(mut slot) = state.region_2fa_pending.lock() {
                *slot = None;
            }
            let _ = cache_store_region(&app, &email, &status.country_code, &status.country_name);
            Ok(status)
        }
        Err(pending) => {
            if let Ok(mut slot) = state.region_2fa_pending.lock() {
                *slot = Some(pending);
            }
            Ok(apple_region::StoreRegionStatus {
                country_code: String::new(),
                country_name: String::new(),
                needs_2fa: true,
                message: Some("Введите 6-значный код с iPhone.".into()),
            })
        }
    }
}

#[tauri::command]
async fn change_store_region(
    app: AppHandle,
    state: State<'_, AppState>,
    input: apple_region::ChangeRegionInput,
) -> Result<apple_region::ChangeRegionResult, String> {
    if demo_active() {
        let country = input.country.trim().to_ascii_uppercase();
        if country.len() != 3 {
            return Err("Выберите страну.".into());
        }
        let name = apple_region::list_store_regions()
            .into_iter()
            .find(|option| option.code == country)
            .map(|option| option.name)
            .unwrap_or_else(|| country.clone());
        demo_set_region(&country, &name);
        return Ok(apple_region::ChangeRegionResult {
            ok: true,
            needs_2fa: false,
            country_code: country,
            country_name: name,
            message: "Страна магазина обновлена.".into(),
        });
    }
    let email = region_account_email(&app, input.email.as_deref())?;
    let country = input.country.trim().to_ascii_uppercase();
    if country.len() != 3 {
        return Err("Выберите страну.".into());
    }
    let two_factor = input
        .two_factor_code
        .as_ref()
        .map(|s| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
        .filter(|s| s.len() >= 6);
    if input.resend_code.unwrap_or(false) {
        let _guard = state.ipatool_lock.lock().await;
        resend_region_code(&state, &email).await?;
        return Ok(apple_region::ChangeRegionResult {
            ok: false,
            needs_2fa: true,
            country_code: String::new(),
            country_name: String::new(),
            message: "Новый код отправлен на доверенное устройство.".into(),
        });
    }
    let pending = take_region_pending(&state, &email, two_factor.as_deref())?;
    if two_factor.is_none() {
        if let Some(p) = pending.as_ref() {
            if p.matches_email(&email) && !p.is_expired() {
                return Ok(apple_region::ChangeRegionResult {
                    ok: false,
                    needs_2fa: true,
                    country_code: String::new(),
                    country_name: String::new(),
                    message: "Введите 6-значный код с iPhone.".into(),
                });
            }
        }
    }
    let _guard = state.ipatool_lock.lock().await;
    match apple_region::apply_store_region(
        &email,
        &country,
        input.address,
        input.password.as_deref(),
        two_factor.as_deref(),
        pending,
    )
    .await?
    {
        Ok(result) => {
            if let Ok(mut slot) = state.region_2fa_pending.lock() {
                *slot = None;
            }
            if result.ok {
                let _ =
                    cache_store_region(&app, &email, &result.country_code, &result.country_name);
            }
            Ok(result)
        }
        Err(pending) => {
            if let Ok(mut slot) = state.region_2fa_pending.lock() {
                *slot = Some(pending);
            }
            Ok(apple_region::ChangeRegionResult {
                ok: false,
                needs_2fa: true,
                country_code: String::new(),
                country_name: String::new(),
                message: "Введите 6-значный код с iPhone.".into(),
            })
        }
    }
}

fn history_app_from_raw(a: apple_history::HistoryApp) -> PurchaseHistoryApp {
    PurchaseHistoryApp {
        adam_id: a.adam_id,
        name: a.name,
        artist: a.artist,
        artwork: a.artwork,
        price: a.price,
        date: a.date,
        bundle_id: String::new(),
    }
}

fn history_app_from_cached(a: history_secrets::CachedHistoryApp) -> PurchaseHistoryApp {
    PurchaseHistoryApp {
        adam_id: a.adam_id,
        name: a.name,
        artist: a.artist,
        artwork: a.artwork,
        price: a.price,
        date: a.date,
        bundle_id: a.bundle_id,
    }
}

fn cached_from_history_app(a: &PurchaseHistoryApp) -> history_secrets::CachedHistoryApp {
    history_secrets::CachedHistoryApp {
        adam_id: a.adam_id.clone(),
        name: a.name.clone(),
        artist: a.artist.clone(),
        artwork: a.artwork.clone(),
        price: a.price.clone(),
        date: a.date.clone(),
        bundle_id: a.bundle_id.clone(),
    }
}

fn history_result_from_cache(
    email: String,
    cache: history_secrets::HistoryAppsCache,
    detailed: bool,
) -> PurchaseHistoryResult {
    let next_batch_id = cache.next_batch_id.clone();
    let apps: Vec<PurchaseHistoryApp> = cache
        .apps
        .into_iter()
        .map(history_app_from_cached)
        .collect();
    let n = apps.len();
    let has_more = cache.has_more && next_batch_id.is_some();
    let session_saved = history_secrets::has_history_session(&email);
    PurchaseHistoryResult {
        email,
        count: n,
        apps,
        message: None,
        needs_2fa: false,
        has_more,
        next_batch_id: if has_more { next_batch_id } else { None },
        session_saved,
        from_cache: true,
        debug_log_path: if detailed {
            apple_history::last_debug_log_path()
        } else {
            None
        },
    }
}

/// Fill missing bundle ids in a cached history list (async iTunes), update disk cache.
async fn enrich_history_cache_bundle_ids(
    email: &str,
    mut result: PurchaseHistoryResult,
) -> PurchaseHistoryResult {
    let missing: Vec<String> = result
        .apps
        .iter()
        .filter(|a| is_placeholder_bundle_id(&a.bundle_id))
        .map(|a| a.adam_id.clone())
        .take(40)
        .collect();
    if missing.is_empty() {
        return result;
    }
    let mut map: HashMap<String, String> = HashMap::new();
    for chunk in missing.chunks(6) {
        let futs: Vec<_> = chunk
            .iter()
            .map(|id| {
                let id = id.clone();
                async move { (id.clone(), itunes_lookup_bundle_id(&id).await) }
            })
            .collect();
        for (id, bundle) in futures_util::future::join_all(futs).await {
            if let Some(b) = bundle {
                map.insert(id, b);
            }
        }
    }
    if map.is_empty() {
        return result;
    }
    for app in &mut result.apps {
        if is_placeholder_bundle_id(&app.bundle_id) {
            if let Some(b) = map.get(&app.adam_id) {
                app.bundle_id = b.clone();
                history_secrets::update_history_app_bundle(email, &app.adam_id, b);
            }
        }
    }
    result
}

async fn finalize_purchase_history_result(
    email: String,
    success: apple_history::HistorySuccess,
    detailed: bool,
    append: bool,
    _unused: bool,
) -> Result<PurchaseHistoryResult, String> {
    let mut page_apps: Vec<PurchaseHistoryApp> =
        success.apps.into_iter().map(history_app_from_raw).collect();

    // Resolve real CFBundleIdentifier for every app on this page (iTunes, multi-storefront).
    // adamId-only installs still work; this is for UI and cleaner job subtitles.
    let ids: Vec<String> = page_apps
        .iter()
        .filter(|a| a.bundle_id.trim().is_empty() || a.bundle_id.starts_with("app.store."))
        .map(|a| a.adam_id.clone())
        .collect();
    let mut map: HashMap<String, String> = HashMap::new();
    for chunk in ids.chunks(6) {
        let futs: Vec<_> = chunk
            .iter()
            .map(|id| {
                let id = id.clone();
                async move { (id.clone(), itunes_lookup_bundle_id(&id).await) }
            })
            .collect();
        for (id, bundle) in futures_util::future::join_all(futs).await {
            if let Some(b) = bundle {
                map.insert(id, b);
            }
        }
    }
    for app_row in &mut page_apps {
        if let Some(b) = map.get(&app_row.adam_id) {
            app_row.bundle_id = b.clone();
        }
    }

    let cached_page: Vec<history_secrets::CachedHistoryApp> =
        page_apps.iter().map(cached_from_history_app).collect();
    let next_id = success.next_batch_id.clone();
    let page_has_more =
        success.has_more && next_id.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
    let cache = history_secrets::merge_history_apps_cache_with_rap(
        &email,
        &cached_page,
        next_id.clone(),
        page_has_more,
        !append, // first page / refresh replaces; load_more appends
        success.rap_dsid.clone(),
        success.rap_xsrf.clone(),
    );

    let apps: Vec<PurchaseHistoryApp> = cache
        .apps
        .into_iter()
        .map(history_app_from_cached)
        .collect();
    let n = apps.len();
    let has_more = cache.has_more && cache.next_batch_id.is_some();
    let next_batch_id = cache.next_batch_id.clone();
    let session_saved = history_secrets::has_history_session(&email);

    Ok(PurchaseHistoryResult {
        email,
        count: n,
        apps,
        message: None,
        needs_2fa: false,
        has_more,
        next_batch_id: if has_more { next_batch_id } else { None },
        session_saved,
        from_cache: false,
        debug_log_path: if detailed {
            apple_history::last_debug_log_path()
        } else {
            None
        },
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeAccountRestoreAppInput {
    app_id: String,
    /// When true, ignore cache and re-run list-versions for this app only.
    #[serde(default)]
    force: bool,
    /// Build currently shown on the offer card — hidden from the list so the
    /// open app is not offered again as an «other version».
    #[serde(default)]
    current_app_store_id: Option<String>,
    #[serde(default)]
    current_bundle_id: Option<String>,
}

/// Locate the catalog row the offer card is showing. `primary_catalog_version`
/// is not a stand-in for it: the card usually shows the first *owned* build,
/// and excluding the wrong row lists the open app a second time.
fn current_catalog_version<'a>(
    versions: &'a [JsonValue],
    app_store_id: Option<&str>,
    bundle_id: Option<&str>,
) -> Option<&'a JsonValue> {
    let find = |key: &str, wanted: Option<&str>| {
        let wanted = wanted.map(str::trim).filter(|value| !value.is_empty())?;
        versions.iter().find(|version| {
            version
                .get(key)
                .and_then(|value| value.as_str())
                .map(str::trim)
                == Some(wanted)
        })
    };
    find("app_store_id", app_store_id).or_else(|| find("bundle_id", bundle_id))
}

#[tauri::command]
async fn probe_account_restore_for_app(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ProbeAccountRestoreAppInput,
) -> Result<AccountRestoreScan, String> {
    require_auth(&app)?;
    let app_id = input.app_id.trim().to_string();
    if app_id.is_empty() {
        return Err("Не указано приложение каталога".into());
    }
    let _guard = state.ipatool_lock.lock().await;
    ensure_active_ipatool_session(&app).await?;

    let device_ios = get_device_info_inner(&app)
        .await
        .ok()
        .and_then(|device| device.product_version);
    let (info, versions) = fetch_catalog_versions(&app, &app_id).await?;
    let app_title = info
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or("Приложение")
        .to_string();

    // Reuse the probe from card open (resolve_catalog_install). Only re-scan when force.
    // IMPORTANT: list only versions confirmed on this Apple ID (list-versions).
    // Never inject unowned REMOVED/catalog rows — that led to useless purchase attempts.
    let (owned, unchecked) =
        probe_owned_catalog_versions_cached(&app, &state, &app_id, &versions, input.force, true)
            .await;
    // Do not query USB here: the version list is account-only, and a USB
    // round-trip can steal the cable from the following install.
    let installed = HashMap::new();

    let primary = current_catalog_version(
        &versions,
        input.current_app_store_id.as_deref(),
        input.current_bundle_id.as_deref(),
    )
    .or_else(|| primary_catalog_version(&versions));
    let mut items = owned_versions_to_restore_items(
        &app_id,
        &info,
        &app_title,
        &owned,
        device_ios.as_deref(),
        primary,
        &installed,
    );

    items.sort_by(|left, right| {
        compare_versions(&right.version, &left.version)
            .cmp(&0)
            .then(left.title.cmp(&right.title))
    });

    // Three different situations used to read as one flat «нет в покупках».
    let message = if items.is_empty() {
        Some(if owned.is_empty() && unchecked > 0 {
            "Не удалось спросить Apple про покупки этого Apple ID.\n\n\
                 Это не значит, что приложения нет. Нажмите «Обновить список». \
                 Если не помогло — откройте «Аккаунт», выйдите и войдите снова."
                .to_string()
        } else if owned.is_empty() {
            "На этом Apple ID нет версий «{app}» в покупках.\n\n\
                 Показаны только приложения, которые уже покупались на аккаунте. \
                 Если покупали на другом Apple ID — войдите в него и нажмите «Обновить список»."
                .replace("{app}", &app_title)
        } else {
            "Других версий в покупках нет — на аккаунте только текущая.".into()
        })
    } else if unchecked > 0 {
        Some(format!(
            "Ещё {unchecked} — проверить не удалось, Apple не ответила. \
             Нажмите «Обновить список», чтобы попробовать снова."
        ))
    } else {
        None
    };

    Ok(AccountRestoreScan {
        device_ios,
        items,
        message,
    })
}

async fn find_restore_offer_on_account(
    app: &AppHandle,
    state: &AppState,
    app_id: &str,
    info: &JsonValue,
    versions: &[JsonValue],
    device_ios: Option<&str>,
    force_probe: bool,
    installed: &HashMap<String, InstalledApp>,
) -> Result<CatalogInstallOffer, String> {
    let app_title = info
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or("Приложение");

    // One probe for this app — reused by «другие версии».
    let (owned, _unchecked) =
        probe_owned_catalog_versions_cached(app, state, app_id, versions, force_probe, false).await;

    let finalize =
        |version: &JsonValue, source: &str, message: &str| -> Result<CatalogInstallOffer, String> {
            let mut offer = build_catalog_offer(app_id, info, version, source, message)?;
            let others = owned_versions_to_restore_items(
                app_id,
                info,
                app_title,
                &owned,
                device_ios,
                Some(version),
                installed,
            );
            // Only if the first probe found other owned builds (no empty second search UI).
            offer.show_account_versions = !others.is_empty()
                || catalog_has_alternative_versions(versions, version);
            decorate_offer_device_status(&mut offer, installed, device_ios);
            Ok(offer)
        };

    // 1) Prefer owned versions that fit the device iOS.
    for version in &owned {
        if device_supports_min_ios(
            device_ios,
            version.get("min_ios").and_then(|value| value.as_str()),
        ) == Some(false)
        {
            continue;
        }
        let bundle_id = version
            .get("bundle_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let app_store_id = version.get("app_store_id").and_then(|value| value.as_str());
        if bundle_id.is_empty() && app_store_id.is_none() {
            continue;
        }
        return finalize(version, "restore", "Версия найдена в покупках Apple ID.");
    }

    // 2) Owned but iOS too low — still allow install; UI warns the app won't launch.
    for version in &owned {
        let bundle_id = version
            .get("bundle_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let app_store_id = version.get("app_store_id").and_then(|value| value.as_str());
        if bundle_id.is_empty() && app_store_id.is_none() {
            continue;
        }
        let required = version
            .get("min_ios")
            .and_then(|value| value.as_str())
            .unwrap_or("более новую");
        return finalize(
            version,
            "restore",
            &format!(
                "Версия есть в покупках Apple ID, но требует iOS {required}. \
                 Установить можно, но на этом iPhone приложение не запустится."
            ),
        );
    }

    // Never use a catalog fallback here. A catalog row is only metadata and does
    // not prove that Apple will issue the IPA for this Apple ID.
    Err(format!(
        "На этом Apple ID нет доступных версий «{app_title}» в покупках. \
         Войдите в Apple ID, на котором приложение уже приобреталось, и повторите попытку."
    ))
}

#[tauri::command]
async fn resolve_catalog_install(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ResolveCatalogInput,
) -> Result<CatalogInstallOffer, String> {
    require_auth(&app)?;
    let app_id = input.app_id.trim().to_string();
    if app_id.is_empty() {
        return Err("Не указано приложение каталога".into());
    }
    let _guard = state.ipatool_lock.lock().await;
    ensure_active_ipatool_session(&app).await?;
    let (info, versions) = fetch_catalog_versions(&app, &app_id).await?;
    let device_ios = get_device_info_inner(&app)
        .await
        .ok()
        .and_then(|device| device.product_version);
    // Catalog must not hold USB (installed apps) — that raced the install job.
    let installed = HashMap::new();

    if input.restore_only {
        return find_restore_offer_on_account(
            &app,
            &state,
            &app_id,
            &info,
            &versions,
            device_ios.as_deref(),
            false,
            &installed,
        )
        .await;
    }

    // Happ is the only explicit acquire-on-demand flow: its dedicated UI may
    // temporarily switch to the US store and add the app before installing it.
    // Every other catalog app must first be confirmed in this Apple ID purchases.
    if app_id == "happ" {
        if let Some(version) = best_compatible_store_version(&versions, device_ios.as_deref()) {
            let message = "Доступна новая версия в App Store.";
            let mut offer = build_catalog_offer(&app_id, &info, version, "app_store", message)?;
            // Warm ownership cache; show «другие версии» only if other builds are on this Apple ID.
            let (owned, _unchecked) =
                probe_owned_catalog_versions_cached(&app, &state, &app_id, &versions, false, false)
                    .await;
            let others = owned_versions_to_restore_items(
                &app_id,
                &info,
                offer.app_title.as_str(),
                &owned,
                device_ios.as_deref(),
                Some(version),
                &installed,
            );
            offer.show_account_versions = !others.is_empty();
            decorate_offer_device_status(&mut offer, &installed, device_ios.as_deref());
            return Ok(offer);
        }
    }

    if app_id == "happ" {
        if let Some(version) = best_store_catalog_version(&versions) {
            let has_restore = catalog_has_restore_versions(&versions);
            if has_restore {
                if let Ok(mut offer) = find_restore_offer_on_account(
                    &app,
                    &state,
                    &app_id,
                    &info,
                    &versions,
                    device_ios.as_deref(),
                    false,
                    &installed,
                )
                .await
                {
                    // Prefer a compatible restore if one was found; keep device status.
                    if offer.device_compatible != Some(false) {
                        offer.message = "Версия из App Store не подходит под iOS устройства — выбрана совместимая версия из покупок.".into();
                        return Ok(offer);
                    }
                }
            }
            let required = version
                .get("min_ios")
                .and_then(|value| value.as_str())
                .unwrap_or("более новую");
            let message = format!(
                "Версия из App Store требует iOS {required}. Установить можно, \
             но на этом iPhone приложение не запустится. Можно выбрать другую версию."
            );
            let mut offer = build_catalog_offer(&app_id, &info, version, "app_store", &message)?;
            let (owned, _unchecked) =
                probe_owned_catalog_versions_cached(&app, &state, &app_id, &versions, false, false)
                    .await;
            let others = owned_versions_to_restore_items(
                &app_id,
                &info,
                offer.app_title.as_str(),
                &owned,
                device_ios.as_deref(),
                Some(version),
                &installed,
            );
            offer.show_account_versions = !others.is_empty();
            decorate_offer_device_status(&mut offer, &installed, device_ios.as_deref());
            return Ok(offer);
        }
    }

    // One account probe (cached) for the primary offer and the version list.
    // It returns an error instead of an unowned catalog fallback.
    find_restore_offer_on_account(
        &app,
        &state,
        &app_id,
        &info,
        &versions,
        device_ios.as_deref(),
        false,
        &installed,
    )
    .await
}

fn parse_catalog_groups(value: &JsonValue) -> Vec<CatalogGroup> {
    value
        .get("groups")
        .and_then(|v| v.as_array())
        .map(|list| {
            let mut groups: Vec<CatalogGroup> = list
                .iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(|v| v.as_str())?.trim();
                    if id.is_empty() {
                        return None;
                    }
                    Some(CatalogGroup {
                        id: id.to_string(),
                        title: item
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or(id)
                            .to_string(),
                        priority: item.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    })
                })
                .collect();
            groups.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.title.cmp(&b.title)));
            groups
        })
        .unwrap_or_else(|| {
            vec![
                CatalogGroup {
                    id: "sber".into(),
                    title: "Сбер".into(),
                    priority: 100,
                },
                CatalogGroup {
                    id: "tbank".into(),
                    title: "Т-Банк".into(),
                    priority: 90,
                },
                CatalogGroup {
                    id: "alfa".into(),
                    title: "Альфа".into(),
                    priority: 80,
                },
                CatalogGroup {
                    id: "gazprom".into(),
                    title: "Газпромбанк".into(),
                    priority: 70,
                },
                CatalogGroup {
                    id: "novikom".into(),
                    title: "Новикомбанк".into(),
                    priority: 60,
                },
                CatalogGroup {
                    id: "other".into(),
                    title: "Другие".into(),
                    priority: 10,
                },
            ]
        })
}

fn parse_catalog_banners(value: &JsonValue) -> Vec<CatalogBanner> {
    value
        .get("banners")
        .and_then(|v| v.as_array())
        .map(|list| {
            let mut banners: Vec<CatalogBanner> = list
                .iter()
                .filter(|item| json_bool(item.get("active"), true))
                .filter_map(|item| {
                    let id = item.get("id").and_then(|v| v.as_str())?.trim();
                    let title = item.get("title").and_then(|v| v.as_str())?.trim();
                    if id.is_empty() || title.is_empty() {
                        return None;
                    }
                    Some(CatalogBanner {
                        id: id.to_string(),
                        title: title.to_string(),
                        text: item
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        style: item
                            .get("style")
                            .and_then(|v| v.as_str())
                            .unwrap_or("blue")
                            .to_string(),
                        app_id: item
                            .get("app_id")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string),
                        action_label: item
                            .get("action_label")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string),
                        priority: item.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    })
                })
                .collect();
            banners.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.title.cmp(&b.title)));
            banners
        })
        .unwrap_or_default()
}

async fn enrich_catalog_apps(app: &AppHandle, list: &[JsonValue]) -> Vec<JsonValue> {
    let bust = icon_cache_bust();

    let mut items: Vec<JsonValue> = list
        .iter()
        .filter(|item| {
            item.get("id").is_some()
                && json_bool(item.get("active").or_else(|| item.get("is_active")), true)
        })
        .cloned()
        .collect();

    // Which cards the summary left incomplete. Filling these used to be one
    // info.php call per app, awaited in turn: a catalogue where the summary
    // omits a key cost one round trip per app before anything rendered.
    // `info.php?ids=` answers the whole set in a single request, so the cost is
    // flat no matter how many cards are short.
    let needs_info: Vec<String> = items
        .iter()
        .filter(|item| {
            let missing = |key: &str| {
                item.get(key)
                    .and_then(|value| value.as_str())
                    .is_none_or(|value| value.trim().is_empty())
            };
            missing("version") || missing("min_ios")
        })
        .filter_map(|item| item.get("id").and_then(|value| value.as_str()))
        .map(str::to_string)
        .collect();
    let extra = fetch_catalog_versions_batch(app, &needs_info)
        .await
        .unwrap_or_default();

    for item in &mut items {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        if let Some(icon) = obj
            .get("icon_url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        {
            obj.insert(
                "icon_url".into(),
                JsonValue::String(cache_bust_url(&icon, &bust)),
            );
        }
        let version_missing = obj
            .get("version")
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.trim().is_empty());
        let min_ios_missing = obj
            .get("min_ios")
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.trim().is_empty());
        if !version_missing && !min_ios_missing {
            continue;
        }
        let Some(app_id) = obj.get("id").and_then(|value| value.as_str()) else {
            continue;
        };
        if let Some((info, _)) = extra.get(app_id) {
            let info = info.clone();
            if version_missing {
                let version = info
                    .get("version")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| latest_version_string(&info, "version"));
                if !version.is_empty() {
                    obj.insert("version".into(), JsonValue::String(version));
                }
            }
            if min_ios_missing {
                let min_ios = latest_version_string(&info, "min_ios");
                if !min_ios.is_empty() {
                    obj.insert("min_ios".into(), JsonValue::String(min_ios));
                }
            }
        }
    }

    items
}

/// Apps only — used by background tasks and legacy callers.
#[tauri::command]
async fn fetch_catalog(app: AppHandle) -> Result<Vec<JsonValue>, String> {
    Ok(fetch_catalog_feed(app).await?.apps)
}

/// Full catalog home: apps + groups + text banners from API.
#[tauri::command]
async fn fetch_catalog_feed(app: AppHandle) -> Result<CatalogFeed, String> {
    let value = api_get(&app, "main.php", &[]).await?;
    let list = value
        .get("apps")
        .or_else(|| value.get("data"))
        .unwrap_or(&value)
        .as_array()
        .ok_or_else(|| "main.php не вернул список приложений".to_string())?;
    let from_cache = json_bool(value.get("_signum_from_cache"), false);
    let apps = if from_cache { list.clone() } else { enrich_catalog_apps(&app, list).await };
    if !from_cache {
        let mut enriched = value.clone();
        enriched["apps"] = serde_json::json!(apps);
        if let Ok(path) = catalog_response_path(&app, "main.php", &[]) { let _ = save_catalog_response(&path, &enriched); }
    }
    Ok(CatalogFeed {
        apps,
        groups: parse_catalog_groups(&value),
        banners: parse_catalog_banners(&value),
        from_cache,
        cached_at: value.get("_signum_cached_at").and_then(JsonValue::as_u64),
    })
}

fn cache_bust_url(url: &str, bust: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.contains("signum_cb=") {
        return trimmed.to_string();
    }
    let sep = if trimmed.contains('?') { '&' } else { '?' };
    format!("{trimmed}{sep}signum_cb={bust}")
}

#[tauri::command]
async fn fetch_app_info(app: AppHandle, app_id: String) -> Result<JsonValue, String> {
    let value = api_get(&app, "info.php", &[("id", app_id.as_str())]).await?;
    Ok(value.get("data").cloned().unwrap_or(value))
}

fn auth_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("Signum", "apple_auth_email").map_err(|e| e.to_string())
}

fn stored_auth_email(app: &AppHandle) -> Option<String> {
    if let Some(email) = active_account_email(app) {
        return Some(email);
    }
    if let Ok(entry) = auth_entry() {
        if let Ok(email) = entry.get_password() {
            let trimmed = normalize_account_email(&email);
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn require_auth(app: &AppHandle) -> Result<(), String> {
    if demo_active() {
        return Ok(());
    }
    if stored_auth_email(app).is_some() {
        Ok(())
    } else {
        Err("Сначала войдите в Apple ID на странице «Аккаунт»".into())
    }
}
fn resolve_ipatool_keychain(app: &AppHandle) -> Result<IpatoolKeychain, String> {
    let settings = load_settings(app)?;
    match settings.ipatool_keychain_mode.as_str() {
        "custom" => settings
            .ipatool_keychain_passphrase
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(IpatoolKeychain::Passphrase)
            .ok_or_else(|| {
                "В настройках выбран «Свой ключ», но поле пустое. Укажите ключ или смените режим."
                    .into()
            }),
        _ => get_or_create_random_passphrase().map(IpatoolKeychain::Passphrase),
    }
}

fn ipatool_data_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .map(|home| home.join(".ipatool"))
}

fn reset_ipatool_local_store() {
    let Some(dir) = ipatool_data_dir() else {
        return;
    };
    for name in [
        "account",
        "cookies",
        "account.json.bak",
        "account.export.json",
    ] {
        let _ = fs::remove_file(dir.join(name));
    }
    let _ = fs::remove_file(dir.join("keyring").join("account"));
    cleanup_stale_cookie_lock(true);
}

/// Wipe Netscape junk and the empty `{}` we used to write as a reset.
/// Keep a non-empty JSON object: ipatool-cpp (v3) stores cookies that way.
/// Go ipatool (v2) wants an array; wiping a live `{...}` jar after login
/// made auth info say «Not logged in».
fn ipatool_cookie_bytes_are_corrupt(raw: &[u8]) -> bool {
    match first_non_ws_byte(raw) {
        None => false,
        Some(b'[') => false,
        Some(b'{') => {
            let compact: Vec<u8> = raw
                .iter()
                .copied()
                .filter(|b| !b.is_ascii_whitespace())
                .collect();
            compact.as_slice() == b"{}"
        }
        Some(_) => true,
    }
}

fn ipatool_cookie_file_is_corrupt(path: &Path) -> bool {
    // "Corrupt" here means "unreadable by the engine that owns the store".
    // ipatool-cpp keeps a libcurl (Netscape) jar, which only the JSON-based Go
    // engines choke on; rewriting it wiped the session cpp had just created and
    // surfaced later as "password token is expired".
    if cpp_engine_owns_ipatool_store() {
        return false;
    }
    fs::read(path)
        .map(|raw| ipatool_cookie_bytes_are_corrupt(&raw))
        .unwrap_or(false)
}

fn sanitize_cookie_file(path: &Path) {
    if !path.is_file() || !ipatool_cookie_file_is_corrupt(path) {
        return;
    }
    if let Some(parent) = path.parent() {
        let bak = parent.join("cookies.netscape.bak");
        let _ = fs::rename(path, &bak);
    } else {
        let _ = fs::remove_file(path);
    }
    let _ = fs::write(path, b"[]\n");
}

fn migrate_ipatool_account_layout() {
    let Some(dir) = ipatool_data_dir() else {
        return;
    };
    let legacy = dir.join("account");
    if !legacy.is_file() {
        return;
    }
    let Ok(raw) = fs::read(&legacy) else {
        return;
    };
    if raw.iter().all(|b| b.is_ascii_whitespace()) {
        let _ = fs::remove_file(&legacy);
        return;
    }
    let keyring_dir = dir.join("keyring");
    let keyring_account = keyring_dir.join("account");
    if file_nonempty(&keyring_account) {
        if let Ok(key_raw) = fs::read(&keyring_account) {
            if ipatool_account_bytes_are_json_sidecar(&key_raw)
                || !ipatool_account_bytes_are_jwe(&key_raw)
            {
                let _ = fs::remove_file(&keyring_account);
            }
        }
    }
    if ipatool_account_bytes_are_json_sidecar(&raw) {
        return;
    }
    if !ipatool_account_bytes_are_jwe(&raw) {
        return;
    }
    if file_nonempty(&keyring_account) {
        return;
    }
    let _ = fs::create_dir_all(&keyring_dir);
    let _ = fs::copy(&legacy, &keyring_account);
}

fn sanitize_ipatool_cookies() {
    sanitize_ipatool_session_files();
}

fn sanitize_ipatool_session_files() {
    let Some(dir) = ipatool_data_dir() else {
        return;
    };
    sanitize_cookie_file(&dir.join("cookies"));
    migrate_ipatool_account_layout();
}

fn is_keychain_missing_item_error(output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    lowered.contains("could not be found in the keyring")
        || lowered.contains("item not found")
        || lowered.contains("errkeynotfound")
}

fn is_keychain_integrity_error(output: &str) -> bool {
    if is_keychain_missing_item_error(output) {
        return false;
    }
    let lowered = output.to_ascii_lowercase();
    lowered.contains("account file is not protected")
        || lowered.contains("keyunwrap")
        || lowered.contains("integrity check failed")
        || lowered.contains("illegal base64")
        || (lowered.contains("failed to get account") && lowered.contains("illegal"))
}

fn is_keychain_passphrase_required_error(output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    lowered.contains("keychain passphrase is required")
        || lowered.contains("use the --keychain-passphrase")
}

fn strip_ansi(text: &str) -> String {
    static ANSI: OnceLock<Regex> = OnceLock::new();
    ANSI.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap())
        .replace_all(text, "")
        .trim()
        .to_string()
}

/// Готовит лог для показа человеку.
///
/// Движок сыплет в тот же поток строку прогресса — «PROGRESS:7» тысячами
/// повторов. Из-за этого в окне ошибки было видно только их: настоящая причина
/// стояла последней строкой и не влезала в обрезку. Выкидываем счётчик и режем
/// начало, а не конец — причина отказа всегда в конце.
fn condense_log(text: &str, limit: usize) -> String {
    static PROGRESS: OnceLock<Regex> = OnceLock::new();
    let body = PROGRESS
        .get_or_init(|| Regex::new(r"(?m)^[ \t]*PROGRESS:\d{1,3}[ \t]*$|PROGRESS:\d{1,3}[ \t]*").unwrap())
        .replace_all(text, "");
    let body: String = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let total = body.chars().count();
    if total <= limit {
        return body;
    }
    let tail: String = body.chars().skip(total - limit).collect();
    format!("…\n{tail}")
}

fn device_not_found_message() -> String {
    "iPhone не найден. Подключите устройство по USB, разблокируйте его и подтвердите доверие."
        .into()
}

/// True when the Apple ID cannot use this storefront at all (must not soft-fail).
/// Apple granted the licence and then refused to serve the download: the app is
/// not in the storefront this Apple ID belongs to. ipatool-cpp words it as
/// "license was granted but download was blocked"; owning the app is not enough
/// to enumerate its history, though the latest build still downloads.
/// The developer pulled the app. Apple's download endpoints answer with a
/// `MZCommerce.ProductNotForSale` dialog naming it ("X No Longer Available"),
/// and refuse the file even to an Apple ID that holds the licence. No
/// storefront switch and no other engine changes that.
fn is_withdrawn_from_sale_error(text: &str) -> bool {
    let lowered = strip_ansi(text).to_ascii_lowercase();
    lowered.contains("no longer ava") || lowered.contains("productnotforsale")
}

fn is_region_locked_owned_app_error(text: &str) -> bool {
    let lowered = strip_ansi(text).to_ascii_lowercase();
    lowered.contains("license was granted but download was blocked")
        || (lowered.contains("not available for download") && lowered.contains("storefront"))
}

fn is_hard_storefront_error(clean: &str) -> bool {
    let lowered = clean.to_ascii_lowercase();
    lowered.contains("account not in store")
        || lowered.contains("accountnotinstore")
        || lowered.contains("store front ()")
        || lowered.contains("storefront ()")
        || (lowered.contains("itunes auth") && lowered.contains("302"))
}

/// Listing gone / region-hidden — often still downloadable if already purchased.
fn is_app_not_found_store_error(clean: &str) -> bool {
    let lowered = clean.to_ascii_lowercase();
    lowered.contains("app not found")
        || lowered.contains("could not find app")
        // ipatool-cpp words a region-hidden listing as
        // "app is not available for download in your region/storefront".
        || lowered.contains("not available for download")
        || lowered.contains("item not available")
        || lowered.contains("item is temporarily unavailable")
        || lowered.contains("temporarily unavailable")
        || lowered.contains("no longer available")
        || lowered.contains("not available in your country")
        || lowered.contains("not available in the")
}

/// Apple answers `empty songList` when the Apple ID holds no license for the
/// requested build. Same meaning as "not purchased" — parse_ipatool_list_versions
/// already treats it that way, so download/install must not fall through to the
/// generic "check Apple ID and iPhone" text.
/// Apple answering "you already have this". ipatool-cpp raises it as
/// `license already exists` (failureType 2059, or a bare HTTP 500 from the
/// purchase endpoint); the Go builds exit 0 with `alreadyOwned=true`. Either
/// way the licence is on the account and the purchase step is done.
fn is_already_owned_purchase_result(text: &str) -> bool {
    let lowered = strip_ansi(text).to_ascii_lowercase();
    lowered.contains("license already exists")
        || lowered.contains("already purchased")
        || lowered.contains("alreadyowned=true")
}

fn is_not_purchased_store_error(clean: &str) -> bool {
    let lowered = clean.to_ascii_lowercase();
    lowered.contains("empty songlist")
        || lowered.contains("not purchased")
        || lowered.contains("you must purchase this app first")
        || lowered.contains("license not found")
        // Both engines word a missing licence this way (ipatool.h LicenseRequired,
        // and the Go sapfix build prints the same). Without it list-versions
        // fell through to "could not read the list" and hid the purchase step.
        || lowered.contains("license is required")
        || lowered.contains("purchase history")
}

fn friendly_apple_store_error(clean: &str) -> Option<String> {
    if is_password_token_expired_error(clean) {
        return Some(password_token_expired_message());
    }
    if is_hard_storefront_error(clean) {
        return Some(
            "Apple ID не привязан к нужному магазину App Store (Account Not In This Store).\n\n\
             Что сделать:\n\
             1) На iPhone откройте App Store → аккаунт → «Медиаматериалы и покупки»\n\
             2) Войдите этим же Apple ID и примите условия\n\
             3) Установите любое бесплатное приложение из App Store\n\
             4) В Signum выйдите из Apple ID и войдите снова\n\n\
             Без этого покупка и скачивание обложек банков не работают."
                .into(),
        );
    }
    // Most specific first: Apple named the app and refused it outright.
    if is_withdrawn_from_sale_error(clean) {
        return Some(
            "Приложение снято с продажи в App Store.\n\n\
             Apple не отдаёт файл даже владельцу лицензии — восстановить из покупок нельзя. \
             Поможет только IPA, сохранённый раньше."
                .into(),
        );
    }
    // Checked before the generic "not found": Apple says in the same breath
    // that the licence WAS granted, so telling the user to sign in with the
    // Apple ID that owns the app points at a problem they do not have.
    if is_region_locked_owned_app_error(clean) {
        return Some(
            "Приложение есть в покупках этого Apple ID, но Apple не отдаёт файл в магазине вашей страны.\n\n\
             Лицензия подтверждена — дело только в витрине. Помогает временная смена страны Apple ID \
             либо вход с аккаунта той страны, где приложение продаётся."
                .into(),
        );
    }
    if is_app_not_found_store_error(clean) {
        return Some(
            "Приложение не найдено в App Store — его удалили из магазина или оно недоступно в вашем регионе.\n\n\
             Такую версию можно поставить только если она уже есть в покупках этого Apple ID.\n\
             Выберите другую версию или войдите в Apple ID, на котором приложение уже покупалось."
                .into(),
        );
    }
    if is_not_purchased_store_error(clean) {
        return Some(
            "Этой версии нет в покупках вашего Apple ID.\n\n\
             Выберите другую версию, либо войдите в тот Apple ID, где приложение уже загружали."
                .into(),
        );
    }
    None
}

fn format_process_error(program: &str, code: i32, text: &str) -> String {
    let clean = strip_ansi(text);
    // Отказ kbsync при включённой защите Windows — не загадка, а известная
    // причина. Раньше человек видел «приложение снято с продажи» и шёл искать
    // проблему в своём Apple ID, хотя дело было в настройке системы.
    if clean.contains("kbsync") {
        if let Some(advice) = windows_guards().advice() {
            return advice;
        }
    }
    if let Some(message) = friendly_apple_store_error(&clean) {
        return if DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed) {
            format!("{message}\n\nПодробный лог:\n{}", condense_log(&clean, 4000))
        } else {
            message
        };
    }
    // Pairing / Trust: never dump UDID + raw ERROR to the user.
    if is_trust_dialog_pair_error(&clean)
        || Path::new(program)
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|name| name.to_ascii_lowercase().contains("idevicepair"))
    {
        let friendly = friendly_pair_error(&clean);
        return if DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed) {
            format!(
                "{friendly}\n\nПодробный лог:\n{}",
                condense_log(&clean, 2000)
            )
        } else {
            friendly
        };
    }
    let lowered = clean.to_ascii_lowercase();
    if DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed) {
        return format!(
            "Операция завершилась с ошибкой (код {code}).\n\nПодробный лог:\n{}",
            condense_log(&clean, 12_000)
        );
    }
    if lowered.contains("anisette")
        || lowered.contains("x-apple-i-md")
        || (lowered.contains("adi") && lowered.contains("fail"))
    {
        return "Не удалось подготовить вход в Apple ID. Полностью закройте Signum и войдите снова. Если не поможет — переустановите приложение.".into();
    }
    if is_keychain_passphrase_required_error(&clean) {
        return "Для входа в Apple ID нужен ключ шифрования. \
                Откройте «Настройки» → «Ключ сессии Apple ID» и выберите «Рандом» или «Свой ключ»."
            .into();
    }
    if is_keychain_integrity_error(&clean) {
        return "Сохранённый вход повреждён или сменился ключ. \
                Выйдите из аккаунта на странице «Аккаунт» и войдите снова."
            .into();
    }
    // Was falling through to the generic "проверьте Apple ID и iPhone" text,
    // which told the user nothing and made the reports unreadable.
    if is_keychain_missing_item_error(&clean) || lowered.contains("not logged in") {
        return "Вход в Apple ID не найден на этом компьютере. \
                Откройте «Аккаунт» и войдите снова — Apple может запросить код."
            .into();
    }
    if is_corrupt_cookie_error(&clean) {
        return "Сохранённый вход повреждён. Откройте «Аккаунт» и нажмите «Войти снова».".into();
    }
    if is_device_missing_error(&clean) {
        return device_not_found_message();
    }
    if lowered.contains("passcode")
        || lowered.contains("device is locked")
        || lowered.contains("lockdown")
        || lowered.contains("invaliddevice")
    {
        return "iPhone заблокирован или не отвечает. Разблокируйте экран, \
                подтвердите «Доверять» и повторите попытку."
            .into();
    }
    // Same bundle id, different signing identity: iOS refuses to overwrite it,
    // and the raw "BundleIDConflictWithOtherIdentifier" told the user nothing.
    if lowered.contains("bundleidconflictwithotheridentifier") {
        return "На iPhone уже стоит приложение с таким же идентификатором, но подписанное другим сертификатом. Удалите его с телефона и повторите установку."
            .into();
    }
    if lowered.contains("applicationalreadyinstalled")
        || lowered.contains("already installed")
    {
        return "Такое приложение уже установлено на iPhone. Удалите его и повторите установку."
            .into();
    }
    if lowered.contains("devicelocked") || lowered.contains("mismatchedapplicationidentifierentitlement")
    {
        return "Подпись приложения не подходит этому iPhone. \
                Переподключите устройство и повторите установку."
            .into();
    }
    if is_network_unreachable_error(&lowered) {
        return apple_unreachable_message().into();
    }
    if lowered.contains("timed out")
        || lowered.contains("timeout")
        || lowered.contains("connection refused")
        || lowered.contains("connection reset")
        || lowered.contains("network error")
    {
        return "Не удалось подключиться к сервису. Проверьте интернет и повторите попытку.".into();
    }
    if lowered.contains("not authenticated")
        || lowered.contains("session expired")
        || lowered.contains("unauthorized")
    {
        return "Сессия Apple ID устарела. Войдите в аккаунт снова.".into();
    }
    if is_not_purchased_store_error(&clean) {
        return "Приложение не найдено в покупках этого Apple ID.".into();
    }
    if lowered.contains("applicationverificationfailed") || lowered.contains("verification failed")
    {
        return "iPhone отклонил приложение при проверке. Переподключите устройство и повторите установку."
            .into();
    }
    if lowered.contains("no space left") || lowered.contains("disk full") {
        return "Недостаточно свободного места для завершения операции.".into();
    }
    // ipatool-cpp 1.1.x checks the IPA against Apple's md5, re-downloads once,
    // then gives up with "download hash mismatch …". Нужна понятная причина, а
    // не общий текст про Apple ID и iPhone.
    if lowered.contains("hash mismatch") || lowered.contains("could not compute md5") {
        return "Файл приложения скачался повреждённым — не совпала контрольная сумма Apple. \
                Проверьте интернет, отключите VPN или прокси и повторите скачивание."
            .into();
    }
    if lowered.contains("permission denied") || lowered.contains("access is denied") {
        return "Недостаточно прав для выполнения операции. Проверьте доступ к файлам Signum."
            .into();
    }
    // ipatool says `invalid response` when Apple answers something it cannot
    // parse - usually an unsigned or stale Store request. The generic text below
    // sent people to check their iPhone, which has nothing to do with it.
    if lowered.contains("invalid response") {
        return "Apple отклонил запрос на скачивание. Выйдите из Apple ID и войдите снова, \
                затем повторите попытку."
            .into();
    }
    if Path::new(program).file_name().and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().contains("ipatool"))
    {
        return "Не удалось получить файл приложения из App Store. Проверьте интернет и вход в Apple ID, затем повторите скачивание. Если ошибка остаётся — включите подробные ошибки в настройках и отправьте лог в поддержку.".into();
    }
    let _ = code;
    "Не удалось выполнить операцию. Проверьте Apple ID, подключение iPhone и повторите попытку."
        .into()
}

fn clear_active_auth_state(app: &AppHandle) {
    if let Ok(entry) = auth_entry() {
        let _ = entry.delete_credential();
    }
    let mut cache = load_cache(app);
    cache.auth_email = None;
    cache.active_account_email = None;
    // Session files are wiped below, so the recorded engine is stale too.
    // Leaving it would pin "Авто" to an engine that owns nothing.
    cache.active_ipatool_engine = None;
    let _ = save_cache(app, &cache);
    if let Ok(mut slot) = process_account_mac().lock() {
        *slot = None;
    }
    apple_store::set_process_mac(None);
    reset_ipatool_local_store();
}

/// Был ли снят вход при запуске — чтобы сказать об этом человеку.
///
/// Флаг, а не событие: банер подписывается на события уже после запуска, и
/// событие, отправленное из setup(), просто некому поймать.
static LEGACY_LOGOUT_NOTICE: AtomicBool = AtomicBool::new(false);

/// Забрать уведомление о снятом входе. Второй раз вернёт false.
#[tauri::command]
fn take_legacy_logout_notice() -> bool {
    LEGACY_LOGOUT_NOTICE.swap(false, Ordering::SeqCst)
}

/// Apple ответила, что срок входа истёк.
///
/// Ставится там, где это увидели первым, — чаще всего при опросе версий.
/// Без этого отказ уходил в отладочный лог, проверка отвечала «не выяснили»,
/// и человек видел бесконечную загрузку карточки и пустой список версий,
/// хотя Apple прямым текстом сказала, что нужно войти заново.
static SESSION_EXPIRED_NOTICE: AtomicBool = AtomicBool::new(false);

/// Забрать уведомление об истёкшем входе. Второй раз вернёт false.
#[tauri::command]
fn take_session_expired_notice() -> bool {
    SESSION_EXPIRED_NOTICE.swap(false, Ordering::SeqCst)
}

/// Запомнить отказ Apple по сроку входа и сказать об этом окну.
fn note_session_expired(app: &AppHandle, where_from: &str) {
    if SESSION_EXPIRED_NOTICE.swap(true, Ordering::SeqCst) {
        return; // уже сказали, второй баннер ни к чему
    }
    diag_log::note("app", &format!("срок входа истёк ({where_from}) — прошу войти заново"));
    let _ = app.emit("session-expired", ());
}

/// Разлогинить тех, кто пришёл с движка v2 или v3.
///
/// Эти движки хранили вход в своём месте (Go FileBackend — `~/.ipatool/keyring/account`),
/// а v4 и v5 читают своё. После обновления выбор движка сам съезжает на «Авто»,
/// но запись о владельце сессии остаётся, и человек видит себя вошедшим, хотя
/// любая операция отвечает «failed to get account». Вид залогиненного
/// пользователя, который ничего не может, хуже честной просьбы войти заново.
///
/// Возвращает true, если вход действительно сняли.
fn drop_retired_engine_session(app: &AppHandle) -> bool {
    let mut cache = load_cache(app);
    let retired = cache
        .active_ipatool_engine
        .as_deref()
        .map(str::trim)
        .is_some_and(|owner| matches!(owner, "v2" | "v3"));
    if !retired {
        return false;
    }
    let owner = cache.active_ipatool_engine.clone().unwrap_or_default();
    for account in std::mem::take(&mut cache.apple_accounts) {
        let _ = delete_account_session(app, &account.email);
    }
    let _ = save_cache(app, &cache);
    clear_active_auth_state(app);
    ipatool_debug_log(&format!(
        "сессия движка {owner} снята: движок больше не поставляется, нужен повторный вход"
    ));
    true
}

fn clear_auth_state(app: &AppHandle) {
    if let Some(email) = active_account_email(app) {
        let _ = delete_account_session(app, &email);
    }
    clear_active_auth_state(app);
}

fn append_ipatool_keychain_args(args: &mut Vec<String>, keychain: &IpatoolKeychain) {
    let IpatoolKeychain::Passphrase(passphrase) = keychain;
    args.push("--keychain-passphrase".into());
    args.push(passphrase.clone());
}

pub(crate) fn ipatool_args(keychain: &IpatoolKeychain) -> Vec<String> {
    // Go ipatool (Windows sapfix, macOS CommerceKit, Linux) must not block on
    // stdin for 2FA — Signum has no console. The tool then prints
    // "2FA code is required" and Apple already sent the push.
    let mut args = vec!["--non-interactive".into()];
    append_ipatool_keychain_args(&mut args, keychain);
    args
}

fn ipatool_args_download(keychain: &IpatoolKeychain) -> Vec<String> {
    let mut args = vec![];
    append_ipatool_keychain_args(&mut args, keychain);
    args
}

fn cookie_lock_path() -> Option<PathBuf> {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    home.map(|p| p.join(".ipatool").join("cookies.lock"))
}

#[cfg(target_os = "windows")]
fn ipatool_process_running() -> bool {
    let mut command = StdCommand::new("tasklist");
    command
        .args(["/FI", "IMAGENAME eq ipatool.exe"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|text| text.to_ascii_lowercase().contains("ipatool.exe"))
}

#[cfg(target_os = "macos")]
fn ipatool_process_running() -> bool {
    std::process::Command::new("pgrep")
        .arg("-x")
        .arg("ipatool")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn ipatool_process_running() -> bool {
    std::process::Command::new("pgrep")
        .arg("-x")
        .arg("ipatool")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn ipatool_process_running() -> bool {
    false
}

fn is_cookie_lock_error(output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    lowered.contains("cookies.lock")
        || lowered.contains("cannot acquire lock")
        || lowered.contains("file locked")
}

fn is_corrupt_cookie_error(output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    lowered.contains("cannot load cookies")
        || lowered.contains("discarding cookies")
        || lowered.contains("[]cookiejar.entry")
        || (lowered.contains("cannot unmarshal") && lowered.contains("cookie"))
        || (lowered.contains("panic") && lowered.contains("cookie"))
        || (lowered.contains("invalid character") && lowered.contains("cookie"))
        || (lowered.contains("looking for beginning of value") && lowered.contains("cookie"))
        || (lowered.contains("invalid character '#'") && lowered.contains("goroutine"))
}

fn is_two_factor_error(output: &str) -> bool {
    // Only treat as "code is required now" — bare "2fa"/"auth-code" also appear in
    // generic login hints and would force the UI into the 2FA step incorrectly.
    let lowered = output.to_ascii_lowercase();
    if lowered.starts_with("signum_2fa_required") {
        return true;
    }
    [
        // Windows / older ipatool wording
        "2fa code is required",
        "auth code is required",
        "two-factor authentication is required",
        "two factor authentication is required",
        "secondary authentication required",
        "enter the verification code",
        "enter verification code",
        "enter the code to continue",
        "enter 2fa code",
        "enter 2fa code:",
        "failed to read string",
        "failed to read auth code",
        "verification code has been sent",
        "[2fa]",
        "supply a code using the `--auth-code`",
        "supply a code using the --auth-code",
        // macOS ipatool-cpp (v2/v3): "Error: two-factor auth code required. Retry with --auth-code CODE"
        "two-factor auth code required",
        "two factor auth code required",
        "two-factor auth code is required",
        "two factor auth code is required",
        "retry with --auth-code",
        "retry with -a",
        "requires an auth code",
        "require an auth code",
        "auth-code required",
        "auth code required",
        "запросил код подтверждения",
        "требуется код подтверждения",
        "нужен код подтверждения",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn two_factor_channel(output: &str) -> Option<&'static str> {
    // Our ipatool fork prints the channel to stderr right before the 2FA
    // prompt: Apple either pushes to trusted devices or sends an SMS.
    let lowered = output.to_ascii_lowercase();
    if lowered.contains("[2fa] channel=sms") {
        return Some("sms");
    }
    if lowered.contains("[2fa] channel=trusted-device") {
        return Some("trusted-device");
    }
    None
}

fn is_network_unreachable_error(lowered: &str) -> bool {
    lowered.contains("winhttp")
        || lowered.contains("could not resolve")
        || lowered.contains("couldn't connect")
        || lowered.contains("couldn't resolve")
}

fn apple_unreachable_message() -> &'static str {
    "Нет связи с серверами Apple. Проверьте интернет; если включены VPN или прокси — \
     попробуйте без них или с другим сервером, затем повторите."
}

fn format_login_error(output: &str, password: &str) -> String {
    let sanitized = strip_ansi(&output.replace(password, "********"));
    if is_two_factor_error(&sanitized) {
        let channel_hint = match two_factor_channel(&sanitized) {
            Some("sms") => " Apple отправил код по SMS на номер телефона аккаунта.",
            Some("trusted-device") => {
                " Apple отправил код на доверенные устройства (iPhone)."
            }
            _ => "",
        };
        let message = format!(
            "SIGNUM_2FA_REQUIRED: Apple запросил код подтверждения. Введите 6 цифр с доверенного устройства.{channel_hint}"
        );
        ipatool_debug_log(&format!(
            "login-2fa-channel channel={:?}",
            two_factor_channel(&sanitized)
        ));
        return message;
    }
    if is_cookie_lock_error(&sanitized) {
        return with_technical_detail(
            "Signum сейчас занят. Подождите минуту и повторите вход.",
            &sanitized,
        );
    }
    let lowered = sanitized.to_ascii_lowercase();
    // ipatool-cpp не достучался до Apple: WinHTTP 12029 и т. п. на Windows,
    // ошибки curl на Mac и Linux. Раньше это уходило в «проверьте email и
    // пароль», и люди меняли пароль, хотя дело было в сети или VPN.
    if is_network_unreachable_error(&lowered) {
        return with_technical_detail(apple_unreachable_message(), &sanitized);
    }
    if lowered.contains("sap helper")
        || lowered.contains("sapsigner")
        || lowered.contains("0xc0000409")
    {
        let friendly = "Не удалось подготовить вход в Apple ID. Полностью закройте Signum и войдите снова.";
        return with_technical_detail(friendly, &sanitized);
    }
    if lowered.contains("dyld")
        || lowered.contains("library not loaded")
        || lowered.contains("image not found")
        || lowered.contains("mach-o file")
    {
        return with_technical_detail(
            "macOS не смог загрузить библиотеку Apple из комплекта Signum. Обновите или переустановите Signum, затем откройте приложение заново.",
            &sanitized,
        );
    }
    if lowered.contains("anisette")
        || lowered.contains("adi") && lowered.contains("fail")
        || lowered.contains("x-apple-i-md")
    {
        let friendly = "Не удалось подготовить вход в Apple ID. Полностью закройте Signum и войдите снова. Если не поможет — переустановите приложение.";
        return with_technical_detail(friendly, &sanitized);
    }
    if lowered.contains("locked")
        || lowered.contains("iforgot")
        || lowered.contains("-20209")
        || lowered.contains("security reasons")
    {
        return with_technical_detail(
            "Apple заблокировал этот Apple ID по соображениям безопасности. \
                Разблокируйте аккаунт на https://iforgot.apple.com и повторите вход.",
            &sanitized,
        );
    }
    if lowered.contains("account is disabled")
        || lowered.contains("disabled") && lowered.contains("account")
        || lowered.contains("this apple account has been disabled")
    {
        return with_technical_detail(
            "Этот Apple ID отключён. Проверьте статус аккаунта на appleid.apple.com.",
            &sanitized,
        );
    }
    if (lowered.contains("incorrect") || lowered.contains("invalid"))
        && (lowered.contains("password")
            || lowered.contains("credential")
            || lowered.contains("account"))
        || lowered.contains("srp") && lowered.contains("fail")
        || lowered.contains("authentication failed")
        || lowered.contains("failed to sign in")
    {
        return with_technical_detail("Неверный Apple ID или пароль.", &sanitized);
    }
    if lowered.contains("too many")
        || lowered.contains("rate limit")
        || lowered.contains("try again later")
        || lowered.contains("throttl")
    {
        return with_technical_detail(
            "Слишком много попыток входа. Подождите несколько минут и попробуйте снова.",
            &sanitized,
        );
    }
    if is_corrupt_cookie_error(&sanitized) {
        return "Сохранённый вход повреждён. Нажмите «Войти снова».".into();
    }
    if is_keychain_passphrase_required_error(&sanitized) {
        return with_technical_detail(
            "Для входа в Apple ID нужен ключ. \
                Откройте «Настройки» → «Ключ сессии Apple ID» и выберите «Рандом» или «Свой ключ».",
            &sanitized,
        );
    }
    if is_keychain_integrity_error(&sanitized) {
        return with_technical_detail(
            "Сохранённый вход повреждён или сменился ключ. \
                Выйдите из аккаунта на странице «Аккаунт» и войдите снова.",
            &sanitized,
        );
    }
    if lowered.contains("network")
        || lowered.contains("timed out")
        || lowered.contains("timeout")
        || lowered.contains("could not resolve")
        || lowered.contains("connection")
    {
        return with_technical_detail(
            "Не удалось связаться с серверами Apple. Проверьте интернет и повторите вход.",
            &sanitized,
        );
    }
    if lowered.contains("2fa code rejected")
        || lowered.contains("2fa code was rejected")
        || lowered.contains("invalid auth code")
        || lowered.contains("invalid verification code")
        || lowered.contains("auth code rejected")
        || lowered.contains("incorrect verification code")
        || lowered.contains("wrong verification code")
        || lowered.contains("authentication code is incorrect")
        || lowered.contains("the code you entered is incorrect")
        || (lowered.contains("неверный") && lowered.contains("код"))
    {
        return with_technical_detail(
            "Код не подошёл. Введите код с iPhone ещё раз. Новый запрашивайте, только если пришёл другой.",
            &sanitized,
        );
    }
    // Apple ответила на вход не решением, а служебным кодом: 301, 204, 403,
    // 404 или 500. Пароль тут ни при чём, и второй движок с теми же данными
    // получит то же самое. Метку SIGNUM_APPLE_UNAVAILABLE читает
    // login_catalog_with_fallback: технический хвост с кодом ответа в обычном
    // режиме отрезается, и без метки отличить этот отказ было бы не по чему.
    if let Some(status) = apple_opaque_auth_status(&lowered) {
        return with_technical_detail(
            &format!(
                "{APPLE_UNAVAILABLE_MARKER} Apple сейчас не принимает вход (ответ сервера {status}). \
                 Пароль менять не нужно — подождите несколько минут и повторите."
            ),
            &sanitized,
        );
    }
    // Opaque ipatool-cpp body. The caller may turn this into a 2FA retry only
    // when it actually submitted a code; an initial login remains a generic
    // transient Apple error.
    if lowered.contains("something went wrong")
        || (lowered.contains("status=200") && lowered.contains("login error"))
    {
        return with_technical_detail(
            "SIGNUM_LOGIN_OPAQUE: Apple отклонил вход (часто устаревший код 2FA или лимит попыток). Запросите новый код или подождите пару минут.",
            &sanitized,
        );
    }
    with_technical_detail(
        "Не удалось войти в Apple ID. Проверьте email, пароль и интернет, затем повторите попытку.",
        &sanitized,
    )
}

fn cleanup_stale_cookie_lock(force: bool) {
    let Some(path) = cookie_lock_path() else {
        return;
    };
    if !path.exists() {
        return;
    }
    let should_remove = if force {
        !ipatool_process_running()
    } else {
        fs::metadata(&path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .map(|age| age > Duration::from_secs(30) && !ipatool_process_running())
            .unwrap_or(false)
    };
    if should_remove {
        let _ = fs::remove_file(path);
    }
    sanitize_ipatool_cookies();
}

fn parse_json_object_from_text(text: &str) -> Option<JsonValue> {
    for line in text.lines() {
        let line = line.trim();
        let Some(start) = line.find('{') else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<JsonValue>(&line[start..]) {
            return Some(value);
        }
    }
    let trimmed = text.trim();
    if let Some(start) = trimmed.find('{') {
        if let Ok(value) = serde_json::from_str::<JsonValue>(&trimmed[start..]) {
            return Some(value);
        }
    }
    None
}

fn parse_ipatool_auth_info_email(text: &str) -> Result<String, String> {
    if let Some(value) = parse_json_object_from_text(text) {
        if !json_bool(value.get("success"), false) {
            if let Some(error) = value.get("error").and_then(|value| value.as_str()) {
                return Err(strip_ansi(error));
            }
        } else {
            for key in ["email", "accountName", "account"] {
                if let Some(email) = value
                    .get(key)
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    return Ok(normalize_account_email(email));
                }
            }
        }
    }
    Err("Не удалось проверить сессию Apple ID".into())
}

fn format_session_verify_error(error: &str) -> String {
    let clean = strip_ansi(error);
    let lowered = clean.to_ascii_lowercase();
    if lowered.contains("not logged in") || is_keychain_missing_item_error(&clean) {
        return with_technical_detail(
            "Вход в Apple ID не сохранился. Повторите вход; если Apple пришлёт код — введите его сразу.",
            &clean,
        );
    }
    if is_corrupt_cookie_error(&clean) {
        return "Сохранённый вход повреждён. Нажмите «Войти снова».".into();
    }
    if is_keychain_integrity_error(&clean) || is_keychain_passphrase_required_error(&clean) {
        return with_technical_detail(
            "Сохранённый вход повреждён или не подходит ключ. \
                В «Настройках» проверьте «Ключ сессии Apple ID», выйдите из аккаунта и войдите снова.",
            &clean,
        );
    }
    if is_two_factor_error(&clean) {
        return "SIGNUM_2FA_REQUIRED: Apple запросил код подтверждения. Введите 6 цифр с доверенного устройства."
            .into();
    }
    if lowered.contains("anisette")
        || lowered.contains("x-apple-i-md")
        || (lowered.contains("adi") && lowered.contains("fail"))
    {
        return with_technical_detail(
            "Не удалось подтвердить вход в Apple ID. Полностью закройте Signum и войдите снова.",
            &clean,
        );
    }
    with_technical_detail("Не удалось проверить вход в Apple ID.", &clean)
}

async fn run_auth_info_once(program: &str, keychain: &IpatoolKeychain) -> Result<String, String> {
    // Global flags must come before the subcommand. Putting --format after
    // `auth info` can confuse argument parsing on some ipatool-cpp builds.
    let mut args = ipatool_args(keychain);
    args.extend([
        "--format".into(),
        "json".into(),
        "auth".into(),
        "info".into(),
    ]);
    let run = || run_process_raw_error(program, &args, Duration::from_secs(30), None);
    let output = match run().await {
        Ok(output) => output,
        Err(error)
            if is_corrupt_cookie_error(&error) || is_keychain_integrity_error(&error) =>
        {
            sanitize_ipatool_session_files();
            cleanup_stale_cookie_lock(true);
            run()
                .await
                .map_err(|retry| format_session_verify_error(&retry))?
        }
        Err(error) => return Err(format_session_verify_error(&error)),
    };
    if is_corrupt_cookie_error(&output) || is_keychain_integrity_error(&output) {
        sanitize_ipatool_session_files();
        cleanup_stale_cookie_lock(true);
        match run().await {
            Ok(retry_output) => {
                return parse_ipatool_auth_info_email(&retry_output)
                    .map_err(|error| format_session_verify_error(&error));
            }
            Err(retry) => return Err(format_session_verify_error(&retry)),
        }
    }
    parse_ipatool_auth_info_email(&output).map_err(|error| format_session_verify_error(&error))
}

/// The session is stored per engine: v2 (Go FileBackend) uses
/// `~/.ipatool/keyring/account`, v3 uses `~/.ipatool/account`. Asking the wrong
/// engine returns "not logged in" / "could not be found in the keyring" even
/// though the login succeeded, which looked like a lost session to the user.
fn session_lookup_missed_engine(error: &str) -> bool {
    let clean = strip_ansi(error);
    is_keychain_missing_item_error(&clean) || clean.to_ascii_lowercase().contains("not logged in")
}

async fn verify_ipatool_session(app: &AppHandle) -> Result<String, String> {
    let tools = resolve_tool_paths(app)?;
    let keychain = resolve_ipatool_keychain(app)?;
    // No fallback to the legacy hardcoded key — it must never reappear in keyring.
    let first = run_auth_info_once(&tools.ipatool, &keychain).await;
    let Err(error) = first else {
        return first;
    };
    if !session_lookup_missed_engine(&error) {
        return Err(error);
    }
    // Every Go engine rewrites `~/.ipatool/cookies` in its own JSON format and
    // renames whatever it found to `cookies.corrupt`. Probing them therefore
    // destroys the libcurl jar ipatool-cpp had just written — which is exactly
    // the "cookies but no account yet" state that means Apple is waiting for a
    // 2FA code, so the user was told the login had not been saved.
    let jar = ipatool_cookie_snapshot();
    // Try the other bundled engine before declaring the session lost.
    for (engine, candidate) in login_tool_paths(app) {
        if candidate.ipatool == tools.ipatool {
            continue;
        }
        match run_auth_info_once(&candidate.ipatool, &keychain).await {
            Ok(email) => {
                ipatool_debug_log(&format!(
                    "auth-info cross-engine hit engine={engine} ipatool={}",
                    candidate.ipatool
                ));
                remember_login_engine(app, &engine);
                // The engine that answered owns the store now; its jar stays.
                return Ok(email);
            }
            Err(other) => ipatool_debug_log(&format!(
                "auth-info cross-engine miss engine={engine}: {}",
                technical_excerpt(&other, 300)
            )),
        }
    }
    // Nobody claimed the session: undo whatever the probes did to the jar.
    restore_ipatool_cookie_snapshot(jar);
    Err(error)
}

fn restore_previous_active_session(
    app: &AppHandle,
    previous_email: Option<&str>,
) -> Result<(), String> {
    reset_ipatool_local_store();
    // Session files are rewritten from a backup, so the engine recorded earlier
    // no longer describes what is on disk. Drop the pin and let the next
    // `auth info` re-learn the owner instead of trusting a stale value.
    forget_login_engine(app);
    if let Some(email) = previous_email {
        restore_ipatool_session_for_account(app, email)?;
    }
    Ok(())
}

fn rollback_to_previous_active_session(
    app: &AppHandle,
    previous_email: Option<&str>,
) -> Result<(), String> {
    reset_ipatool_local_store();
    if let Some(email) = previous_email.filter(|email| saved_account_session_exists(app, email)) {
        restore_ipatool_session_for_account(app, email)?;
    }
    Ok(())
}

async fn ensure_active_ipatool_session(app: &AppHandle) -> Result<String, String> {
    if demo_active() {
        return Ok(demo_email());
    }
    sanitize_ipatool_cookies();
    let expected = stored_auth_email(app)
        .ok_or_else(|| "Сначала войдите в Apple ID на странице «Аккаунт»".to_string())?;
    if verify_ipatool_session(app)
        .await
        .is_ok_and(|actual| actual == expected)
    {
        bind_account_catalog_cache(app, &expected);
        return Ok(expected);
    }

    restore_previous_active_session(app, Some(&expected))?;
    sanitize_ipatool_cookies();
    match verify_ipatool_session(app).await {
        Ok(actual) if actual == expected => {
            bind_account_catalog_cache(app, &expected);
            Ok(expected)
        }
        Ok(actual) => Err(format!(
            "Сохранённая сессия принадлежит другому Apple ID ({actual}), ожидался {expected}. Войдите в {expected} снова."
        )),
        Err(error) if is_corrupt_cookie_error(&error) => {
            reset_ipatool_local_store();
            Err("Сохранённый вход повреждён. Нажмите «Войти снова».".into())
        }
        Err(error) => Err(with_technical_detail(
            "Сессия истекла или повреждена. Войдите в этот Apple ID снова.",
            &error,
        )),
    }
}

fn login_output_indicates_failure(output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    if is_two_factor_error(output) {
        return false;
    }
    lowered.contains("login error")
        || lowered.contains("not logged in")
        || lowered.contains("authentication failed")
        || lowered.contains("failed to sign in")
        || lowered.contains("gsa init error")
        || lowered.contains("gsa complete error")
        || lowered.contains("gsa srp")
        || lowered.contains("anisette")
            && (lowered.contains("fail")
                || lowered.contains("error")
                || lowered.contains("not found"))
}

async fn run_ipatool_login_once(
    program: &str,
    args: &[String],
    password: &str,
) -> Result<(), String> {
    let started = Instant::now();
    ipatool_debug_log(&format!(
        "login-once start program={program} args=[{}] {}",
        redact_cmd_args(args),
        ipatool_session_diag()
    ));
    // Preserve raw ipatool stderr/stdout on failure so format_login_error can
    // map Apple/GSA/anisette messages. Generic format_process_error would hide them.
    let result = match run_process_raw_error(program, args, Duration::from_secs(180), None).await {
        // ipatool deliberately exits with code 0 in --non-interactive mode
        // when Apple asks for 2FA. The message in stdout is therefore the
        // source of truth; the process exit status alone is not sufficient.
        Ok(output) if is_two_factor_error(&output) => {
            ipatool_debug_log(&format!(
                "login-once raw-2fa {}",
                technical_excerpt(&output, 800)
            ));
            Err(format_login_error(&output, password))
        }
        // Some builds can exit 0 while still printing Login error / Not logged in.
        Ok(output) if login_output_indicates_failure(&output) => {
            ipatool_debug_log(&format!(
                "login-once raw-fail-exit0 {}",
                technical_excerpt(&output, 1600)
            ));
            Err(format_login_error(&output, password))
        }
        Ok(output) => {
            ipatool_debug_log(&format!(
                "login-once raw-ok {}",
                technical_excerpt(&output, 800)
            ));
            Ok(())
        }
        Err(error) if is_cookie_lock_error(&error) => {
            cleanup_stale_cookie_lock(true);
            match run_process_raw_error(program, args, Duration::from_secs(180), None).await {
                Ok(output) if is_two_factor_error(&output) => {
                    Err(format_login_error(&output, password))
                }
                Ok(output) if login_output_indicates_failure(&output) => {
                    Err(format_login_error(&output, password))
                }
                Ok(_) => Ok(()),
                Err(retry) if is_cookie_lock_error(&retry) => Err(
                    "Signum сейчас занят. Закройте другие окна Signum или подождите минуту и повторите вход."
                        .into(),
                ),
                Err(retry) => {
                    ipatool_debug_log(&format!(
                        "login-once raw-retry-err {}",
                        technical_excerpt(&retry, 1600)
                    ));
                    Err(format_login_error(&retry, password))
                }
            }
        }
        Err(error) => {
            ipatool_debug_log(&format!(
                "login-once raw-err {}",
                technical_excerpt(&error, 1600)
            ));
            Err(format_login_error(&error, password))
        }
    };
    match &result {
        Ok(()) => ipatool_debug_log(&format!(
            "login-once ok elapsed_ms={} {}",
            started.elapsed().as_millis(),
            ipatool_session_diag()
        )),
        Err(error) if error.starts_with("SIGNUM_2FA_REQUIRED") => ipatool_debug_log(&format!(
            "login-once 2fa elapsed_ms={}",
            started.elapsed().as_millis()
        )),
        Err(error) => ipatool_debug_log(&format!(
            "login-once err elapsed_ms={} error={}",
            started.elapsed().as_millis(),
            technical_excerpt(error, 1200)
        )),
    }
    result
}

async fn run_ipatool_login(program: &str, args: &[String], password: &str) -> Result<(), String> {
    match run_ipatool_login_once(program, args, password).await {
        Ok(()) => Ok(()),
        Err(error) if is_corrupt_cookie_error(&error) => {
            reset_ipatool_local_store();
            cleanup_stale_cookie_lock(true);
            run_ipatool_login_once(program, args, password).await
        }
        // Anisette can flake on first try even with iCloud present — one recovery pass.
        Err(error)
            if is_anisette_failure_text(&error) && !error.starts_with("SIGNUM_2FA_REQUIRED") =>
        {
            clear_stale_apple_adi_cache();
            ensure_apple_mobile_device_service();
            if let Some(parent) = Path::new(program).parent() {
                unblock_windows_file(&parent.join("anisette.exe"));
                unblock_windows_file(Path::new(program));
            }
            tokio::time::sleep(Duration::from_millis(1200)).await;
            run_ipatool_login_once(program, args, password).await
        }
        Err(error) => Err(error),
    }
}

/// Record which engine wrote the current session. This must NOT touch
/// settings.ipatool_engine: "Авто" has to stay "Авто" in the UI, and an
/// explicit v2/v3 choice must survive a catalog login.
fn remember_login_engine(app: &AppHandle, engine: &str) {
    let engine = normalize_ipatool_engine(engine);
    if engine == "auto" {
        return;
    }
    let mut cache = load_cache(app);
    if cache.active_ipatool_engine.as_deref() == Some(engine.as_str()) {
        return;
    }
    cache.active_ipatool_engine = Some(engine);
    let _ = save_cache(app, &cache);
}

/// Engine that owns the session on disk, if we know it.
fn active_ipatool_engine(app: &AppHandle) -> Option<String> {
    load_cache(app)
        .active_ipatool_engine
        .map(|engine| normalize_ipatool_engine(&engine))
        .filter(|engine| engine != "auto")
}

fn forget_login_engine(app: &AppHandle) {
    let mut cache = load_cache(app);
    if cache.active_ipatool_engine.is_none() {
        return;
    }
    cache.active_ipatool_engine = None;
    let _ = save_cache(app, &cache);
}

fn copy_ipatool_session_from_sidecar(program: &str) {
    let Some(parent) = Path::new(program).parent() else {
        return;
    };
    let sidecar = parent.join(".ipatool").join("account");
    if !file_nonempty(&sidecar) {
        return;
    }
    if ipatool_account_file_is_json_sidecar(&sidecar) {
        return;
    }
    let Ok(raw) = fs::read(&sidecar) else {
        return;
    };
    if !ipatool_account_bytes_are_jwe(&raw) {
        return;
    }
    let Some(home_dir) = ipatool_data_dir() else {
        return;
    };
    let keyring_dir = home_dir.join("keyring");
    let _ = fs::create_dir_all(&keyring_dir);
    let _ = fs::copy(&sidecar, keyring_dir.join("account"));
    let cookies = parent.join(".ipatool").join("cookies");
    if cookies.is_file() && !ipatool_cookie_file_is_corrupt(&cookies) {
        let _ = fs::copy(cookies, home_dir.join("cookies"));
    }
}

fn is_invalid_2fa_code_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("gsa 2fa: invalid")
        || lower.contains("2fa code was rejected")
        || lower.contains("2fa code rejected")
        || lower.contains("invalid auth code")
        || lower.contains("invalid verification code")
        || lower.contains("incorrect verification code")
        || lower.contains("wrong verification code")
        || lower.contains("authentication code is incorrect")
        || lower.contains("the code you entered is incorrect")
        || lower.contains("-21669")
        || (lower.contains("неверный") && lower.contains("код"))
}

fn is_transient_2fa_check_error(error: &str) -> bool {
    is_anisette_failure_text(error)
        || is_cookie_lock_error(error)
        || {
            let lower = error.to_ascii_lowercase();
            lower.contains("timed out")
                || lower.contains("timeout")
                || lower.contains("connection")
                || lower.contains("network")
                || lower.contains("request failed")
                || lower.contains("temporarily")
        }
}

fn is_fatal_account_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("signum_bad_credentials")
        || lower.contains("неверный пароль")
        || lower.contains("incorrect password")
        || lower.contains("wrong password")
        || lower.contains("invalid password")
        || lower.contains("неверный apple id")
        || lower.contains("account disabled")
        || lower.contains("заблокирован")
        || lower.contains("заблокировал")
        || lower.contains("security reasons")
        || lower.contains("account has been locked")
        || lower.contains("password must be changed")
        || lower.contains("change your password")
        || lower.contains("reset your password")
        || lower.contains("password has expired")
        || lower.contains("iforgot")
        || lower.contains("-20209")
}

fn is_setup_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("повреждён файл")
        || lower.contains("поврежден файл")
        || lower.contains("нет ipatool")
        || lower.contains("не найден ipatool")
        || lower.contains("подготовить вход")
}

async fn login_catalog_with_fallback(
    app: &AppHandle,
    state: &AppState,
    email: &str,
    password: &str,
    two_factor_code: Option<&str>,
    resending: bool,
) -> Result<String, String> {
    auth_stage(
        "catalog-start",
        email,
        &format!("2fa={} resend={resending}", two_factor_code.is_some()),
    );
    let continuing_two_factor = two_factor_code.is_some() || resending;
    let preferred = state
        .login_2fa_engine
        .lock()
        .ok()
        .and_then(|engines| engines.get(email).cloned());
    let mut candidates = login_tool_paths(app);
    if continuing_two_factor {
        if let Some(engine) = preferred.as_deref() {
            candidates.retain(|(candidate, _)| candidate == engine);
        }
    } else if let Ok(mut engines) = state.login_2fa_engine.lock() {
        engines.remove(email);
    }
    if candidates.is_empty() {
        let error = "Не получается подготовить вход — не хватает файлов. Переустановите Signum.".to_string();
        save_last_catalog_error(&error);
        return Err(error);
    }

    let keychain = resolve_ipatool_keychain(app)?;
    let mut last_error = "Сейчас не получается подтвердить вход. Попробуйте позже.".to_string();
    ipatool_debug_log(&format!(
        "login-catalog start engines={} 2fa={} resend={} {}",
        candidates
            .iter()
            .map(|(engine, _)| engine.as_str())
            .collect::<Vec<_>>()
            .join(","),
        two_factor_code.is_some(),
        resending,
        ipatool_session_diag()
    ));
    for (index, (engine, tools)) in candidates.iter().enumerate() {
        V6_RUNTIME_SELECTED.store(engine == "v6", Ordering::Relaxed);
        refresh_v6_identity(app, Some(email));
        let _v6_runtime_guard = V6LoginRuntimeGuard(app.clone());
        if index > 0 && (engine == "v6" || !current_ipatool_session_exists()) {
            ipatool_debug_log(&format!(
                "login fallback reset store before engine={engine}"
            ));
            reset_ipatool_local_store();
            cleanup_stale_cookie_lock(true);
        }
        ensure_windows_sap_sidecars(&tools.ipatool);
        ensure_v4_unicorn_cache(app);
        ensure_anisette_basename(&tools.ipatool, tools.anisette.as_deref());
        ipatool_debug_log(&format!(
            "login try engine={engine} ipatool={} helper={:?} anisette={:?}",
            tools.ipatool,
            sap_helper_path_for(&tools.ipatool),
            tools.anisette
        ));
        let prepare_started = Instant::now();
        if let Err(error) = prepare_apple_auth_helpers(tools).await {
            ipatool_debug_log(&format!(
                "prepare failed engine={engine} elapsed_ms={}: {error}",
                prepare_started.elapsed().as_millis()
            ));
            last_error = error;
            continue;
        }
        ipatool_debug_log(&format!(
            "prepare ok engine={engine} elapsed_ms={}",
            prepare_started.elapsed().as_millis()
        ));
        let mut args = ipatool_args(&keychain);
        args.extend([
            "auth".into(),
            "login".into(),
            "-e".into(),
            email.to_string(),
            "-p".into(),
            password.to_string(),
        ]);
        if let Some(code) = two_factor_code {
            args.extend(["--auth-code".into(), code.to_string()]);
        }
        match run_ipatool_login(&tools.ipatool, &args, password).await {
            Ok(()) => {
                if !current_ipatool_session_exists() {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                }
                if !current_ipatool_session_exists() {
                    let _ = recover_ipatool_account_file(&tools.ipatool, &keychain).await;
                }
                // On macOS the Go engines keep the account in the login
                // Keychain, not in a file: keyring.Open takes the first
                // registered backend and darwin registers KeychainBackend, so
                // nothing ever appears under ~/.ipatool. The file check below
                // therefore reports "no session" after a login Apple already
                // completed, and the cookie heuristic then turns that into a
                // demand for a 2FA code Apple was never asked to send — which
                // is exactly the "code never arrives" people report. Ask the
                // engine instead of stat-ing files.
                if !current_ipatool_session_exists()
                    && run_auth_info_once(&tools.ipatool, &keychain)
                        .await
                        .is_ok_and(|found| {
                            normalize_account_email(&found) == normalize_account_email(email)
                        })
                {
                    ipatool_debug_log(&format!(
                        "login ok via auth-info (no account file) engine={engine}"
                    ));
                    auth_stage("catalog-session-saved", email, &format!("engine={engine}"));
                    remember_login_engine(app, &engine);
                    return Ok(engine.clone());
                }
                if !current_ipatool_session_exists() {
                    let detail = format!(
                        "ipatool login engine={engine} finished without writing account. {}",
                        ipatool_session_diag()
                    );
                    ipatool_debug_log(&detail);
                    last_error = with_technical_detail("Вход не удалось сохранить.", &detail);
                    continue;
                }
                match run_auth_info_once(&tools.ipatool, &keychain).await {
                    // Сравниваем приведённые адреса, а не как есть.
                    //
                    // Введённый человеком адрес уже приведён к нижнему регистру,
                    // а ipatool отдаёт тот, что записан у Apple. Если Apple ID
                    // заведён как Ivan@mail.ru, строки не совпадали, и вход
                    // отклонялся сообщением «подтверждён для другого Apple ID» —
                    // при том что аккаунт был тот самый. Строкой выше этот же
                    // адрес сверяется уже через normalize_account_email.
                    Ok(actual) if normalize_account_email(&actual) == normalize_account_email(email) => {
                        ipatool_debug_log(&format!("login saved engine={engine} email={actual}"));
                        if let Ok(mut engines) = state.login_2fa_engine.lock() {
                            engines.remove(email);
                        }
                        remember_login_engine(app, engine);
                        auth_stage("catalog-session-saved", email, &format!("engine={engine}"));
                        return Ok(engine.clone());
                    }
                    Ok(actual) => {
                        last_error = format!(
                            "Вход подтверждён для другого Apple ID ({actual}), ожидался {email}."
                        );
                    }
                    Err(error) => {
                        ipatool_debug_log(&format!(
                            "auth-info after login engine={engine}: {error} {}",
                            ipatool_session_diag()
                        ));
                        last_error = error;
                    }
                }
            }
            Err(error) if error.starts_with("SIGNUM_2FA_REQUIRED") => {
                // Channel (trusted-device push vs SMS) is embedded in the
                // message text by format_login_error when ipatool reported it.
                let channel = if error.contains("SMS") {
                    "sms"
                } else if error.contains("доверенные устройства") {
                    "trusted-device"
                } else {
                    "unknown"
                };
                ipatool_debug_log(&format!(
                    "login 2fa engine={engine} channel={channel} {}",
                    ipatool_session_diag()
                ));
                if let Ok(mut engines) = state.login_2fa_engine.lock() {
                    engines.insert(email.into(), engine.clone());
                }
                auth_stage("catalog-2fa-pending", email, &format!("engine={engine}"));
                return Err(error);
            }
            Err(error) => {
                ipatool_debug_log(&format!("login err engine={engine}: {error}"));
                // Do not submit the same credentials to the fallback engine
                // after Apple has explicitly rejected or locked the account.
                // A second engine cannot change that verdict and only creates
                // another consecutive sign-in attempt.
                if is_fatal_account_error(&error) {
                    auth_stage("catalog-account-rejected", email, "fallback=stopped");
                    return Err(error);
                }
                // Apple transport/auth endpoints returning an opaque 301/204/
                // 403/404/500 response are not made better by submitting the
                // same credentials through a second engine. The old fallback
                // created extra attempts and could trigger Apple's rate limit.
                if is_apple_opaque_auth_response(&error) {
                    auth_stage("catalog-apple-opaque", email, "fallback=stopped");
                    return Err(error);
                }
                last_error = error;
            }
        }
    }
    Err(last_error)
}

#[tauri::command]
async fn login_apple(
    app: AppHandle,
    state: State<'_, AppState>,
    input: LoginInput,
) -> Result<LoginAppleResult, String> {
    if DEMO_MODE {
        demo_set_email(&input.email);
        DEMO_LOGGED_OUT.store(false, Ordering::Relaxed);
        return Ok(LoginAppleResult {
            email: demo_email(),
            needs_2fa: false,
            history_needs_2fa: false,
            history_ready: true,
            history_error: None,
            message: Some("Вход выполнен.".into()),
            catalog_error: None,
        });
    }
    if input.email.trim().is_empty() {
        return Err("Введите Apple ID".into());
    }
    let resending = input.resend_code.unwrap_or(false);
    // Digits only — UI may paste spaces/dashes; ipatool wants exactly 6 numbers.
    let two_factor_digits = input
        .two_factor_code
        .as_ref()
        .map(|v| v.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
        .filter(|v| !v.is_empty());
    let providing_2fa = two_factor_digits.as_ref().is_some_and(|v| v.len() >= 6);

    let email = normalize_account_email(&input.email);
    refresh_v6_identity(&app, Some(&email));
    auth_stage(
        if providing_2fa {
            "catalog-2fa-submit"
        } else if resending {
            "catalog-2fa-resend"
        } else {
            "catalog-login"
        },
        &email,
        "begin",
    );

    // Resolve password: form → memory/keyring (needed when only 2FA is sent).
    let mut store_pw = input.password.trim().to_string();
    if store_pw.is_empty() {
        store_pw = history_secrets::load_history_password(&email).unwrap_or_default();
    }
    if store_pw.is_empty() {
        // 2FA step without a stored password cannot complete login — ask to re-enter.
        return Err(if providing_2fa || resending {
            "Пароль Apple ID не найден. Нажмите «Назад» и введите email с паролем снова.".into()
        } else {
            "Введите Apple ID и пароль".into()
        });
    }
    // Lock password into storage immediately (memory + disk) so history never "loses" it.
    let _ = history_secrets::store_history_password(&email, &store_pw);

    let _kbsync_cancel = KbsyncCancelGuard::new();
    let _guard = state.ipatool_lock.lock().await;
    // A failed update check never blocks sign-in: the last verified or bundled tool remains active.
    let _ = refresh_managed_tools(&app).await;
    cleanup_stale_cookie_lock(false);

    let previous_active = active_account_email(&app);
    // Do not wipe a working ipatool session before login. Go sapfix overwrites
    // `account` on success; deleting it first is why the UI showed
    // «восстановление недоступно» after a no-2FA Signum login.
    if !providing_2fa {
        if let Some(active) = previous_active.as_deref() {
            let _ = backup_ipatool_session_for_account(&app, &active);
            // Account switch: the previous account's session/cookies otherwise
            // linger and contaminate the new login (ipatool reuses the cookie
            // jar), so a second account "does not log in" until the old one is
            // fully removed. The old session was just backed up, so clearing it
            // here is safe — switch_apple_account restores it on demand. Only for
            // a DIFFERENT email; re-login of the same account keeps its session
            // to preserve «восстановление».
            if !active.eq_ignore_ascii_case(&email) {
                reset_ipatool_local_store();
            }
        }
        cleanup_stale_cookie_lock(true);
    }

    let login_result = login_catalog_with_fallback(
        &app,
        state.inner(),
        &email,
        &store_pw,
        two_factor_digits.as_deref().filter(|code| code.len() >= 6),
        resending,
    )
    .await;
    let login_result = match login_result {
        Err(error)
            if providing_2fa
                && !is_invalid_2fa_code_error(&error)
                // Служебный ответ Apple — не сбой связи: повтор стал бы ещё
                // одной засчитанной попыткой входа.
                && !is_apple_opaque_auth_response(&error)
                && is_transient_2fa_check_error(&error) =>
        {
            ipatool_debug_log("login 2fa transient retry");
            tokio::time::sleep(Duration::from_millis(800)).await;
            login_catalog_with_fallback(
                &app,
                state.inner(),
                &email,
                &store_pw,
                two_factor_digits.as_deref().filter(|code| code.len() >= 6),
                false,
            )
            .await
        }
        other => other,
    };
    return match login_result {
        Ok(_) => {
            auth_stage("catalog-login", &email, "ok");
            register_apple_account(&app, &email)?;
            set_active_account(&app, &email)?;
            clear_account_version_cache(&state);
            let session_saved = current_ipatool_session_exists()
                && backup_ipatool_session_for_account(&app, &email).is_ok();
            if !session_saved {
                auth_stage("catalog-session-save", &email, "failed");
                let detail = "Сессия каталога не сохранилась. Войдите в Apple ID снова.".to_string();
                save_last_catalog_error(&detail);
                return Ok(LoginAppleResult {
                    email,
                    needs_2fa: false,
                    history_needs_2fa: false,
                    history_ready: false,
                    history_error: None,
                    message: Some("Apple ID добавлен, но сессия каталога не сохранилась.".into()),
                    catalog_error: Some(detail),
                });
            }
            warm_kbsync_cache(&app, &email);
            // Do not immediately authenticate the same Apple ID again through
            // the native Store client. Engines without a Store signer create
            // that session lazily on the first actual Store request via
            // ensure_session_locked; this avoids two separate Apple sign-ins
            // back-to-back while preserving the native fallback libraries.
            clear_last_catalog_error();
            auth_stage("store-lazy", &email, "deferred-until-request");
            Ok(LoginAppleResult {
                email,
                needs_2fa: false,
                history_needs_2fa: false,
                history_ready: false,
                history_error: None,
                message: Some("Apple ID добавлен. Восстановление приложений доступно.".into()),
                catalog_error: None,
            })
        }
        Err(error) if error.starts_with("SIGNUM_2FA_REQUIRED") => {
            auth_stage("catalog-2fa", &email, "pending");
            Ok(LoginAppleResult {
                email,
                needs_2fa: true,
                history_needs_2fa: false,
                history_ready: false,
                history_error: None,
                message: Some(if resending {
                    "Повторный запрос отправлен. Проверьте доверенные Apple-устройства и дождитесь нового кода.".into()
                } else {
                    "Apple запросил проверочный код. Проверьте доверенные Apple-устройства.".into()
                }),
                catalog_error: None,
            })
        }
        Err(error) if providing_2fa && is_invalid_2fa_code_error(&error) => Ok(LoginAppleResult {
            email,
            needs_2fa: true,
            history_needs_2fa: false,
            history_ready: false,
            history_error: None,
            message: Some(
                "Код не подошёл. Введите код с iPhone ещё раз. Новый запрашивайте, только если пришёл другой."
                    .into(),
            ),
            catalog_error: Some(error),
        }),
        // Код отправлен, а Apple ответила служебным кодом. «Введите тот же код
        // ещё раз» звало бы сразу сделать ещё одну засчитанную попытку —
        // показываем как есть: подождать. Состояние 2FA не трогаем.
        Err(error) if providing_2fa && is_apple_opaque_auth_response(&error) => {
            auth_stage("catalog-2fa-submit", &email, "apple-unavailable");
            Err(error)
        }
        Err(error) if providing_2fa && !is_fatal_account_error(&error) => {
            ipatool_debug_log(&format!(
                "login 2fa check failed: {}",
                technical_excerpt(&error, 800)
            ));
            Err(
                "Не получилось проверить код. Введите тот же код ещё раз — новый запрашивать не нужно."
                    .into(),
            )
        }
        Err(error) if is_fatal_account_error(&error) => {
            auth_stage("catalog-account-rejected", &email, "cleanup");
            history_secrets::delete_all_history_secrets(&email);
            let _ = rollback_to_previous_active_session(&app, previous_active.as_deref());
            save_last_catalog_error(&error);
            Err("Не удалось войти. Проверьте Apple ID и пароль.".into())
        }
        Err(error) if is_setup_error(&error) => {
            save_last_catalog_error(&error);
            Err(error)
        }
        Err(error) => {
            auth_stage("catalog-login", &email, "failed");
            let _ = rollback_to_previous_active_session(&app, previous_active.as_deref());
            // A cookie jar can contain stale/partial cookies after an unrelated
            // failure (for example, Windows rejected a large HTTP header). Only
            // explicit ipatool 2FA markers enter the code flow.
            save_last_catalog_error(&error);
            Err(error)
        }
    };
}

struct HistorySetupOutcome {
    needs_2fa: bool,
    ready: bool,
    error: Option<String>,
    message: String,
}

/// After ipatool login: authenticate history with the same password (SRP → optional 2FA).
async fn setup_history_after_login(
    state: &AppState,
    email: &str,
    password: &str,
    keep_session: bool,
) -> HistorySetupOutcome {
    if password.trim().is_empty() {
        return HistorySetupOutcome {
            needs_2fa: false,
            ready: false,
            error: Some("Пароль для истории не сохранился.".into()),
            message: "Apple ID подключён, но историю настроить не удалось: нет пароля.".into(),
        };
    }

    // Reuse living RAP cookies if any (one page is enough to prove session + seed cache).
    if keep_session {
        if let Some(cookies) = history_secrets::load_history_session(email) {
            if let Ok(success) = apple_history::fetch_history_with_cookies(
                &cookies,
                apple_history::HISTORY_PAGE_BATCHES,
                None,
                None,
            )
            .await
            {
                let _ = history_secrets::store_history_session(email, &success.cookies);
                let page: Vec<history_secrets::CachedHistoryApp> = success
                    .apps
                    .iter()
                    .map(|a| history_secrets::CachedHistoryApp {
                        adam_id: a.adam_id.clone(),
                        name: a.name.clone(),
                        artist: a.artist.clone(),
                        artwork: a.artwork.clone(),
                        price: a.price.clone(),
                        date: a.date.clone(),
                        bundle_id: String::new(),
                    })
                    .collect();
                let _ = history_secrets::merge_history_apps_cache_with_rap(
                    email,
                    &page,
                    success.next_batch_id,
                    success.has_more,
                    true,
                    success.rap_dsid,
                    success.rap_xsrf,
                );
                return HistorySetupOutcome {
                    needs_2fa: false,
                    ready: true,
                    error: None,
                    message: "Apple ID подключён. История покупок готова (сессия сохранена)."
                        .into(),
                };
            }
            history_secrets::delete_history_session_cookies_only(email);
        }
    }

    // Always keep Apple ID password for history — independent of RAP cookie session.
    let _ = history_secrets::store_history_password(email, password);

    // Clear stale pending 2FA; start a fresh idmsa session for history.
    if let Ok(mut slot) = state.history_2fa_pending.lock() {
        *slot = None;
    }

    match apple_history::fetch_purchase_history(email, password, None, None).await {
        Ok(apple_history::HistoryOutcome::Apps(success)) => {
            if keep_session {
                let _ = history_secrets::store_history_session(email, &success.cookies);
            } else {
                history_secrets::delete_history_session_cookies_only(email);
            }
            let _ = history_secrets::store_history_password(email, password);
            let page: Vec<history_secrets::CachedHistoryApp> = success
                .apps
                .iter()
                .map(|a| history_secrets::CachedHistoryApp {
                    adam_id: a.adam_id.clone(),
                    name: a.name.clone(),
                    artist: a.artist.clone(),
                    artwork: a.artwork.clone(),
                    price: a.price.clone(),
                    date: a.date.clone(),
                    bundle_id: String::new(),
                })
                .collect();
            let _ = history_secrets::merge_history_apps_cache_with_rap(
                email,
                &page,
                success.next_batch_id,
                success.has_more,
                true,
                success.rap_dsid,
                success.rap_xsrf,
            );
            HistorySetupOutcome {
                needs_2fa: false,
                ready: true,
                error: None,
                message: "Apple ID подключён. История покупок готова (сессия сохранена).".into(),
            }
        }
        Ok(apple_history::HistoryOutcome::Needs2fa { pending }) => {
            if let Ok(mut slot) = state.history_2fa_pending.lock() {
                *slot = Some(pending);
            }
            let _ = history_secrets::store_history_password(email, password);
            HistorySetupOutcome {
                needs_2fa: true,
                ready: false,
                error: None,
                message: "Введите 6-значный код с iPhone.".into(),
            }
        }
        Err(e) => {
            if matches!(e, apple_history::HistoryError::WrongPassword) {
                // Should not happen right after successful ipatool login with same password.
            }
            HistorySetupOutcome {
                needs_2fa: false,
                ready: false,
                error: Some(e.user_message()),
                message: "Apple ID подключён.".into(),
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SessionState {
    Active,
    Saved,
    Expired,
    Needs2fa,
    Damaged,
    NotSaved,
}

#[derive(Serialize)]
struct AppleAccountInfo {
    email: String,
    active: bool,
    session_saved: bool,
    session_state: SessionState,
    added_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatus {
    authenticated: bool,
    email: Option<String>,
    accounts: Vec<AppleAccountInfo>,
    /// Encrypted password present for purchase-history of the active account.
    #[serde(default)]
    history_password_saved: bool,
    /// RAP cookie session stored — history without re-2FA.
    #[serde(default)]
    history_session_saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_error: Option<String>,
}

fn session_state_from_auth_error(error: &str) -> (SessionState, String) {
    if is_two_factor_error(error) {
        return (
            SessionState::Needs2fa,
            "Введите 6-значный код с iPhone.".into(),
        );
    }
    if is_corrupt_cookie_error(error) || is_keychain_integrity_error(error) {
        return (SessionState::Damaged, format_session_verify_error(error));
    }
    (SessionState::Expired, format_session_verify_error(error))
}

fn report_session_auto(friendly: &str, raw: &str) {
    if is_two_factor_error(raw) || is_two_factor_error(friendly) {
        return;
    }
    // The report headline is derived from both texts and is always English, so
    // the raw tool output goes in as the detail rather than being swapped in.
    diag_log::report_error("session", friendly, Some(raw));
}

async fn resolve_session_state(
    app: &AppHandle,
    email: &str,
    is_active: bool,
) -> (bool, SessionState, Option<String>) {
    if is_active {
        sanitize_ipatool_session_files();
    }
    let saved =
        saved_account_session_exists(app, email) || (is_active && current_ipatool_session_exists());
    if !saved {
        return (
            false,
            SessionState::NotSaved,
            Some("Сессия каталога не сохранена. Войдите в Apple ID снова.".into()),
        );
    }
    if !is_active {
        return (true, SessionState::Saved, None);
    }
    let tools = match resolve_tool_paths(app) {
        Ok(value) => value,
        Err(error) => {
            return (saved, SessionState::Expired, Some(error));
        }
    };
    let keychain = match resolve_ipatool_keychain(app) {
        Ok(value) => value,
        Err(error) => {
            return (saved, SessionState::Damaged, Some(error));
        }
    };
    // The live ~/.ipatool store is shared by every account and is wiped on each
    // account switch, logout and tool update, while this account's session sits
    // in its own backup dir. Probing without putting it back reported
    // "could not be found in the keyring" for a session that was never lost —
    // the most common automatic error report by far.
    if !current_ipatool_session_exists() && saved_account_session_exists(app, email) {
        match restore_ipatool_session_for_account(app, email) {
            Ok(()) => ipatool_debug_log(&format!("auth-info restored session for {email}")),
            Err(error) => ipatool_debug_log(&format!("auth-info restore failed: {error}")),
        }
    }
    let mut args = ipatool_args(&keychain);
    args.extend([
        "auth".into(),
        "info".into(),
        "--format".into(),
        "json".into(),
    ]);
    let started = Instant::now();
    ipatool_debug_log(&format!(
        "auth-info start email={email} ipatool={} helper={:?} {}",
        tools.ipatool,
        sap_helper_path_for(&tools.ipatool),
        ipatool_session_diag()
    ));
    let mut owner = auth_info_owner(&tools.ipatool, &args).await;
    if let Err(error) = &owner {
        if is_corrupt_cookie_error(error) || is_keychain_integrity_error(error) {
            sanitize_ipatool_session_files();
            cleanup_stale_cookie_lock(true);
            owner = auth_info_owner(&tools.ipatool, &args).await;
        }
    }
    // v2 keeps the session in ~/.ipatool/keyring/account, v3 in
    // ~/.ipatool/account. Asking the engine that does not own the layout also
    // answers "not found in the keyring", so try the other one before calling
    // the session lost. This mirrors verify_ipatool_session().
    if owner.as_ref().is_err_and(|error| session_lookup_missed_engine(error)) {
        for (engine, candidate) in login_tool_paths(app) {
            if candidate.ipatool == tools.ipatool {
                continue;
            }
            if let Ok(actual) = auth_info_owner(&candidate.ipatool, &args).await {
                ipatool_debug_log(&format!("auth-info cross-engine hit engine={engine}"));
                remember_login_engine(app, &engine);
                owner = Ok(actual);
                break;
            }
        }
    }
    match owner {
        Ok(actual) if actual == email => {
            ipatool_debug_log(&format!(
                "auth-info ok elapsed_ms={}",
                started.elapsed().as_millis()
            ));
            (true, SessionState::Active, None)
        }
        Ok(actual) => {
            ipatool_debug_log(&format!("auth-info email={actual} expected={email}"));
            (
                saved,
                SessionState::Expired,
                Some(format!(
                    "Сессия принадлежит другому Apple ID. Войдите в {email} снова."
                )),
            )
        }
        Err(error) => {
            let (state, detail) = session_state_from_auth_error(&error);
            ipatool_debug_log(&format!(
                "auth-info err elapsed_ms={} raw={} ui={detail}",
                started.elapsed().as_millis(),
                technical_excerpt(&error, 400)
            ));
            report_session_auto(&detail, &error);
            (saved, state, Some(detail))
        }
    }
}

/// `auth info` on one engine: the Apple ID it owns, or the raw English error
/// (ipatool's own `error` field when it answered, the process output otherwise).
async fn auth_info_owner(program: &str, args: &[String]) -> Result<String, String> {
    match run_process_raw_error(program, args, Duration::from_secs(30), None).await {
        Ok(output) => parse_ipatool_auth_info_email(&output),
        Err(error) => Err(error),
    }
}

#[tauri::command]
async fn get_auth_status(app: AppHandle) -> Result<AuthStatus, String> {
    if demo_active() {
        return Ok(demo_auth_status());
    }
    let cache = load_cache(&app);
    let active = active_account_email(&app);
    let mut accounts = Vec::new();
    for account in &cache.apple_accounts {
        let is_active = active.as_deref() == Some(account.email.as_str());
        let (session_saved, session_state, session_error) =
            resolve_session_state(&app, &account.email, is_active).await;
        if is_active {
            if matches!(
                session_state,
                SessionState::Active | SessionState::Saved
            ) {
                clear_last_catalog_error();
            } else if let Some(ref err) = session_error {
                save_last_catalog_error(err);
            }
        }
        accounts.push(AppleAccountInfo {
            email: account.email.clone(),
            active: is_active,
            session_saved,
            session_state,
            added_at: account.added_at,
            session_error,
        });
    }
    // Treat Saved as signed-in for UI (auth info can lag right after login).
    let authenticated = accounts.iter().any(|account| {
        account.active
            && matches!(
                account.session_state,
                SessionState::Active | SessionState::Saved
            )
    });
    let history_password_saved = active
        .as_deref()
        .map(history_secrets::has_history_password)
        .unwrap_or(false);
    let history_session_saved = active
        .as_deref()
        .map(history_secrets::has_history_session)
        .unwrap_or(false);
    let catalog_error = if authenticated {
        None
    } else {
        load_last_catalog_error().or_else(|| {
            accounts
                .iter()
                .find(|account| account.active)
                .and_then(|account| account.session_error.clone())
        })
    };
    Ok(AuthStatus {
        authenticated,
        email: active,
        accounts,
        history_password_saved,
        history_session_saved,
        catalog_error,
    })
}

#[derive(Deserialize)]
struct AccountEmailInput {
    email: String,
}

#[tauri::command]
async fn switch_apple_account(
    app: AppHandle,
    state: State<'_, AppState>,
    input: AccountEmailInput,
) -> Result<(), String> {
    if DEMO_MODE {
        let target = normalize_account_email(&input.email);
        if target.is_empty() {
            return Err("Укажите Apple ID".into());
        }
        if target == demo_email() {
            DEMO_LOGGED_OUT.store(false, Ordering::Relaxed);
            clear_account_version_cache(&state);
            return Ok(());
        }
        return Err("Такого аккаунта в списке нет. Сначала войдите в Apple ID.".into());
    }
    let email = normalize_account_email(&input.email);
    if email.is_empty() {
        return Err("Укажите Apple ID".into());
    }
    let _kbsync_cancel = KbsyncCancelGuard::new();
    let _guard = state.ipatool_lock.lock().await;
    cleanup_stale_cookie_lock(false);
    if active_account_email(&app).as_deref() == Some(email.as_str()) {
        return Ok(());
    }
    let previous_active = active_account_email(&app);
    if let Some(active) = previous_active.as_deref() {
        backup_ipatool_session_for_account(&app, &active)?;
    }
    reset_ipatool_local_store();
    if let Err(error) = restore_ipatool_session_for_account(&app, &email) {
        let _ = rollback_to_previous_active_session(&app, previous_active.as_deref());
        return Err(error);
    }
    let switch_result = match verify_ipatool_session(&app).await {
        Ok(restored_email) if restored_email == email => set_active_account(&app, &email),
        Ok(restored_email) => Err(format!(
            "Восстановлена сессия другого Apple ID ({restored_email}). \
             Войдите в {email} снова через форму ниже — Apple может запросить код 2FA."
        )),
        Err(error) if is_keychain_integrity_error(&error) => {
            Err("Сессия Apple ID повреждена или сменился ключ ipatool. \
             Выйдите из аккаунта и войдите снова — может потребоваться код 2FA."
                .into())
        }
        Err(error) => Err(with_technical_detail(
            "Сессия устарела. Войдите в этот Apple ID снова — Apple может запросить код.",
            &error,
        )),
    };
    if switch_result.is_err() {
        let _ = rollback_to_previous_active_session(&app, previous_active.as_deref());
    } else {
        clear_account_version_cache(&state);
    }
    switch_result
}

async fn revoke_active_ipatool_session(app: &AppHandle) -> Result<(), String> {
    let tools = resolve_tool_paths(app)?;
    let keychain = resolve_ipatool_keychain(app)?;
    let mut args = ipatool_args(&keychain);
    args.extend(["auth".into(), "revoke".into()]);
    run_process(&tools.ipatool, &args, Duration::from_secs(60), None)
        .await
        .map(|_| ())
}

async fn remove_apple_account_inner(app: &AppHandle, email: &str) -> Result<(), String> {
    if DEMO_MODE {
        demo_sign_out();
        return Ok(());
    }
    let email = normalize_account_email(email);
    if email.is_empty() {
        return Err("Укажите Apple ID".into());
    }
    apple_store::clear_session(&email);
    let is_active = active_account_email(app).as_deref() == Some(email.as_str());
    if is_active {
        let _ = revoke_active_ipatool_session(app).await;
        reset_ipatool_local_store();
        remove_apple_account_from_cache(app, &email)?;
        delete_account_session(app, &email)?;
        let next = load_cache(app)
            .apple_accounts
            .first()
            .map(|account| account.email.clone());
        if let Some(next_email) = next {
            restore_ipatool_session_for_account(app, &next_email)?;
            set_active_account(app, &next_email)?;
        } else {
            clear_active_auth_state(app);
        }
        return Ok(());
    }
    remove_apple_account_from_cache(app, &email)?;
    delete_account_session(app, &email)
}

#[tauri::command]
async fn remove_apple_account(
    app: AppHandle,
    state: State<'_, AppState>,
    input: AccountEmailInput,
) -> Result<(), String> {
    let _kbsync_cancel = KbsyncCancelGuard::new();
    let _guard = state.ipatool_lock.lock().await;
    clear_account_version_cache(&state);
    remove_apple_account_inner(&app, &input.email).await
}

#[tauri::command]
async fn logout_apple(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if DEMO_MODE {
        demo_sign_out();
        clear_account_version_cache(&state);
        return Ok(());
    }
    let _kbsync_cancel = KbsyncCancelGuard::new();
    let Some(email) = active_account_email(&app) else {
        clear_active_auth_state(&app);
        return Ok(());
    };
    let _guard = state.ipatool_lock.lock().await;
    clear_account_version_cache(&state);
    if let Ok(mut slot) = state.region_2fa_pending.lock() {
        *slot = None;
    }
    remove_apple_account_inner(&app, &email).await
}

/// Метка «Apple ответила служебным кодом, а не решением по входу».
const APPLE_UNAVAILABLE_MARKER: &str = "SIGNUM_APPLE_UNAVAILABLE:";

/// Коды, которыми Apple отвечает на вход, когда не говорит ни «да», ни «нет».
const APPLE_OPAQUE_AUTH_STATUSES: [u16; 5] = [301, 204, 403, 404, 500];

/// Служебный код ответа Apple из сырого вывода движка, если он из «немых».
///
/// Движки пишут его по-разному: ipatool-cpp — «something went wrong
/// (status=403)», Go-движок — «authentication request failed after 3 attempts
/// (HTTP 204, 404, 404): …». Ждёт текст в нижнем регистре.
fn apple_opaque_auth_status(lowered: &str) -> Option<u16> {
    let about_sign_in = lowered.contains("login error")
        || lowered.contains("something went wrong")
        || lowered.contains("authentication request")
        || lowered.contains("no usable authentication response")
        || lowered.contains("empty or non-plist");
    if !about_sign_in {
        return None;
    }
    static STATUS: OnceLock<Regex> = OnceLock::new();
    static HTTP_LIST: OnceLock<Regex> = OnceLock::new();
    let status = STATUS.get_or_init(|| Regex::new(r"status\s*=\s*(\d{3})\b").unwrap());
    let http_list =
        HTTP_LIST.get_or_init(|| Regex::new(r"http\s+(\d{3}(?:\s*,\s*\d{3})*)").unwrap());
    let mut codes: Vec<u16> = status
        .captures_iter(lowered)
        .filter_map(|found| found[1].parse::<u16>().ok())
        .collect();
    for found in http_list.captures_iter(lowered) {
        codes.extend(found[1].split(',').filter_map(|code| code.trim().parse::<u16>().ok()));
    }
    // Последний код — тот, на котором движок сдался.
    codes
        .into_iter()
        .rev()
        .find(|code| APPLE_OPAQUE_AUTH_STATUSES.contains(code))
}

/// Apple ответила на вход служебным кодом — второй движок не поможет.
///
/// Принимает и сырой вывод движка, и уже готовое сообщение из
/// `format_login_error`: в обычном режиме (без подробных ошибок) от вывода
/// остаётся только метка, кода ответа в тексте уже нет.
fn is_apple_opaque_auth_response(error: &str) -> bool {
    error.contains(APPLE_UNAVAILABLE_MARKER)
        || apple_opaque_auth_status(&error.to_ascii_lowercase()).is_some()
}

fn parse_key_values(output: &str) -> HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| line.split_once(": "))
        .map(|(k, v)| (k.trim().into(), v.trim().into()))
        .collect()
}

fn device_info_from_values(
    values: &HashMap<String, String>,
    connection: &str,
) -> Result<DeviceInfo, String> {
    let udid = values
        .get("UniqueDeviceID")
        .cloned()
        .ok_or_else(|| "Устройство не вернуло UDID".to_string())?;
    Ok(DeviceInfo {
        udid,
        product_type: values.get("ProductType").cloned(),
        product_version: values.get("ProductVersion").cloned(),
        device_name: values.get("DeviceName").cloned(),
        battery_current_capacity: values
            .get("BatteryCurrentCapacity")
            .and_then(|v| v.parse().ok()),
        connection: connection.to_string(),
    })
}

fn merge_device_connection(existing: &str, incoming: &str) -> String {
    match (existing, incoming) {
        ("both", _) | (_, "both") => "both".into(),
        ("usb", "wifi") | ("wifi", "usb") => "both".into(),
        ("wifi", _) => "wifi".into(),
        ("usb", _) => "usb".into(),
        _ => incoming.to_string(),
    }
}

fn merge_device_into_map(
    map: &mut std::collections::HashMap<String, DeviceInfo>,
    device: DeviceInfo,
) {
    map.entry(device.udid.clone())
        .and_modify(|existing| {
            existing.connection = merge_device_connection(&existing.connection, &device.connection);
            if existing.device_name.is_none() {
                existing.device_name = device.device_name.clone();
            }
            if existing.product_type.is_none() {
                existing.product_type = device.product_type.clone();
            }
            if existing.product_version.is_none() {
                existing.product_version = device.product_version.clone();
            }
            if existing.battery_current_capacity.is_none() {
                existing.battery_current_capacity = device.battery_current_capacity;
            }
        })
        .or_insert(device);
}

fn is_device_missing_error(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("no device found")
        || lower.contains("no device")
        || lower.contains("не найден")
        || lower.contains("not found")
}

/// FairPlay `.sinf` / `SC_Info` missing — common for free-signed IPAs.
/// ideviceinstaller may print WARNING and even exit non-zero while the app
/// still installs and runs (development signing does not need App Store sinf).
fn is_harmless_sinf_warning(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (lower.contains("could not locate")
        || lower.contains("cannot locate")
        || lower.contains("missing"))
        && (lower.contains(".sinf") || lower.contains("sc_info"))
}

/// True when the only "failure" noise is FairPlay sinf warnings (no real install error).
fn is_sinf_warning_only_install_output(text: &str) -> bool {
    let clean = strip_ansi(text);
    if clean.trim().is_empty() {
        return false;
    }
    // Таймаут — не «одни предупреждения». С подробными ошибками к нему
    // приклеен вывод процесса, где у переподписанного IPA всегда есть строки
    // про .sinf; без этой проверки зависшая установка считалась удачной.
    if is_process_timeout_error(clean.trim_start()) {
        return false;
    }
    if !is_harmless_sinf_warning(&clean) {
        return false;
    }
    // Reject if there is a hard failure line besides the sinf warning.
    for line in clean.lines() {
        let l = line.trim().to_ascii_lowercase();
        if l.is_empty() {
            continue;
        }
        if l.contains(".sinf") || l.contains("sc_info") || l.contains("could not locate") {
            continue;
        }
        if l.starts_with("warning") {
            continue;
        }
        if l.contains("error")
            || l.contains("failed")
            || l.contains("unable")
            || l.contains("denied")
            || l.contains("no device")
        {
            return false;
        }
    }
    true
}

/// Сколько ждать, пока ideviceinstaller сам выйдет после «Complete».
const IDEVICEINSTALLER_EXIT_GRACE: Duration = Duration::from_secs(5);

/// Запуск ideviceinstaller на установку или обновление приложения.
fn is_ideviceinstaller_install_command(program: &str, args: &[String]) -> bool {
    // Делим по обоим разделителям: путь из Windows приходит с «\», а проверки
    // гоняются и на Linux, где `Path::file_name` такую строку не разбирает.
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase();
    name.starts_with("ideviceinstaller")
        && args.iter().any(|arg| matches!(arg.as_str(), "install" | "upgrade"))
}

/// Телефон подтвердил установку: в выводе есть «Install: Complete».
///
/// Это единственная строка, которую ideviceinstaller печатает ПОСЛЕ того, как
/// installd закончил работу (статус `Complete` в status_cb). Строки
/// «DONE.» и «Installing '<bundle>'» печатаются раньше: первая — когда IPA
/// залит на телефон, вторая — перед самой отправкой команды установки. Считать
/// их успехом нельзя: приложение в этот момент ещё не ставилось, и отказ
/// телефона («ERROR: Install failed. Got error …») приходит позже.
fn ideviceinstaller_install_completed(lines: &[String]) -> bool {
    let mut completed = false;
    for line in lines {
        let line = strip_ansi(line).trim().to_ascii_lowercase();
        if line.starts_with("error:") || line.contains("apiinternalerror") {
            // Отказ: пусть процесс выйдет сам и вернёт свой код.
            return false;
        }
        let status = line
            .strip_prefix("install")
            .or_else(|| line.strip_prefix("upgrade"))
            .map(|rest| rest.trim_start_matches([' ', ':', '-']));
        // Новые сборки пишут «Install: Complete», старые — «Install - Complete».
        if status == Some("complete") {
            completed = true;
        }
    }
    completed
}

/// Стоит ли после отказа `install` пробовать `upgrade`, как это делает
/// IPA_Downloader.
///
/// Не стоит, когда второй заход заведомо ничего не изменит: человек отменил
/// задание, телефон пропал (для этого есть свой повтор по Wi‑Fi), процесс не
/// ответил за шесть минут (ещё шесть минут ожидания никому не нужны) или сам
/// ideviceinstaller не запустился.
fn install_failure_allows_upgrade_retry(error: &str, cancelled: bool) -> bool {
    !cancelled
        && !is_process_timeout_error(error)
        && !is_device_missing_error(error)
        && !error.starts_with("Не удалось запустить")
}

/// Какую из двух ошибок показать, когда не помогли ни install, ни upgrade.
fn pick_install_error(install_error: String, upgrade_error: String) -> String {
    if is_device_missing_error(&upgrade_error) {
        return device_not_found_message();
    }
    // Общая фраза ничего не объясняет — если upgrade назвал причину точнее,
    // показываем её. Во всех остальных случаях важнее первый отказ: это ответ
    // на то действие, о котором человек просил.
    let install_is_generic = install_error.starts_with("Не удалось выполнить операцию");
    if install_is_generic && !is_process_timeout_error(&upgrade_error) {
        upgrade_error
    } else {
        install_error
    }
}

/// Стоит ли на телефоне та самая сборка, что лежит в IPA.
///
/// Совпадения одного идентификатора мало: приложение могло стоять и раньше.
/// Нужна ещё и та же версия; если её не узнать с любой из сторон — «нет».
fn installed_app_matches_ipa(
    installed: &HashMap<String, InstalledApp>,
    bundle_id: &str,
    ipa_version: Option<&str>,
) -> bool {
    let Some(app) = installed.get(bundle_id) else {
        return false;
    };
    match (ipa_version, app.version.as_deref()) {
        (Some(want), Some(have)) if !want.trim().is_empty() && !have.trim().is_empty() => {
            compare_versions(have, want) == 0
        }
        _ => false,
    }
}

/// Встало ли приложение за время установки, которая кончилась таймаутом.
///
/// Одного «на телефоне стоит эта версия» мало: ту же версию переставляют
/// постоянно (перевыпуск подписи раз в неделю), и старая копия выглядела бы
/// как удачная установка. Поэтому нужен снимок ДО: успех — только когда этой
/// сборки на телефоне не было, а теперь она есть. Нет снимка — не гадаем.
fn timed_out_install_landed(
    before: Option<&HashMap<String, InstalledApp>>,
    after: &HashMap<String, InstalledApp>,
    bundle_id: &str,
    ipa_version: Option<&str>,
) -> bool {
    let Some(before) = before else {
        return false;
    };
    !installed_app_matches_ipa(before, bundle_id, ipa_version)
        && installed_app_matches_ipa(after, bundle_id, ipa_version)
}

fn is_usable_device_udid(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && !value.to_ascii_lowercase().starts_with("demo")
        && is_plausible_udid(value)
}

fn sanitize_saved_device_udid(settings: &mut AppSettings) -> bool {
    let Some(udid) = settings.selected_device_udid.as_deref() else {
        return false;
    };
    if is_usable_device_udid(udid) {
        return false;
    }
    settings.selected_device_udid = None;
    settings.selected_device_connection = None;
    true
}

fn selected_device_udid(app: &AppHandle) -> Option<String> {
    load_settings(app)
        .ok()?
        .selected_device_udid
        .map(|value| value.trim().to_string())
        .filter(|value| is_usable_device_udid(value))
}

fn remember_connected_device(app: &AppHandle, device: &DeviceInfo) {
    if !is_usable_device_udid(&device.udid) {
        return;
    }
    let Ok(mut settings) = load_settings(app) else {
        return;
    };
    let conn = if device.connection == "both" || device.connection.is_empty() {
        "usb".to_string()
    } else {
        device.connection.clone()
    };
    let same_udid = settings
        .selected_device_udid
        .as_deref()
        .is_some_and(|udid| udid == device.udid);
    let same_conn = settings
        .selected_device_connection
        .as_deref()
        .is_some_and(|value| value == conn);
    if same_udid && same_conn {
        return;
    }
    settings.selected_device_udid = Some(device.udid.clone());
    settings.selected_device_connection = Some(conn);
    let _ = save_settings(app.clone(), settings);
}

async fn read_ideviceinfo(
    program: &str,
    udid: Option<&str>,
    network: bool,
) -> Result<HashMap<String, String>, String> {
    let mut args = Vec::new();
    if network {
        args.push("-n".into());
    }
    if let Some(value) = udid {
        args.push("-u".into());
        args.push(value.to_string());
    }
    let output = run_process(program, &args, Duration::from_secs(14), None).await?;
    let values = parse_key_values(&output);
    if values.contains_key("UniqueDeviceID") {
        return Ok(values);
    }
    Err("ideviceinfo не вернул данные устройства".into())
}

async fn probe_device_info(
    program: &str,
    udid: Option<&str>,
    network: bool,
) -> Result<DeviceInfo, String> {
    let values = read_ideviceinfo(program, udid, network).await?;
    device_info_from_values(&values, if network { "wifi" } else { "usb" })
}

async fn probe_device_on_links(
    program: &str,
    udid: Option<&str>,
    prefer_wifi: bool,
) -> Result<DeviceInfo, String> {
    let order = if prefer_wifi {
        [true, false]
    } else {
        [false, true]
    };
    let mut last_missing = None;
    for network in order {
        match probe_device_info(program, udid, network).await {
            Ok(device) => return Ok(device),
            Err(error) if is_device_missing_error(&error) => {
                last_missing = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_missing.unwrap_or_else(device_not_found_message))
}

async fn get_device_info_inner(app: &AppHandle) -> Result<DeviceInfo, String> {
    if demo_active() {
        return Ok(demo_device());
    }
    let tools = resolve_tool_paths(app)?;
    require_idevice_tools(&tools)?;
    let udid = selected_device_udid(app);
    let prefer_wifi = selected_device_connection(app) == "wifi";

    if let Some(udid) = udid.as_deref() {
        if let Ok(device) = probe_device_on_links(&tools.ideviceinfo, Some(udid), prefer_wifi).await
        {
            remember_connected_device(app, &device);
            return Ok(device);
        }
    }

    // Saved UDID may be leftover from demo mode or another phone.
    // Use whatever iPhone is actually on USB / Wi‑Fi.
    if let Ok(device) = probe_device_on_links(&tools.ideviceinfo, None, false).await {
        remember_connected_device(app, &device);
        return Ok(device);
    }
    match probe_device_on_links(&tools.ideviceinfo, None, true).await {
        Ok(device) => {
            remember_connected_device(app, &device);
            Ok(device)
        }
        Err(_) => Err(device_not_found_message()),
    }
}

fn is_plausible_udid(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 20
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

async fn list_paired_udids(tools: &ToolPaths) -> Vec<String> {
    let (program, args) = if let Some(idevice_id) = tools.idevice_id.as_deref() {
        (idevice_id, vec!["-l".into()])
    } else {
        (tools.idevicepair.as_str(), vec!["list".into()])
    };
    let output = match run_process_raw_error(
        program,
        &args,
        Duration::from_secs(12),
        None,
    )
    .await {
        Ok(output) => output,
        Err(error) => {
            ipatool_debug_log(&format!(
                "device-discovery paired-list: {}",
                technical_excerpt(&error, 180)
            ));
            return Vec::new();
        }
    };
    output
        .lines()
        .map(str::trim)
        .filter(|value| is_plausible_udid(value))
        .map(str::to_string)
        .collect()
}

/// USB + Wi‑Fi discovery via libimobiledevice (`-n` network mode).
async fn list_connected_devices_inner(app: &AppHandle) -> Result<Vec<DeviceInfo>, String> {
    if demo_active() {
        return Ok(vec![demo_device()]);
    }
    let tools = resolve_tool_paths(app)?;
    require_idevice_tools(&tools)?;
    // Inspect every paired device: transfer needs both iPhones, including the second USB device.
    let paired = list_paired_udids(&tools).await;
    ipatool_debug_log(&format!(
        "device-discovery tools info={} pair={} paired={}",
        tools.ideviceinfo,
        tools.idevicepair,
        paired.len()
    ));
    let mut map: std::collections::HashMap<String, DeviceInfo> = std::collections::HashMap::new();

    // USB: default / first cable device. Its error is already in words
    // («Доверять», «Разблокируйте»); keep it, an empty list explained nothing.
    let mut usb_error = None;
    match probe_device_info(&tools.ideviceinfo, None, false).await {
        Ok(device) => merge_device_into_map(&mut map, device),
        Err(error) => {
            ipatool_debug_log(&format!("device-discovery usb-default: {}", technical_excerpt(&error, 180)));
            usb_error = Some(error);
        }
    }
    for chunk in paired.chunks(8) {
        let probes = chunk.iter().map(|udid| probe_device_info(&tools.ideviceinfo, Some(udid), false));
        for result in futures_util::future::join_all(probes).await {
            match result {
                Ok(device) => merge_device_into_map(&mut map, device),
                Err(error) => ipatool_debug_log(&format!("device-discovery usb-paired: {}", technical_excerpt(&error, 180))),
            }
        }
    }

    // Wi‑Fi / network lockdown devices (same Wi‑Fi + prior USB pair / Finder Wi‑Fi)
    match probe_device_info(&tools.ideviceinfo, None, true).await {
        Ok(device) => merge_device_into_map(&mut map, device),
        Err(error) => ipatool_debug_log(&format!("device-discovery wifi-default: {}", technical_excerpt(&error, 180))),
    }
    for chunk in paired.chunks(8) {
        let probes = chunk.iter().map(|udid| probe_device_info(&tools.ideviceinfo, Some(udid), true));
        for result in futures_util::future::join_all(probes).await {
            match result {
                Ok(device) => merge_device_into_map(&mut map, device),
                Err(error) => ipatool_debug_log(&format!("device-discovery wifi-paired: {}", technical_excerpt(&error, 180))),
            }
        }
    }

    let mut devices: Vec<DeviceInfo> = map.into_values().collect();
    if devices.is_empty() {
        if let Ok(device) = get_device_info_inner(app).await {
            devices.push(device);
        }
    }
    if devices.is_empty() {
        ipatool_debug_log("device-discovery result=0");
        if !usbmuxd_reachable() {
            return Err(usbmuxd_missing_message());
        }
        return Err(usb_error
            .filter(|error| !is_device_missing_error(error))
            .unwrap_or_else(device_not_found_message));
    }
    devices.sort_by(|left, right| {
        left.device_name
            .cmp(&right.device_name)
            .then(left.udid.cmp(&right.udid))
    });
    devices.dedup_by(|left, right| left.udid == right.udid);
    ipatool_debug_log(&format!("device-discovery result={}", devices.len()));
    Ok(devices)
}

#[tauri::command]
async fn list_connected_devices(app: AppHandle) -> Result<Vec<DeviceInfo>, String> {
    list_connected_devices_inner(&app).await
}

/// UI reply while IPA signing waits for GSA 2FA.
/// - `code` with 6 digits → submit code
/// - `action` = "resend" → ask Apple to send the code again
/// - empty / cancel → abort
#[tauri::command]
fn submit_signing_2fa(
    state: State<'_, AppState>,
    code: Option<String>,
    action: Option<String>,
) -> Result<(), String> {
    let action = action.unwrap_or_default();
    let code = code.unwrap_or_default();
    let payload = if action.eq_ignore_ascii_case("resend") {
        "__resend__".to_string()
    } else {
        let digits: String = code
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(6)
            .collect();
        if digits.len() >= 6 {
            digits
        } else if code.trim().is_empty() || action.eq_ignore_ascii_case("cancel") {
            String::new()
        } else {
            return Err("Введите 6-значный код с iPhone или SMS.".into());
        }
    };
    let mut slot = state
        .signing_2fa_reply
        .lock()
        .map_err(|_| "Внутренняя ошибка 2FA.".to_string())?;
    if let Some(tx) = slot.take() {
        tx.send(payload)
            .map_err(|_| "Сессия 2FA истекла. Запустите подпись снова.".to_string())?;
        Ok(())
    } else {
        Err("Сейчас код 2FA для подписи не запрашивается.".into())
    }
}

#[tauri::command]
fn inspect_ipa(path: String) -> ipa_inspect::IpaInspect {
    ipa_inspect::inspect_ipa_file(Path::new(path.trim()))
}

#[tauri::command]
async fn list_custom_certs(app: AppHandle) -> Result<Vec<apple_signing::SavedCertInfo>, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("{e}"))?;
    Ok(apple_signing::list_custom_certs(&dir))
}

#[derive(Debug, Deserialize)]
struct ImportCustomCertInput {
    name: String,
    p12_path: String,
    prov_path: String,
    #[serde(default)]
    password: Option<String>,
}

#[tauri::command]
async fn import_custom_cert(app: AppHandle, input: ImportCustomCertInput) -> Result<(), String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("{e}"))?;
    let name = input.name.trim();
    let p12 = PathBuf::from(input.p12_path.trim());
    let prov = PathBuf::from(input.prov_path.trim());
    apple_signing::save_custom_cert(
        &dir,
        name,
        &p12,
        &prov,
        input.password.as_deref().unwrap_or(""),
    )?;
    Ok(())
}

#[tauri::command]
async fn delete_custom_cert(app: AppHandle, name: String) -> Result<(), String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("{e}"))?;
    apple_signing::delete_custom_cert(&dir, name.trim())
}

#[tauri::command]
async fn enqueue_sign_and_install(
    app: AppHandle,
    state: State<'_, AppState>,
    input: apple_signing::SignAndInstallInput,
) -> Result<String, String> {
    let ipa_path = PathBuf::from(input.ipa_path.trim());
    if !ipa_path.is_file() {
        return Err("SIGN_IPA_FILE_REQUIRED".into());
    }
    if !ipa_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ipa"))
    {
        return Err("SIGN_IPA_FILE_REQUIRED".into());
    }

    let auth = get_auth_status(app.clone()).await?;
    let is_custom_cert = matches!(input.cert_mode, apple_signing::CertMode::Custom);
    let signing_access = auth.authenticated
        || (auth.email.is_some() && auth.history_password_saved);
    // Свой сертификат (.p12) подписывает без Apple ID — входа не нужно.
    if !signing_access && !is_custom_cert {
        return Err("SIGN_IPA_AUTH_REQUIRED".into());
    }

    if input.install == false && !input.save_signed {
        return Err("Выберите установку на устройство и/или сохранение подписанного IPA.".into());
    }

    let requested_udid = input
        .udid
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .or_else(|| selected_device_udid(&app));
    let devices = if input.install {
        list_connected_devices_inner(&app).await?
    } else {
        list_connected_devices_inner(&app).await.unwrap_or_default()
    };
    let connected_udid = requested_udid.as_ref().and_then(|udid| {
        devices
            .iter()
            .find(|device| device.udid == *udid)
            .map(|device| device.udid.clone())
    });

    if input.install {
        let udid = requested_udid
            .clone()
            .ok_or_else(|| "SIGN_IPA_DEVICE_REQUIRED".to_string())?;
        if connected_udid.is_none() {
            return Err("SIGN_IPA_DEVICE_DISCONNECTED".into());
        }
        let _ = select_device(app.clone(), SelectDeviceInput { udid }).await;
    }
    if input.save_signed {
        if let Some(dir) = input
            .output_dir
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            if !PathBuf::from(dir).is_dir() {
                return Err("Папка для сохранения подписанного IPA не найдена.".into());
            }
        }
    }

    let meta = apple_signing::parse_ipa_metadata(&ipa_path).unwrap_or(apple_signing::IpaMetadata {
        bundle_id: String::new(),
        name: ipa_path
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("IPA")
            .to_string(),
        version: None,
        min_ios: None,
    });
    let title = if meta.name.is_empty() {
        "Подпись IPA".into()
    } else {
        format!("Подпись · {}", meta.name)
    };
    let mut input = input;
    input.udid = if input.install {
        requested_udid
    } else {
        connected_udid.or(requested_udid)
    };
    input.ipa_path = ipa_path.to_string_lossy().into_owned();

    let id = Uuid::new_v4().to_string();
    let job_subtitle = if input.install {
        format!(
            "{} · {}",
            meta.bundle_id,
            input.udid.as_deref().unwrap_or("iPhone")
        )
    } else {
        format!("{} · сохранение IPA", meta.bundle_id)
    };
    let job = InstallJob {
        id: id.clone(),
        title,
        subtitle: job_subtitle,
        phase: "queue".into(),
        percent: 0.0,
        message: "Добавлено в очередь".into(),
        status: JobStatus::Queued,
        error: None,
        created_at: now(),
        kind: "install".into(),
        retryable: true,
        retry_payload: Some(RetryPayload::SignAndInstall(input.clone())),
    };
    state.jobs.lock().await.insert(id.clone(), job.clone());
    register_cancel_flag(&state, &id);
    persist_pending_job(
        &app,
        PendingJobRecord {
            id: id.clone(),
            kind: "sign_install".into(),
            title: job.title.clone(),
            subtitle: job.subtitle.clone(),
            catalog_input: None,
            local_path: None,
            sign_input: Some(input.clone()),
            created_at: job.created_at,
        },
    );
    state
        .queue
        .send(QueuedInstall::SignAndInstall {
            job_id: id.clone(),
            input,
        })
        .await
        .map_err(|_| "Очередь установок остановлена".to_string())?;
    Ok(id)
}

#[tauri::command]
async fn get_signing_status(app: AppHandle) -> Result<apple_signing::SigningStatus, String> {
    let auth = get_auth_status(app).await?;
    let signing_access = auth.authenticated
        || (auth.email.is_some() && auth.history_password_saved);
    Ok(apple_signing::SigningStatus {
        authenticated: signing_access,
        email: auth.email,
        team_id: None,
        team_name: None,
        team_kind: None,
        cert_ready: false,
        message: if signing_access {
            Some("Apple ID готов для подписи файлов.".into())
        } else {
            Some("Войдите в Apple ID на странице «Аккаунт».".into())
        },
    })
}

#[derive(Deserialize)]
struct SelectDeviceInput {
    udid: String,
}

#[tauri::command]
async fn select_device(app: AppHandle, input: SelectDeviceInput) -> Result<DeviceInfo, String> {
    let current_busy = app.state::<AppState>().queue.current.lock().await.is_some();
    let pending_busy = !app.state::<AppState>().queue.pending.lock().await.is_empty();
    if current_busy || pending_busy {
        return Err("Завершите или отмените очередь перед сменой iPhone.".into());
    }
    let udid = input.udid.trim().to_string();
    if udid.is_empty() {
        return Err("Не указан UDID устройства".into());
    }
    if demo_active() {
        let device = demo_device();
        let mut settings = load_settings(&app)?;
        settings.selected_device_udid = Some(device.udid.clone());
        settings.selected_device_connection = Some("usb".into());
        save_settings(app, settings)?;
        return Ok(device);
    }
    let tools = resolve_tool_paths(&app)?;
    // Prefer USB; fall back to Wi‑Fi (network lockdown).
    let device = match probe_device_info(&tools.ideviceinfo, Some(&udid), false).await {
        Ok(d) => d,
        Err(usb_err) => match probe_device_info(&tools.ideviceinfo, Some(&udid), true).await {
            Ok(d) => d,
            Err(_) => {
                return Err(if is_device_missing_error(&usb_err) {
                    device_not_found_message()
                } else {
                    usb_err
                });
            }
        },
    };
    let mut settings = load_settings(&app)?;
    settings.selected_device_udid = Some(device.udid.clone());
    // Prefer USB when both available; otherwise use what we probed.
    let conn = if device.connection == "both" {
        "usb".to_string()
    } else {
        device.connection.clone()
    };
    settings.selected_device_connection = Some(conn);
    save_settings(app, settings)?;
    Ok(device)
}

#[tauri::command]
async fn get_device_info(app: AppHandle) -> Result<DeviceInfo, String> {
    get_device_info_inner(&app).await
}

fn is_trust_dialog_pair_error(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("trust dialog")
        || lower.contains("please accept the trust")
        || lower.contains("accept the trust")
        || lower.contains("user denied the trust")
        || lower.contains("pairing dialog")
        || (lower.contains("pair") && lower.contains("trust"))
}

fn friendly_pair_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if is_trust_dialog_pair_error(raw) {
        return "На iPhone нажмите «Доверять» (и введите код-пароль, если попросит).\n\n\
                Телефон должен быть разблокирован и подключён кабелем.\n\
                После «Доверять» снова нажмите «Переподключить доверие»."
            .into();
    }
    if is_device_missing_error(raw)
        || lower.contains("no device")
        || lower.contains("device not found")
    {
        return device_not_found_message();
    }
    if lower.contains("password protected") || lower.contains("locked") {
        return "Разблокируйте iPhone и повторите.".into();
    }
    "Не удалось установить доверие с iPhone.\n\n\
     Разблокируйте телефон, подключите USB-кабель, нажмите «Доверять» на экране iPhone \
     и снова «Переподключить доверие»."
        .into()
}

#[tauri::command]
async fn trust_device(app: AppHandle) -> Result<String, String> {
    if demo_active() {
        return Ok("iPhone уже доверяет этому компьютеру.".into());
    }
    let tools = resolve_tool_paths(&app)?;
    require_idevice_tools(&tools)?;
    let udid = selected_device_udid(&app);

    let mut pair_args = Vec::new();
    if let Some(ref id) = udid {
        pair_args.extend(["-u".into(), id.clone()]);
    }
    pair_args.push("pair".into());

    // First pair usually shows Trust on phone; user taps Trust; later retries succeed.
    let mut last_raw = String::new();
    for attempt in 0..12 {
        match run_process_raw_error(
            &tools.idevicepair,
            &pair_args,
            Duration::from_secs(40),
            None,
        )
        .await
        {
            Ok(_) => {
                let info_ok = read_ideviceinfo(&tools.ideviceinfo, udid.as_deref(), false)
                    .await
                    .is_ok()
                    || read_ideviceinfo(&tools.ideviceinfo, udid.as_deref(), true)
                        .await
                        .is_ok();
                if info_ok {
                    return Ok(
                        "Готово. iPhone доверяет этому компьютеру. Можно устанавливать приложения."
                            .into(),
                    );
                }
                let mut validate_args = Vec::new();
                if let Some(ref id) = udid {
                    validate_args.extend(["-u".into(), id.clone()]);
                }
                validate_args.push("validate".into());
                if run_process_raw_error(
                    &tools.idevicepair,
                    &validate_args,
                    Duration::from_secs(15),
                    None,
                )
                .await
                .is_ok()
                {
                    return Ok(
                        "Готово. iPhone доверяет этому компьютеру. Можно устанавливать приложения."
                            .into(),
                    );
                }
                last_raw = "pair reported ok but device not ready".into();
            }
            Err(error) => {
                last_raw = error;
                if is_trust_dialog_pair_error(&last_raw) {
                    if attempt + 1 < 12 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    return Err(friendly_pair_error(&last_raw));
                }
                if attempt < 2 {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                return Err(friendly_pair_error(&last_raw));
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    Err(friendly_pair_error(&last_raw))
}

fn plist_string(dict: &plist::Dictionary, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        dict.get(*key)
            .and_then(|v| v.as_string())
            .map(str::to_string)
    })
}

async fn itunes_lookup_app(adam_id: &str) -> Option<JsonValue> {
    let adam_id = adam_id.trim();
    if normalized_app_store_id(Some(adam_id)).is_none() { return None; }
    let client = reqwest::Client::builder().timeout(Duration::from_secs(12)).build().ok()?;
    for country in ["ru", "us", "kz", "by", "ua", "gb", "de", ""] {
        let response = if country.is_empty() {
            client.get("https://itunes.apple.com/lookup").query(&[("id", adam_id), ("entity", "software")]).send().await
        } else {
            client.get("https://itunes.apple.com/lookup").query(&[("id", adam_id), ("country", country), ("entity", "software")]).send().await
        };
        let Ok(response) = response else { continue };
        if !response.status().is_success() { continue; }
        let Ok(value) = response.json::<JsonValue>().await else { continue };
        if let Some(app) = value.get("results").and_then(JsonValue::as_array).and_then(|items| items.first()) {
            return Some(app.clone());
        }
    }
    None
}

fn plist_item_id(value: &plist::Value) -> Option<String> {
    match value {
        plist::Value::String(value)
            if normalized_app_store_id(Some(value.trim())).is_some() =>
        {
            Some(value.trim().to_string())
        }
        plist::Value::Integer(value) => value.as_unsigned().filter(|id| *id > 0).map(|id| id.to_string()),
        plist::Value::Data(bytes) => plist::Value::from_reader(Cursor::new(bytes))
            .ok()
            .and_then(|value| plist_item_id(&value)),
        plist::Value::Dictionary(dict) => {
            for key in ["itemId", "iTunesStoreItemIdentifier", "trackId"] {
                if let Some(item) = dict.get(key).and_then(plist_item_id) {
                    return Some(item);
                }
            }
            dict.values().filter(|value| matches!(value,
                plist::Value::Dictionary(_) | plist::Value::Array(_) | plist::Value::Data(_)
            )).find_map(plist_item_id)
        }
        plist::Value::Array(values) => values.iter().filter(|value| matches!(value,
            plist::Value::Dictionary(_) | plist::Value::Array(_) | plist::Value::Data(_)
        )).find_map(plist_item_id),
        _ => None,
    }
}

fn apps_from_plist(output: &str) -> Vec<InstalledApp> {
    let start = output.find("<?xml").unwrap_or(0);
    let Ok(value) = plist::Value::from_reader_xml(Cursor::new(
        output.as_bytes().get(start..).unwrap_or_default(),
    )) else {
        return vec![];
    };
    let values: Vec<&plist::Value> = if let Some(array) = value.as_array() {
        array.iter().collect()
    } else if let Some(dict) = value.as_dictionary() {
        dict.get("Applications")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_else(|| dict.values().collect())
    } else {
        vec![]
    };
    values
        .into_iter()
        .filter_map(|item| {
            let dict = item.as_dictionary()?;
            let bundle_id = plist_string(dict, &["CFBundleIdentifier", "bundleIdentifier"])?;
            Some(InstalledApp {
                bundle_id,
                name: plist_string(dict, &["CFBundleDisplayName", "CFBundleName", "Path"]),
                version: plist_string(
                    dict,
                    &["CFBundleShortVersionString", "CFBundleVersion", "version"],
                ),
                app_store_id: dict.get("iTunesMetadata")
                    .and_then(plist_item_id)
                    .or_else(|| dict.get("iTunesStoreItemIdentifier").and_then(plist_item_id)),
                is_placeholder: dict
                    .get("IsPlaceholder")
                    .and_then(plist::Value::as_boolean)
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn apps_from_lines(output: &str) -> Vec<InstalledApp> {
    static BUNDLE: OnceLock<Regex> = OnceLock::new();
    static VERSION: OnceLock<Regex> = OnceLock::new();
    let bundle =
        BUNDLE.get_or_init(|| Regex::new(r"([A-Za-z0-9][A-Za-z0-9.-]+\.[A-Za-z0-9.-]+)").unwrap());
    let version =
        VERSION.get_or_init(|| Regex::new(r"\d+(?:\.\d+){1,4}(?:[-+][A-Za-z0-9.]+)?").unwrap());
    output
        .lines()
        .filter_map(|line| {
            let found = bundle.find(line)?;
            let id = found.as_str().to_string();
            let rest = &line[found.end()..];
            let ver = version.find(rest).map(|m| m.as_str().to_string());
            let mut name = line.replace(&id, "");
            if let Some(v) = &ver {
                name = name.replace(v, "");
            }
            Some(InstalledApp {
                bundle_id: id,
                name: Some(
                    name.trim_matches(|c: char| c.is_whitespace() || "-,".contains(c))
                        .to_string(),
                )
                .filter(|v| !v.is_empty()),
                version: ver,
                app_store_id: None,
                is_placeholder: false,
            })
        })
        .collect()
}

async fn list_installed_apps_inner(app: &AppHandle) -> Result<Vec<InstalledApp>, String> {
    list_apps_for_device(app, selected_device_udid(app).as_deref()).await
}

fn installed_app_list_attempts(udid: Option<&str>) -> Vec<Vec<String>> {
    let mut attempts = Vec::new();
    for network in [false, true] {
        for xml in [true, false] {
            let mut args = Vec::new();
            if network { args.push("-n".into()); }
            if let Some(udid) = udid { args.extend(["-u".into(), udid.into()]); }
            args.extend(["list".into(), "--user".into()]);
            if xml {
                args.extend([
                    "--xml".into(),
                    "--attribute".into(), "CFBundleIdentifier".into(),
                    "--attribute".into(), "CFBundleDisplayName".into(),
                    "--attribute".into(), "CFBundleShortVersionString".into(),
                    "--attribute".into(), "CFBundleVersion".into(),
                    "--attribute".into(), "IsPlaceholder".into(),
                    "--attribute".into(), "iTunesMetadata".into(),
                ]);
                // IPA_Downloader 4.3's installer fork exposes this switch.
                // Keep it as a first attempt; stock ideviceinstaller falls back
                // to the same query without it.
                let mut with_placeholders = args.clone();
                let at = with_placeholders.iter().position(|arg: &String| arg == "--xml").unwrap();
                with_placeholders.insert(at, "--placeholders".into());
                attempts.push(with_placeholders);
            }
            attempts.push(args);
        }
    }
    attempts
}

async fn list_apps_for_device(app: &AppHandle, udid: Option<&str>) -> Result<Vec<InstalledApp>, String> {
    if demo_active() { return Ok(Vec::new()); }
    let tools = resolve_tool_paths(app)?;
    require_idevice_tools(&tools)?;
    let mut last_err = String::new();
    for args in installed_app_list_attempts(udid) {
        match run_process(
            &tools.ideviceinstaller,
            &args,
            Duration::from_secs(35),
            None,
        )
        .await
        {
            Ok(output) => {
                let apps = if args.iter().any(|a| a == "--xml") {
                    apps_from_plist(&output)
                } else {
                    apps_from_lines(&output)
                };
                if !apps.is_empty() || !is_device_missing_error(&output) {
                    return Ok(apps);
                }
            }
            Err(error) => {
                last_err = error;
            }
        }
    }
    if !last_err.is_empty() {
        return Err(if is_device_missing_error(&last_err) {
            device_not_found_message()
        } else {
            last_err
        });
    }
    Ok(Vec::new())
}

#[derive(Debug, Deserialize)]
struct PlaceholderRestoreItem {
    title: String,
    bundle_id: String,
    app_store_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaceholderRestoreInput {
    items: Vec<PlaceholderRestoreItem>,
}

async fn enqueue_placeholder_restore_locked(
    app: &AppHandle,
    state: &AppState,
    item: PlaceholderRestoreItem,
    device_ios: &str,
) -> Result<String, String> {
    let bundle_id = item.bundle_id.trim();
    if bundle_id.is_empty() || bundle_id.len() > 255 {
        return Err("У сгруженного приложения некорректный Bundle ID.".into());
    }
    let app_store_id = match normalized_app_store_id(item.app_store_id.as_deref()) {
        Some(id) => id.to_string(),
        None => apple_store::lookup_adam_id(bundle_id)
            .await
            .ok_or_else(|| "Не удалось определить App Store ID сгруженного приложения.".to_string())?,
    };
    let owned = load_owned_version_ids(app, &app_store_id).await?;
    if owned.ids.is_empty() {
        return Err("Этого приложения нет в покупках активного Apple ID.".into());
    }
    let mut selected = None;
    for external_id in owned.ids {
        let Some(min_ios) = resolve_build_min_ios(app, &app_store_id, &external_id).await else {
            continue;
        };
        if device_supports_min_ios(Some(device_ios), Some(&min_ios)) == Some(true) {
            selected = Some((external_id, min_ios));
            break;
        }
    }
    let Some((external_version_id, min_ios)) = selected else {
        return Err(format!("В покупках нет версии, совместимой с iOS {device_ios}."));
    };
    let input = CatalogInstallInput {
        title: item.title.trim().to_string(),
        bundle_id: bundle_id.to_string(),
        app_store_id: Some(app_store_id),
        in_store: false,
        min_ios: Some(min_ios),
        version: None,
        external_version_id: Some(external_version_id),
        already_owned: true,
        temporary_us_region: false,
        temporary_region: None,
    };
    enqueue_catalog_install_inner(app.clone(), state, input).await
}

/// Restore one or more apps that iOS has offloaded. Their app records are
/// verified against the currently selected device; only versions owned by the
/// active Apple ID and compatible with that device are queued.
#[tauri::command]
async fn enqueue_placeholder_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    input: PlaceholderRestoreInput,
) -> Result<CatalogInstallManyResult, String> {
    if input.items.is_empty() || input.items.len() > 500 {
        return Err("Выберите от 1 до 500 сгруженных приложений.".into());
    }
    require_auth(&app)?;
    let installed = list_installed_apps_inner(&app).await?;
    let placeholders: HashSet<String> = installed.into_iter()
        .filter(|item| item.is_placeholder)
        .map(|item| item.bundle_id)
        .collect();
    let device = get_device_info_inner(&app).await?;
    let device_ios = device.product_version.as_deref()
        .ok_or_else(|| "Не удалось определить версию iOS устройства.".to_string())?;
    let _guard = state.ipatool_lock.lock().await;
    ensure_active_ipatool_session(&app).await?;
    let mut result = CatalogInstallManyResult { job_ids: Vec::new(), skipped: Vec::new() };
    for item in input.items {
        let bundle_id = item.bundle_id.trim().to_string();
        if !placeholders.contains(&bundle_id) {
            result.skipped.push(CatalogInstallSkipped {
                app_id: bundle_id,
                reason: "Приложение больше не отмечено как сгруженное на устройстве.".into(),
            });
            continue;
        }
        let title = item.title.trim().to_string();
        match enqueue_placeholder_restore_locked(&app, state.inner(), item, device_ios).await {
            Ok(id) => result.job_ids.push(id),
            Err(reason) => result.skipped.push(CatalogInstallSkipped {
                app_id: bundle_id,
                reason: format!("{}: {reason}", if title.is_empty() { "Приложение" } else { &title }),
            }),
        }
    }
    Ok(result)
}

#[tauri::command]
async fn list_installed_apps(app: AppHandle) -> Result<Vec<InstalledApp>, String> {
    list_installed_apps_inner(&app).await
}

async fn update_job(
    app: &AppHandle,
    id: &str,
    phase: &str,
    percent: f64,
    message: &str,
    status: JobStatus,
    error: Option<String>,
) {
    let state = app.state::<AppState>();
    let mut jobs = state.jobs.lock().await;
    if let Some(job) = jobs.get_mut(id) {
        job.phase = phase.into();
        job.percent = percent.clamp(0.0, 100.0);
        job.message = message.into();
        job.status = status.clone();
        job.error = error.clone();
    }
    drop(jobs);
    if matches!(status, JobStatus::Failed) {
        let shown = error
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or(message);
        diag_log::report_error("job", shown, error.as_deref());
    }
    let _ = app.emit(
        "install-progress",
        ProgressEvent {
            job_id: id.into(),
            phase: phase.into(),
            percent: percent.clamp(0.0, 100.0),
            message: message.into(),
            status,
            error,
        },
    );
}

fn extract_percent(line: &str) -> Option<f64> {
    // ipatool-cpp prints "PROGRESS:42" on a piped stdout instead of a percent
    // bar, so match that first; the Go engines keep printing "42%".
    static CPP_PROGRESS: OnceLock<Regex> = OnceLock::new();
    static PERCENT: OnceLock<Regex> = OnceLock::new();
    let captures = CPP_PROGRESS
        .get_or_init(|| Regex::new(r"PROGRESS:(\d{1,3})").unwrap())
        .captures(line)
        .or_else(|| {
            PERCENT
                .get_or_init(|| Regex::new(r"(\d{1,3}(?:[.,]\d+)?)\s*%").unwrap())
                .captures(line)
        })?;
    captures
        .get(1)?
        .as_str()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .map(|v| v.clamp(0.0, 100.0))
}

fn megabytes_to_bytes(value: f64) -> u64 {
    (value * 1_048_576.0).round() as u64
}

fn format_download_progress_label(raw: f64) -> String {
    format!("Загрузка IPA… {:.0}%", raw.clamp(0.0, 100.0))
}

fn download_progress_from_bytes(current_bytes: u64, total_bytes: u64) -> (f64, String) {
    let raw = if total_bytes > 0 {
        (current_bytes as f64 / total_bytes as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    (raw, format_download_progress_label(raw))
}

fn parse_download_progress(line: &str) -> Option<(f64, String, Option<u64>)> {
    let clean = strip_ansi(line);
    if clean.is_empty() {
        return None;
    }

    if let Some(percent) = extract_percent(&clean) {
        return Some((percent, format_download_progress_label(percent), None));
    }

    static BYTE_RATIO: OnceLock<Regex> = OnceLock::new();
    let ratio = BYTE_RATIO
        .get_or_init(|| Regex::new(r"(\d+(?:[.,]\d+)?)\s*/\s*(\d+(?:[.,]\d+)?)").unwrap());
    if let Some(caps) = ratio.captures(&clean) {
        let current: f64 = caps.get(1)?.as_str().replace(',', ".").parse().ok()?;
        let total: f64 = caps.get(2)?.as_str().replace(',', ".").parse().ok()?;
        if total > 0.0 {
            let current_bytes = megabytes_to_bytes(current);
            let total_bytes = megabytes_to_bytes(total);
            let (raw, message) = download_progress_from_bytes(current_bytes, total_bytes);
            return Some((raw, message, Some(total_bytes)));
        }
    }

    static BYTE_UNITS: OnceLock<Regex> = OnceLock::new();
    let units = BYTE_UNITS.get_or_init(|| {
        Regex::new(r"(?i)(\d+(?:[.,]\d+)?)\s*(B|KB|MB|GB)\s*/\s*(\d+(?:[.,]\d+)?)\s*(B|KB|MB|GB)")
            .unwrap()
    });
    if let Some(caps) = units.captures(&clean) {
        let current = caps
            .get(1)?
            .as_str()
            .replace(',', ".")
            .parse::<f64>()
            .ok()?;
        let current_unit = caps.get(2)?.as_str();
        let total = caps
            .get(3)?
            .as_str()
            .replace(',', ".")
            .parse::<f64>()
            .ok()?;
        let total_unit = caps.get(4)?.as_str();
        let current_bytes = bytes_with_unit(current, current_unit).round() as u64;
        let total_bytes = bytes_with_unit(total, total_unit).round() as u64;
        if total_bytes > 0 {
            let (raw, message) = download_progress_from_bytes(current_bytes, total_bytes);
            return Some((raw, message, Some(total_bytes)));
        }
    }

    None
}

fn bytes_with_unit(value: f64, unit: &str) -> f64 {
    match unit.to_ascii_uppercase().as_str() {
        "GB" => value * 1_073_741_824.0,
        "MB" => value * 1_048_576.0,
        "KB" => value * 1024.0,
        _ => value,
    }
}

fn is_noisy_process_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with('{') || lower.contains("level=") || lower == "download" || lower == "purchase"
}

fn download_watch_dirs(primary: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![primary.to_path_buf()];
    if let Some(ipatool_dir) = ipatool_data_dir() {
        dirs.push(ipatool_dir);
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

fn is_active_download_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".ipa.tmp")
        || lower.ends_with(".ipa.part")
        || lower.ends_with(".ipa.download")
        || lower.ends_with(".tmp")
        || lower.ends_with(".part")
}

fn download_file_matches_name(name: &str, bundle_id: &str, app_store_id: Option<&str>) -> bool {
    let lower = name.to_ascii_lowercase();
    // Prefer adamId patterns from ipatool: "{id}_{version}.ipa"
    if let Some(id) = app_store_id.filter(|value| !value.is_empty()) {
        let id_l = id.to_ascii_lowercase();
        if lower.starts_with(&format!("{id_l}_"))
            || lower.starts_with(&format!("{id_l}."))
            || lower.contains(&format!("_{id_l}_"))
            || lower.contains(&format!("-{id_l}-"))
            || lower.contains(&id_l)
        {
            return true;
        }
    }
    // Real reverse-DNS bundle only (skip synthetic app.store.{adamId}).
    if !bundle_id.is_empty()
        && !bundle_id.eq_ignore_ascii_case("NONE")
        && !bundle_id.starts_with("app.store.")
        && lower.contains(&bundle_id.to_ascii_lowercase())
    {
        return true;
    }
    false
}

fn is_synthetic_history_bundle(bundle_id: &str) -> bool {
    bundle_id.starts_with("app.store.")
}

fn largest_matching_download_size(
    dirs: &[PathBuf],
    bundle_id: &str,
    app_store_id: Option<&str>,
) -> Option<u64> {
    let mut best = None::<u64>;
    for dir in dirs {
        let entries = match fs::read_dir(dir) {
            Ok(value) => value,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            let name_match = lower == "download.ipa" || lower.starts_with("download.ipa.")
                || download_file_matches_name(name, bundle_id, app_store_id);
            let is_partial = is_active_download_file_name(&lower) && name_match;
            let is_ipa = lower.ends_with(".ipa");
            if !is_partial && !is_ipa {
                continue;
            }
            if is_ipa && !name_match {
                if ipa_bundle_id(&path).as_deref() != Some(bundle_id) {
                    continue;
                }
            }
            // `?` здесь выходил из функции целиком, теряя уже найденный максимум:
        // на Windows metadata() по файлу, в который сейчас пишет ipatool,
        // периодически отдаёт отказ в доступе.
        let Ok(meta) = entry.metadata() else { continue };
        let size = meta.len();
            if size == 0 {
                continue;
            }
            best = Some(best.map_or(size, |value| value.max(size)));
        }
    }
    best
}

fn is_download_cache_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".ipa") || lower.ends_with(".ipa.tmp")
}

async fn watch_ipa_download(
    progress: ProcessProgress,
    watch_dirs: Vec<PathBuf>,
    bundle_id: String,
    app_store_id: Option<String>,
    session_start_bytes: u64,
    stop: Arc<AtomicBool>,
) {
    let mut peak = session_start_bytes;
    let mut restarted = false;
    while !stop.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(350)).await;
        let scanned =
            largest_matching_download_size(&watch_dirs, &bundle_id, app_store_id.as_deref())
                .unwrap_or(0);
        if scanned > 0 {
            if session_start_bytes > scanned.saturating_add(10 * 1024 * 1024) {
                restarted = true;
                peak = scanned;
            } else if scanned > peak {
                peak = scanned;
            }
        }
        let known_total = *progress.known_total_bytes.lock().unwrap();
        if let Some(total_bytes) = known_total.filter(|value| *value > 0) {
            let current_bytes = if restarted {
                peak
            } else {
                peak.max(session_start_bytes)
            };
            if current_bytes > 0 {
                let (raw, message) = download_progress_from_bytes(current_bytes, total_bytes);
                progress.publish_download_progress(raw, &message).await;
                continue;
            }
        }
        // Раньше здесь рисовался секундомер: raw = 2 + прошедшие_секунды * 0.35.
        // Это не прогресс, а время — и пока движок печатал свои настоящие
        // проценты, полоса дёргалась между двумя шкалами: 4, 15, 5, 16.
        // Наблюдатель теперь только считает байты; нечего считать — молчит.
    }
}

/// Строка счётчика загрузки — «PROGRESS:7» и ничего больше.
///
/// Для полосы она нужна, а в журнале запуска — только мусор: за одну загрузку
/// их набегают тысячи, и настоящая причина отказа тонет среди них. В накопитель
/// такие строки не кладём, полосе отдаём как раньше.
fn is_progress_only_line(line: &str) -> bool {
    line.strip_prefix("PROGRESS:")
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

async fn consume_stream<R: AsyncRead + Unpin>(
    mut stream: R,
    output: Arc<StdMutex<Vec<String>>>,
    progress: Option<ProcessProgress>,
) {
    let mut chunk = [0u8; 2048];
    let mut pending: Vec<u8> = vec![];
    loop {
        let Ok(read) = stream.read(&mut chunk).await else {
            break;
        };
        if read == 0 {
            break;
        }
        pending.extend_from_slice(&chunk[..read]);
        while let Some(index) = pending.iter().position(|b| *b == b'\n' || *b == b'\r') {
            let bytes: Vec<u8> = pending.drain(..=index).collect();
            let line = String::from_utf8_lossy(&bytes).trim().to_string();
            if line.is_empty() {
                continue;
            }
            if !is_progress_only_line(&line) {
                output.lock().unwrap().push(line.clone());
            }
            if let Some(sink) = &progress {
                sink.publish(&line).await;
            }
        }
    }
    if !pending.is_empty() {
        let line = String::from_utf8_lossy(&pending).trim().to_string();
        if !line.is_empty() {
            output.lock().unwrap().push(line.clone());
            if let Some(sink) = progress {
                sink.publish(&line).await;
            }
        }
    }
}

fn prepend_tool_dir_to_path(program: &str) -> Option<String> {
    let parent = Path::new(program).parent();
    let mut current = std::env::var("PATH").unwrap_or_default();
    #[cfg(target_os = "macos")]
    {
        // GUI apps often lack Homebrew in PATH.
        for prefix in [
            "/usr/local/bin",
            "/opt/homebrew/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ] {
            if !current.split(':').any(|p| p == prefix) {
                current = if current.is_empty() {
                    prefix.to_string()
                } else {
                    format!("{prefix}:{current}")
                };
            }
        }
    }
    if let Some(parent) = parent.filter(|p| !p.as_os_str().is_empty()) {
        let sep = if cfg!(windows) { ";" } else { ":" };
        return Some(format!("{}{sep}{current}", parent.to_string_lossy()));
    }
    if current.is_empty() {
        None
    } else {
        Some(current)
    }
}

/// No-op when PATH already prepared by [`prepend_tool_dir_to_path`].
#[cfg(target_os = "macos")]
fn ensure_unix_tool_path_env(command: &mut Command) {
    // prepend_tool_dir_to_path already injects Homebrew on macOS; only fill if PATH missing.
    let already = command
        .as_std()
        .get_envs()
        .any(|(k, _)| k == std::ffi::OsStr::new("PATH"));
    if already {
        return;
    }
    if let Some(path) = prepend_tool_dir_to_path("") {
        command.env("PATH", path);
    }
}

#[cfg(not(target_os = "macos"))]
fn ensure_unix_tool_path_env(_command: &mut Command) {}

fn apply_windows_ipatool_helper_env(command: &mut Command, program: &str) {
    #[cfg(target_os = "windows")]
    ensure_windows_sap_sidecars(program);
    ensure_macos_sap_sidecar(program);
    let Some(parent) = Path::new(program).parent() else {
        return;
    };
    let sap = ["sapsigner.exe", "sapsigner", "sapsign-ssh.exe", "sapsign-ssh"]
        .into_iter()
        .map(|name| parent.join(name))
        .find(|p| p.is_file());
    if let Some(sap) = sap {
        command.env("IPATOOL_SAP_HELPER", windows_plain_path(&sap));
        let mut paths = vec![parent.to_path_buf()];
        if let Some(old) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&old));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            command.env("PATH", joined);
        }
    }
    for ani_name in ["anisette.exe", "anisette"] {
        let ani = parent.join(ani_name);
        if ani.is_file() {
            command.env("IPATOOL_ANISETTE", ani);
            break;
        }
    }
}

/// Начало сообщения о превышенном времени ожидания.
///
/// Вынесено, чтобы установка могла отличить «не дождались» от настоящего
/// отказа, не сравнивая русский текст в двух местах.
const PROCESS_TIMEOUT_PREFIX: &str = "Компонент не ответил вовремя";

fn is_process_timeout_error(error: &str) -> bool {
    error.starts_with(PROCESS_TIMEOUT_PREFIX)
}

/// Сколько ждать, пока убитый процесс закроет свои потоки вывода.
const KILLED_CHILD_DRAIN: Duration = Duration::from_secs(2);

/// Дочитать вывод убитого процесса, но не ждать его вечно.
///
/// Каналы stdout/stderr закрываются, только когда их отпустят ВСЕ владельцы.
/// Если движок успел запустить соседний процесс (kbsync на macOS и Linux
/// собирается отдельным файлом), тот наследует канал и держит его открытым
/// ещё минуты после смерти родителя. Ждать тут без предела — значит вернуть
/// ту самую задержку, ради снятия которой процесс убивали.
async fn drain_killed_child_output(
    out_task: &mut tokio::task::JoinHandle<()>,
    err_task: &mut tokio::task::JoinHandle<()>,
) {
    let drained = tokio::time::timeout(KILLED_CHILD_DRAIN, async {
        let _ = (&mut *out_task).await;
        let _ = (&mut *err_task).await;
    })
    .await;
    if drained.is_err() {
        out_task.abort();
        err_task.abort();
    }
}

async fn run_process(
    program: &str,
    args: &[String],
    duration: Duration,
    progress: Option<ProcessProgress>,
) -> Result<String, String> {
    run_process_inner(program, args, duration, progress, false, false).await
}

/// Like [`run_process`], but failed exits return the raw combined stdout/stderr
/// so callers (login) can classify Apple/GSA/anisette errors.
pub(crate) async fn run_process_raw_error(
    program: &str,
    args: &[String],
    duration: Duration,
    progress: Option<ProcessProgress>,
) -> Result<String, String> {
    run_process_inner(program, args, duration, progress, true, false).await
}

/// Имя переменной окружения с ключом к запечатанным артефактам.
pub(crate) const SEALED_ASSET_KEY_ENV: &str = "SIGNUM_ASSET_KEY";

/// Ключ хранится развёрнутым по байтам и складывается только в момент запуска.
///
/// Это не криптографическая защита — всё, что исполняется у пользователя, в
/// итоге вскрывается. Смысл в другом: ключ не виден `strings` по бинарю, а
/// значит утилиту нельзя выдернуть из ресурсов и подставить в свой проект за
/// пять минут. Для этого пришлось бы разбирать сам Signum.
pub(crate) fn sealed_asset_key() -> String {
    const MASK: u8 = 0x5A;
    const SEALED: [u8; 64] = [0x3B, 0x62, 0x6A, 0x62, 0x6C, 0x6E, 0x6A, 0x6D, 0x6D, 0x39, 0x63, 0x39, 0x38, 0x6E, 0x62, 0x6D, 0x63, 0x3E, 0x6F, 0x69, 0x6A, 0x6E, 0x6B, 0x6F, 0x38, 0x62, 0x6C, 0x6B, 0x3C, 0x6B, 0x6D, 0x6B, 0x69, 0x63, 0x3E, 0x68, 0x3F, 0x3B, 0x62, 0x68, 0x6A, 0x3E, 0x69, 0x69, 0x3B, 0x3B, 0x6B, 0x6A, 0x38, 0x69, 0x6A, 0x68, 0x62, 0x6D, 0x6A, 0x6A, 0x38, 0x6C, 0x6C, 0x62, 0x38, 0x3B, 0x69, 0x3B];
    SEALED.iter().map(|b| (b ^ MASK) as char).collect()
}

#[cfg(target_os = "windows")]
fn windows_short_path(path: &Path) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut short = vec![0u16; 32768];
    let size = unsafe { GetShortPathNameW(wide.as_ptr(), short.as_mut_ptr(), short.len() as u32) };
    if size == 0 || size as usize >= short.len() {
        return path.to_path_buf();
    }
    short.truncate(size as usize);
    PathBuf::from(std::ffi::OsString::from_wide(&short))
}

#[cfg(target_os = "windows")]
fn ipatool_home_for_child() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| windows_short_path(Path::new(&home)))
}

#[cfg(not(target_os = "windows"))]
fn ipatool_home_for_child() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

async fn run_process_inner(
    program: &str,
    args: &[String],
    duration: Duration,
    progress: Option<ProcessProgress>,
    raw_error: bool,
    low_priority: bool,
) -> Result<String, String> {
    let program_path = windows_plain_path(Path::new(program));
    // Engine v5 (ipatool-cpp) rejects --non-interactive, so drop it here
    // instead of threading the engine name through every ipatool call site.
    let without_non_interactive: Vec<String>;
    let args: &[String] = if is_cpp_ipatool(program) {
        without_non_interactive = args
            .iter()
            .filter(|arg| arg.as_str() != "--non-interactive")
            .cloned()
            .collect();
        &without_non_interactive
    } else {
        args
    };
    #[cfg(unix)]
    let mut command = if low_priority {
        let mut command = Command::new("/usr/bin/nice");
        command.args(["-n", "19"]).arg(&program_path);
        command
    } else {
        Command::new(&program_path)
    };
    #[cfg(not(unix))]
    let mut command = Command::new(&program_path);
    // Прокси для дочернего процесса. Движок v5 построен на libcurl, а тот
    // читает ALL_PROXY — поэтому одна переменная накрывает и socks5, и http,
    // и отдельной настройки внутри ipatool не требуется.
    if let Some(url) = current_proxy_url() {
        command.env("ALL_PROXY", &url);
        command.env("HTTPS_PROXY", &url);
        command.env("HTTP_PROXY", &url);
    }
    // Ключ к запечатанным артефактам FairPlay. Утилита kbsync без него
    // отказывается работать, поэтому унесённая из ресурсов в чужой проект она
    // бесполезна. Ключ живёт только в окружении дочернего процесса и никогда
    // не попадает ни в аргументы, ни на диск.
    command.env(SEALED_ASSET_KEY_ENV, sealed_asset_key());
    if let Ok(guard) = v6_slot().lock() {
        if let Some(id) = guard.as_ref() {
            command.env("IPATOOL_UA", apple_store::CONFIGURATOR_UA);
            command.env("SAPSIGNER_MAC", &id.mac);
        }
    }
    if let Ok(guard) = process_account_mac().lock() {
        if let Some(mac) = guard.as_ref() {
            command.env("IPATOOL_MAC", mac);
            command.env("SAPSIGNER_MAC", mac);
        }
    }
    // ipatool 2.6.0 и новее, увидев XDG_STATE_HOME или XDG_DATA_HOME, молча
    // ПЕРЕНОСИТ туда всю папку ~/.ipatool целиком. Движок v5 хранит сессию там
    // же — и у человека с такой переменной вход уехал бы неизвестно куда прямо
    // во время обычной операции. Снимаем их у потомка: место хранения должно
    // определять приложение, а не случайная переменная окружения.
    command.env_remove("XDG_STATE_HOME");
    command.env_remove("XDG_DATA_HOME");
    command
        .args(args)
        // Signum supplies credentials and 2FA through command-line arguments.
        // Never let ipatool-cpp wait for its own console prompt: the desktop
        // app has no visible stdin and must receive the 2FA-required response.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // If the Tauri task is cancelled or the application closes, do not
        // leave ipatool/anisette running and locking bundled executables.
        .kill_on_drop(true);
    if let Some(parent) = program_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        // ipatool-cpp starts its companion as `anisette.exe`. Running from the
        // tool directory makes that lookup reliable in development, portable
        // builds and installed Tauri bundles.
        command.current_dir(parent);
        #[cfg(target_os = "macos")]
        {
            // Bundled libimobiledevice dylibs live next to the CLI.
            command.env("DYLD_FALLBACK_LIBRARY_PATH", parent);
        }
        #[cfg(target_os = "linux")]
        {
            let lib = parent.join("lib");
            if lib.is_dir() {
                let mut paths = vec![lib];
                if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
                    paths.extend(std::env::split_paths(&existing));
                }
                if let Ok(joined) = std::env::join_paths(paths) {
                    command.env("LD_LIBRARY_PATH", joined);
                }
            }
        }
    }
    if let Some(path) = prepend_tool_dir_to_path(program) {
        command.env("PATH", path);
    }
    apply_windows_ipatool_helper_env(
        &mut command,
        program_path.to_str().unwrap_or(program),
    );
    sanitize_ipatool_cookies();
    // macOS GUI apps: inject Homebrew/system bins so idevice* resolve.
    ensure_unix_tool_path_env(&mut command);
    // Force a consistent home for ipatool-cpp. On Windows HOME is often unset;
    // if some parent process sets a broken HOME, account files land elsewhere
    // than USERPROFILE\.ipatool and auth info reports "Not logged in".
    if let Some(profile) = ipatool_home_for_child() {
        command.env("USERPROFILE", &profile);
        command.env("HOME", &profile);
        #[cfg(target_os = "windows")]
        {
            let profile_path = PathBuf::from(&profile);
            if let Some(raw) = profile_path.to_str() {
                if raw.len() >= 2 && raw.as_bytes().get(1) == Some(&b':') {
                    command.env("HOMEDRIVE", &raw[..2]);
                    command.env("HOMEPATH", &raw[2..]);
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const IDLE_PRIORITY_CLASS: u32 = 0x00000040;
        command.creation_flags(CREATE_NO_WINDOW | if low_priority { IDLE_PRIORITY_CLASS } else { 0 });
    }
    if should_log_helper_process(program) {
        ipatool_debug_log(&format!(
            "spawn program={program} args=[{}] helper={:?} timeout_s={} {}",
            redact_cmd_args(args),
            sap_helper_path_for(program),
            duration.as_secs(),
            ipatool_session_diag()
        ));
    }
    let mut child = command.spawn().map_err(|e| {
        let detail = format!(
            "Не удалось запустить {}: {e}",
            Path::new(program)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("компонент Signum")
        );
        ipatool_debug_log(&detail);
        detail
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Не удалось получить ответ вспомогательного компонента.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Не удалось получить журнал вспомогательного компонента.".to_string())?;
    let output = Arc::new(StdMutex::new(Vec::<String>::new()));
    let mut out_task = tokio::spawn(consume_stream(stdout, output.clone(), progress.clone()));
    let mut err_task = tokio::spawn(consume_stream(stderr, output.clone(), progress.clone()));
    let started = Instant::now();
    let is_kbsync_run = args.iter().any(|arg| arg == "kbsync");
    let watch_install_complete = is_ideviceinstaller_install_command(program, args);
    let mut install_complete_since: Option<Instant> = None;
    let status = loop {
        if is_kbsync_run && kbsync_cancel_requested() {
            let _ = child.kill().await;
            drain_killed_child_output(&mut out_task, &mut err_task).await;
            ipatool_debug_log(&format!(
                "kbsync остановлен: меняется вход в Apple ID elapsed_ms={}",
                started.elapsed().as_millis()
            ));
            return Err(KBSYNC_CANCELLED.into());
        }
        if let Some(progress) = progress.as_ref() {
            let state = progress.app.state::<AppState>();
            if is_job_cancelled(&state, &progress.job_id) {
                let _ = child.kill().await;
                return Err("Загрузка отменена".into());
            }
        }
        if let Some(status) = child.try_wait().map_err(|_| {
            "Не удалось дождаться завершения вспомогательного компонента.".to_string()
        })? {
            break status;
        }
        // ideviceinstaller уже напечатал «Install: Complete», но не выходит
        // (ждёт уведомление от телефона или отписку от usbmuxd). Установка
        // состоялась — даём ему несколько секунд выйти самому и завершаем.
        if watch_install_complete {
            if install_complete_since.is_none()
                && ideviceinstaller_install_completed(&output.lock().unwrap())
            {
                install_complete_since = Some(Instant::now());
            }
            if install_complete_since
                .is_some_and(|since| since.elapsed() >= IDEVICEINSTALLER_EXIT_GRACE)
            {
                ipatool_debug_log(&format!(
                    "ideviceinstaller reported Complete but did not exit; stopping it elapsed_ms={}",
                    started.elapsed().as_millis()
                ));
                let _ = child.kill().await;
                drain_killed_child_output(&mut out_task, &mut err_task).await;
                return Ok(output.lock().unwrap().join("\n"));
            }
        }
        // Телефон уже подтвердил установку — общий предел её не отменяет,
        // процесс доживает свои секунды и завершается веткой выше.
        if started.elapsed() >= duration && install_complete_since.is_none() {
            let _ = child.kill().await;
            drain_killed_child_output(&mut out_task, &mut err_task).await;
            let text = output.lock().unwrap().join("\n");
            let program_name = Path::new(program)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Компонент");
            let timeout_text = if is_kbsync_run {
                redact_kbsync_output(&text)
            } else {
                text
            };
            let excerpt = technical_excerpt(&timeout_text, 1600);
            ipatool_debug_log(&format!(
                "timeout program={program_name} secs={} excerpt={excerpt}",
                duration.as_secs()
            ));
            return Err(with_technical_detail(
                &format!(
                    "{PROCESS_TIMEOUT_PREFIX} ({} с). Проверьте подключение и повторите попытку.",
                    duration.as_secs()
                ),
                &excerpt,
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let _ = tokio::join!(out_task, err_task);
    let text = output.lock().unwrap().join("\n");
    let diagnostic_text = if is_kbsync_run {
        redact_kbsync_output(&text)
    } else {
        text.clone()
    };
    if should_log_helper_process(program) {
        ipatool_debug_log(&format!(
            "proc program={} exit={:?} elapsed_ms={} out_len={} excerpt={}",
            Path::new(program)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or(program),
            status.code(),
            started.elapsed().as_millis(),
            text.len(),
            technical_excerpt(&diagnostic_text, 800)
        ));
        diag_log::proc_out(program, status.code(), &diagnostic_text);
    }
    if status.success() {
        return Ok(text);
    }
    // Телефон ответил «Complete» и ни одной ошибки не было, а процесс вышел с
    // ненулевым кодом (сбой уже на разъединении). Приложение установлено;
    // считать это отказом — значит заливать IPA заново через upgrade.
    if watch_install_complete && ideviceinstaller_install_completed(&output.lock().unwrap()) {
        ipatool_debug_log(&format!(
            "ideviceinstaller exit={:?} after Complete — treating as installed",
            status.code()
        ));
        return Ok(text);
    }
    // Keep the untouched output for the automatic report. The friendly Russian
    // sentence built below throws the real cause away, which is why so many
    // reports used to arrive as a bare «Не удалось выполнить операцию».
    diag_log::remember_raw(&format!(
        "{} exit={} :: {}",
        Path::new(program)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(program),
        status.code().unwrap_or(-1),
        technical_excerpt(&diagnostic_text, 1600)
    ));
    if raw_error {
        let clean = strip_ansi(&text);
        if clean.trim().is_empty() {
            Err(format_process_error(
                program,
                status.code().unwrap_or(-1),
                &text,
            ))
        } else {
            Err(clean)
        }
    } else {
        Err(format_process_error(
            program,
            status.code().unwrap_or(-1),
            &text,
        ))
    }
}

fn redact_kbsync_output(text: &str) -> String {
    let Some(value) = parse_json_object_from_text(text) else {
        return format!("kbsync output omitted ({} bytes)", text.len());
    };
    let mut summary = serde_json::Map::new();
    for key in ["success", "length", "source"] {
        if let Some(value) = value.get(key) {
            summary.insert(key.to_string(), value.clone());
        }
    }
    serde_json::Value::Object(summary).to_string()
}

fn ipa_bundle_id(path: &Path) -> Option<String> {
    cached_ipa_metadata(path).0
}

fn cached_ipa_metadata(
    path: &Path,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    (|| {
        let file = fs::File::open(path).ok()?;
        let mut archive = zip::ZipArchive::new(file).ok()?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).ok()?;
            let name = file.name().to_string();
            if name.starts_with("Payload/") && name.ends_with(".app/Info.plist") && name.matches('/').count() == 2 {
                let mut data = vec![];
                file.read_to_end(&mut data).ok()?;
                let value = plist::Value::from_reader(Cursor::new(data)).ok()?;
                let dict = value.as_dictionary()?;
                return Some((
                    plist_string(dict, &["CFBundleIdentifier"]),
                    plist_string(dict, &["CFBundleDisplayName", "CFBundleName"]),
                    plist_string(dict, &["CFBundleShortVersionString", "CFBundleVersion"]),
                    plist_string(dict, &["MinimumOSVersion"]),
                ));
            }
        }
        None
    })()
    .unwrap_or_default()
}

/// Apple ID that an IPA is licensed to, read from its `iTunesMetadata.plist`.
/// App Store downloads embed the downloading account here; sideload/history IPAs
/// may lack it (returns None → callers must not assume a mismatch).
fn ipa_apple_id(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut data = vec![];
    {
        let mut entry = archive.by_name("iTunesMetadata.plist").ok()?;
        entry.read_to_end(&mut data).ok()?;
    }
    let value = plist::Value::from_reader(Cursor::new(data)).ok()?;
    let dict = value.as_dictionary()?;
    if let Some(id) = plist_string(dict, &["apple-id", "appleId"]).filter(|v| !v.is_empty()) {
        return Some(normalize_account_email(&id));
    }
    let info = dict
        .get("com.apple.iTunesStore.downloadInfo")
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("accountInfo"))
        .and_then(|v| v.as_dictionary())?;
    plist_string(info, &["AppleID", "appleId"])
        .filter(|v| !v.is_empty())
        .map(|v| normalize_account_email(&v))
}

fn ipa_external_version_id(path: &Path) -> Option<String> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
    let mut entry = archive.by_name("iTunesMetadata.plist").ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    let value = plist::Value::from_reader(Cursor::new(bytes)).ok()?;
    let dict = value.as_dictionary()?;
    ["softwareVersionExternalIdentifier", "software-version-external-identifier", "externalVersionId"]
        .iter().find_map(|key| dict.get(*key).and_then(|value| match value {
            plist::Value::String(value) => Some(value.clone()),
            plist::Value::Integer(value) => value.as_unsigned().map(|value| value.to_string()),
            _ => None,
        }))
}

fn validate_requested_ipa_version(path: &Path, input: &CatalogInstallInput) -> Result<(), String> {
    let Some(wanted) = input.external_version_id.as_deref().map(str::trim)
        .filter(|value| !value.is_empty() && *value != "latest") else { return Ok(()); };
    if let Some(actual) = ipa_external_version_id(path) {
        if actual == wanted { return Ok(()); }
        return Err(format!("Apple вернула другую сборку: {actual}, выбрана {wanted}. Файл не будет установлен или экспортирован."));
    }
    Err("Не удалось подтвердить выбранную версию внутри IPA. Файл не будет установлен или экспортирован.".into())
}

fn resolve_downloaded_ipa(
    dir: &Path,
    bundle_id: &str,
    app_store_id: Option<&str>,
) -> Result<PathBuf, String> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| p.extension().is_some_and(|v| v.eq_ignore_ascii_case("ipa")))
        .collect();
    files.sort_by_key(|p| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH)
    });
    files.reverse();

    // Prefer the embedded store ID: filenames vary between download engines,
    // and history entries may only have a synthetic app.store.{id} bundle.
    if let Some(id) = app_store_id.filter(|value| !value.is_empty()) {
        if let Some(path) = files.iter().find(|path| ipa_app_store_id(path).as_deref() == Some(id)) {
            return Ok(path.clone());
        }
    }

    // 1) Filename match by adamId — ipatool writes "{adamId}_{version}.ipa".
    //    Do NOT require CFBundleIdentifier == synthetic "app.store.{id}" (history path).
    if let Some(id) = app_store_id.filter(|value| !value.is_empty()) {
        if let Some(path) = files.iter().find(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| download_file_matches_name(name, bundle_id, Some(id)))
                && ipa_app_store_id(path).is_none()
        }) {
            return Ok(path.clone());
        }
    }

    // 2) Match real CFBundleIdentifier inside the IPA (catalog installs).
    if !bundle_id.is_empty()
        && !bundle_id.eq_ignore_ascii_case("NONE")
        && !is_synthetic_history_bundle(bundle_id)
    {
        if let Some(path) = files
            .iter()
            .find(|p| ipa_bundle_id(p).as_deref() == Some(bundle_id))
        {
            return Ok(path.clone());
        }
    }

    let label = app_store_id.filter(|s| !s.is_empty()).unwrap_or(bundle_id);
    Err(format!(
        "Не удалось найти скачанный файл приложения ({label})."
    ))
}

fn ipa_app_store_id(path: &Path) -> Option<String> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
    let mut entry = archive.by_name("iTunesMetadata.plist").ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    let value = plist::Value::from_reader(Cursor::new(bytes)).ok()?;
    match value.as_dictionary()?.get("itemId")? {
        plist::Value::Integer(value) => value.as_unsigned().map(|id| id.to_string()),
        plist::Value::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn resolve_completed_download(
    output_path: &Path,
    bundle_id: &str,
    app_store_id: Option<&str>,
) -> Result<PathBuf, String> {
    // This is the explicit output in this job's private staging directory.
    // It does not need to encode the store ID or the real bundle in its name.
    if output_path.is_file() {
        return Ok(output_path.to_path_buf());
    }
    resolve_downloaded_ipa(
        output_path.parent().ok_or("Не указана папка загрузки IPA.")?,
        bundle_id,
        app_store_id,
    )
}

fn is_zip_complete(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let Ok(meta) = file.metadata() else {
        return false;
    };
    if meta.len() < 22 {
        return false;
    }
    if file.seek(SeekFrom::End(-22)).is_err() {
        return false;
    }
    let mut buf = [0u8; 4];
    if file.read_exact(&mut buf).is_err() {
        return false;
    }
    buf == [0x50, 0x4b, 0x05, 0x06]
}

fn try_cached_complete_ipa(
    dir: &Path,
    bundle_id: &str,
    app_store_id: Option<&str>,
    account_email: Option<&str>,
    external_version_id: Option<&str>,
) -> Option<PathBuf> {
    let wanted = external_version_id.map(str::trim).filter(|value| !value.is_empty() && *value != "latest")?;
    let path = resolve_downloaded_ipa(dir, bundle_id, app_store_id).ok()?;
    if ipa_external_version_id(&path).as_deref() != Some(wanted) { return None; }
    // Synthetic history bundle_id never matches Info.plist — filename + zip EOCD is enough.
    if !is_zip_complete(&path) {
        return None;
    }
    // Reuse only a licensed IPA belonging to the active account. Raw files left
    // by a failed minizip append must not be treated as completed downloads.
    if let Some(want) = account_email
        .map(normalize_account_email)
        .filter(|v| !v.is_empty())
    {
        if ipa_apple_id(&path).as_deref() != Some(want.as_str()) { return None; }
    }
    if !is_synthetic_history_bundle(bundle_id) {
        if let Some(actual) = ipa_bundle_id(&path) {
            if actual != bundle_id
                && app_store_id
                    .map(|id| {
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| !download_file_matches_name(n, "", Some(id)))
                            .unwrap_or(true)
                    })
                    .unwrap_or(true)
            {
                return None;
            }
        }
    }
    Some(path)
}

async fn install_ipa(
    app: &AppHandle,
    job_id: &str,
    path: &Path,
    progress_from: f64,
    progress_to: f64,
) -> Result<(), String> {
    if demo_active() {
        let state = app.state::<AppState>();
        update_job(
            app,
            job_id,
            "install",
            progress_from + (progress_to - progress_from) * 0.5,
            "Устанавливаем на iPhone…",
            JobStatus::Running,
            None,
        )
        .await;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        ensure_not_cancelled(&state, job_id)?;
        return Ok(());
    }
    if !path.exists() {
        return Err(format!("IPA не найден: {}", path.display()));
    }
    let tools = resolve_tool_paths(app)?;
    require_idevice_tools(&tools)?;
    let progress = ProcessProgress::new(
        app.clone(),
        job_id.into(),
        "install".into(),
        progress_from,
        progress_to,
    );
    // Что стоит на телефоне до установки. Понадобится, только если
    // ideviceinstaller замолчит до таймаута: тогда по этому снимку видно,
    // появилось приложение сейчас или стояло и раньше.
    let installed_before = installed_snapshot_before_install(app).await;
    let base = vec!["install".into(), path.to_string_lossy().into_owned()];
    let args = idevice_args_with_udid(app, base.clone());
    let install_error = match run_process(
        &tools.ideviceinstaller,
        &args,
        Duration::from_secs(360),
        Some(progress),
    )
    .await
    {
        Ok(_) => return Ok(()),
        // Free-sign IPAs have no App Store .sinf — warning only, install still OK.
        Err(error) if is_sinf_warning_only_install_output(&error) => return Ok(()),
        Err(error) => error,
    };
    let cancelled = is_job_cancelled(&app.state::<AppState>(), job_id);
    if cancelled {
        return Err(install_error);
    }
    if is_process_timeout_error(&install_error) {
        // ideviceinstaller молчал шесть минут. Бывает, что телефон приложение
        // поставил, а процесс так и не узнал об этом. Гадать по его выводу
        // нельзя — спрашиваем сам телефон.
        if ipa_landed_on_device_after_timeout(
            app,
            job_id,
            path,
            progress_to,
            installed_before.as_ref(),
        )
        .await
        {
            return Ok(());
        }
        return Err(install_error);
    }
    if is_device_missing_error(&install_error) {
        if !args.iter().any(|a| a == "-n") && selected_device_connection(app) != "wifi" {
            // USB gone mid-job — retry over Wi‑Fi if the phone is on the network.
            let progress_retry = ProcessProgress::new(
                app.clone(),
                job_id.into(),
                "install".into(),
                progress_from,
                progress_to,
            );
            let net_args = idevice_args_network(app, base);
            return match run_process(
                &tools.ideviceinstaller,
                &net_args,
                Duration::from_secs(360),
                Some(progress_retry),
            )
            .await
            {
                Ok(_) => Ok(()),
                Err(net_err) if is_sinf_warning_only_install_output(&net_err) => Ok(()),
                Err(net_err) if is_device_missing_error(&net_err) => {
                    Err(device_not_found_message())
                }
                Err(net_err) => Err(net_err),
            };
        }
        return Err(device_not_found_message());
    }
    if !install_failure_allows_upgrade_retry(&install_error, cancelled) {
        return Err(install_error);
    }
    // Match IPA_Downloader: an existing bundle is upgraded after a normal
    // install refusal. This also handles the common «already installed»
    // path without making the user remove the app manually.
    ipatool_debug_log(&format!(
        "install refused, trying upgrade: {}",
        technical_excerpt(&install_error, 400)
    ));
    let upgrade = idevice_args_with_udid(
        app,
        vec!["upgrade".into(), path.to_string_lossy().into_owned()],
    );
    match run_process(
        &tools.ideviceinstaller,
        &upgrade,
        Duration::from_secs(360),
        Some(ProcessProgress::new(
            app.clone(),
            job_id.into(),
            "install".into(),
            progress_from,
            progress_to,
        )),
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(upgrade_error) if is_sinf_warning_only_install_output(&upgrade_error) => Ok(()),
        Err(upgrade_error) => {
            if is_job_cancelled(&app.state::<AppState>(), job_id) {
                return Err(upgrade_error);
            }
            if is_process_timeout_error(&upgrade_error)
                && ipa_landed_on_device_after_timeout(
                    app,
                    job_id,
                    path,
                    progress_to,
                    installed_before.as_ref(),
                )
                .await
            {
                return Ok(());
            }
            ipatool_debug_log(&format!(
                "upgrade after refused install failed too: {}",
                technical_excerpt(&upgrade_error, 400)
            ));
            Err(pick_install_error(install_error, upgrade_error))
        }
    }
}

/// Список приложений на телефоне перед установкой.
///
/// Ждём недолго: это страховка на редкий случай, и задерживать из-за неё
/// каждую установку нельзя. Не успели или телефон не ответил — снимка нет, и
/// после таймаута установка останется ошибкой, как и раньше.
async fn installed_snapshot_before_install(
    app: &AppHandle,
) -> Option<HashMap<String, InstalledApp>> {
    match tokio::time::timeout(Duration::from_secs(8), list_installed_apps_inner(app)).await {
        Ok(Ok(apps)) => Some(
            apps.into_iter()
                .filter(|item| !item.is_placeholder)
                .map(|item| (item.bundle_id.clone(), item))
                .collect(),
        ),
        _ => None,
    }
}

/// После таймаута ideviceinstaller: поставилось ли приложение на самом деле.
///
/// Смотрим список приложений на телефоне и сверяем идентификатор и версию с
/// тем, что лежит в IPA, и со снимком до установки. Телефон мог ещё
/// доделывать установку в момент, когда процесс убили, поэтому спрашиваем
/// дважды с паузой.
async fn ipa_landed_on_device_after_timeout(
    app: &AppHandle,
    job_id: &str,
    path: &Path,
    progress: f64,
    installed_before: Option<&HashMap<String, InstalledApp>>,
) -> bool {
    // Без снимка «до» ответ телефона ничего не доказывает — не тратим время.
    if installed_before.is_none() {
        return false;
    }
    let (bundle_id, _, version, _) = cached_ipa_metadata(path);
    let Some(bundle_id) = bundle_id.filter(|value| !value.trim().is_empty()) else {
        return false;
    };
    update_job(
        app,
        job_id,
        "install",
        progress,
        "Проверяем, установилось ли приложение на iPhone…",
        JobStatus::Running,
        None,
    )
    .await;
    for attempt in 0..2u8 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        if is_job_cancelled(&app.state::<AppState>(), job_id) {
            return false;
        }
        // Сам опрос тоже ограничен: зависший телефон иначе добавил бы к
        // шести минутам ожидания ещё несколько.
        let Ok(installed) =
            tokio::time::timeout(Duration::from_secs(20), installed_apps_map(app)).await
        else {
            continue;
        };
        if timed_out_install_landed(installed_before, &installed, &bundle_id, version.as_deref()) {
            ipatool_debug_log(&format!(
                "install timed out, but {bundle_id} {} appeared on the device — treating as installed",
                version.as_deref().unwrap_or("?")
            ));
            return true;
        }
    }
    ipatool_debug_log(&format!(
        "install timed out; {bundle_id} {} did not appear on the device",
        version.as_deref().unwrap_or("?")
    ));
    false
}

/// Добавить приложение в покупки Apple ID, не скачивая его.
///
/// Иногда человеку нужно именно это: закрепить бесплатное приложение за
/// аккаунтом, чтобы потом скачать его с другого устройства или когда оно
/// пропадёт из магазина. Качать при этом сотню мегабайт незачем.
#[tauri::command]
async fn purchase_app_only(
    app: AppHandle,
    state: State<'_, AppState>,
    app_store_id: Option<String>,
    bundle_id: Option<String>,
    title: Option<String>,
) -> Result<String, String> {
    let store_id = app_store_id.as_deref().map(str::trim).filter(|v| !v.is_empty());
    let bundle = bundle_id.as_deref().map(str::trim).filter(|v| !v.is_empty());
    if store_id.is_none() && bundle.is_none() {
        return Err("Не указано, какое приложение покупать.".into());
    }
    let name = title
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("Приложение")
        .to_string();

    let input = CatalogInstallInput {
        title: name.clone(),
        bundle_id: bundle.unwrap_or_default().to_string(),
        app_store_id: store_id.map(str::to_string),
        in_store: true,
        min_ios: None,
        version: None,
        external_version_id: None,
        already_owned: false,
        temporary_us_region: false,
        temporary_region: None,
    };

    // Тот же замок, что у загрузок: два ipatool разом ломают файл сессии.
    let _guard = state.ipatool_lock.lock().await;
    let tools = resolve_tool_paths(&app)?;
    let keychain = resolve_ipatool_keychain(&app)?;
    diag_log::note("ui", &format!("покупка без скачивания: {name}"));
    match purchase_store_app(&app, &tools, &keychain, &input).await {
        Ok(_) => Ok(format!("{name} добавлено в покупки Apple ID.")),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseManyItem {
    app_store_id: String,
    #[serde(default)]
    bundle_id: Option<String>,
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseManyInput {
    items: Vec<PurchaseManyItem>,
    account_emails: Vec<String>,
    #[serde(default)]
    confirmed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseManyAccountResult {
    email: String,
    ok: usize,
    failed: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseManyResult {
    results: Vec<PurchaseManyAccountResult>,
    restored_account: Option<String>,
    restore_error: Option<String>,
}

async fn switch_saved_account_locked(
    app: &AppHandle,
    state: &AppState,
    current: &str,
    target: &str,
) -> Result<(), String> {
    if current.eq_ignore_ascii_case(target) {
        return Ok(());
    }
    backup_ipatool_session_for_account(app, current)?;
    reset_ipatool_local_store();
    forget_login_engine(app);
    if let Err(error) = restore_ipatool_session_for_account(app, target) {
        let _ = rollback_to_previous_active_session(app, Some(current));
        return Err(error);
    }
    match verify_ipatool_session(app).await {
        Ok(actual) if actual == target => {
            if let Err(error) = set_active_account(app, target) {
                let _ = rollback_to_previous_active_session(app, Some(current));
                return Err(error);
            }
            clear_account_version_cache(state);
            Ok(())
        }
        Ok(actual) => {
            let _ = rollback_to_previous_active_session(app, Some(current));
            Err(format!("Сессия принадлежит другому Apple ID ({actual})."))
        }
        Err(error) => {
            let _ = rollback_to_previous_active_session(app, Some(current));
            Err(with_technical_detail(
                "Не удалось проверить сохранённую сессию Apple ID.",
                &error,
            ))
        }
    }
}

/// Purchase selected App Store apps on several already authenticated Apple IDs.
/// Sessions are switched from their encrypted per-account backups and the
/// original active account is restored before the command returns.
#[tauri::command]
async fn purchase_apps_on_accounts(
    app: AppHandle,
    state: State<'_, AppState>,
    input: PurchaseManyInput,
) -> Result<PurchaseManyResult, String> {
    if !input.confirmed {
        return Err("Подтвердите покупку на выбранных Apple ID.".into());
    }
    if input.items.is_empty() || input.items.len() > 50 {
        return Err("Выберите от 1 до 50 приложений.".into());
    }
    if input.account_emails.is_empty() || input.account_emails.len() > 20 {
        return Err("Выберите от 1 до 20 Apple ID.".into());
    }
    let mut items = Vec::new();
    let mut seen_items = HashSet::new();
    for mut item in input.items {
        let id = item.app_store_id.trim().to_string();
        if normalized_app_store_id(Some(&id)).is_none() {
            return Err("У одного из приложений некорректный App Store ID.".into());
        }
        item.app_store_id = id;
        item.title = item.title.trim().to_string();
        if item.title.is_empty() { item.title = "Приложение".into(); }
        if seen_items.insert(item.app_store_id.clone()) { items.push(item); }
    }
    let mut accounts = Vec::new();
    let mut seen_accounts = HashSet::new();
    for email in input.account_emails {
        let normalized = normalize_account_email(&email);
        if normalized.is_empty() { return Err("Укажите корректный Apple ID.".into()); }
        if seen_accounts.insert(normalized.clone()) { accounts.push(normalized); }
    }

    if DEMO_MODE {
        return Ok(PurchaseManyResult {
            results: accounts.into_iter().map(|email| PurchaseManyAccountResult {
                email, ok: items.len(), failed: Vec::new(), warnings: Vec::new(),
            }).collect(),
            restored_account: Some(demo_email()),
            restore_error: None,
        });
    }
    let _guard = state.ipatool_lock.lock().await;
    require_auth(&app)?;
    ensure_active_ipatool_session(&app).await?;
    let original = active_account_email(&app)
        .ok_or_else(|| "Сначала войдите в Apple ID.".to_string())?;
    let cache = load_cache(&app);
    for email in &accounts {
        if !cache.apple_accounts.iter().any(|account| account.email == *email) {
            return Err(format!("Apple ID {email} не найден в сохранённых аккаунтах."));
        }
        if email != &original && !saved_account_session_exists(&app, email) {
            return Err(format!("Для {email} нет сохранённой сессии. Войдите в этот Apple ID снова."));
        }
    }

    backup_ipatool_session_for_account(&app, &original)?;
    let mut current = original.clone();
    let mut switch_failed = false;
    let mut results = Vec::new();
    for email in &accounts {
        let mut row = PurchaseManyAccountResult {
            email: email.clone(), ok: 0, failed: Vec::new(), warnings: Vec::new(),
        };
        if let Err(error) = switch_saved_account_locked(&app, state.inner(), &current, email).await {
            row.failed.push(error);
            results.push(row);
            switch_failed = true;
            break;
        }
        current = email.clone();
        // Saved sessions may have been created by different ipatool engines.
        let (tools, keychain) = match (resolve_tool_paths(&app), resolve_ipatool_keychain(&app)) {
            (Ok(tools), Ok(keychain)) => (tools, keychain),
            (Err(error), _) | (_, Err(error)) => {
                row.failed.push(error);
                results.push(row);
                continue;
            }
        };
        for item in &items {
            let mut input = CatalogInstallInput {
                title: item.title.clone(),
                bundle_id: item.bundle_id.clone().unwrap_or_default(),
                app_store_id: Some(item.app_store_id.clone()),
                in_store: true,
                min_ios: None,
                version: None,
                external_version_id: None,
                already_owned: false,
                temporary_us_region: false,
                temporary_region: None,
            };
            if input.bundle_id.is_empty() && !is_cpp_ipatool(&tools.ipatool) {
                input.bundle_id = itunes_lookup_app(&item.app_store_id).await
                    .and_then(|value| value.get("bundleId").and_then(JsonValue::as_str).map(str::to_string))
                    .unwrap_or_default();
                if input.bundle_id.is_empty() {
                    row.failed.push(format!("{}: Не удалось определить Bundle ID для покупки.", item.title));
                    continue;
                }
            }
            match purchase_store_app(&app, &tools, &keychain, &input).await {
                Ok(Some(_)) => row.failed.push(format!("{}: Покупка не подтверждена магазином.", item.title)),
                Ok(None) => row.ok += 1,
                Err(error) => row.failed.push(format!("{}: {error}", item.title)),
            }
        }
        results.push(row);
    }

    let mut restored_account = None;
    let mut restore_error = None;
    if current != original || switch_failed {
        // Restore directly: a failed switch may have left no usable current
        // session, so backing it up could overwrite the last good snapshot.
        let restored = async {
            forget_login_engine(&app);
            rollback_to_previous_active_session(&app, Some(&original))?;
            let actual = verify_ipatool_session(&app).await?;
            if !actual.eq_ignore_ascii_case(&original) {
                return Err(format!("Сессия принадлежит другому Apple ID ({actual})."));
            }
            set_active_account(&app, &original)?;
            clear_account_version_cache(state.inner());
            Ok::<(), String>(())
        }.await;
        match restored {
            Ok(()) => { restored_account = Some(original.clone()); }
            Err(error) => restore_error = Some(error),
        }
    } else {
        restored_account = Some(original);
    }
    Ok(PurchaseManyResult { results, restored_account, restore_error })
}

async fn purchase_store_app(
    app: &AppHandle,
    tools: &ToolPaths,
    keychain: &IpatoolKeychain,
    input: &CatalogInstallInput,
) -> Result<Option<String>, String> {
    // Removed covers / known purchases: no standalone `purchase` step.
    // `download … --purchase` will re-bind or pull the licensed IPA.
    if !input.in_store || input.already_owned {
        return Ok(None);
    }

    let mut identifiers = Vec::<(&str, String)>::new();
    // `purchase -i <id>` exists only in ipatool-cpp, and there it is the better
    // first try: cover apps often carry a synthetic bundle id that -b cannot
    // resolve. Every Go build answers "unknown shorthand flag: 'i' in -i"
    // instead, so offering it there only burns a process launch per purchase.
    let store_id = normalized_app_store_id(input.app_store_id.as_deref());
    if is_cpp_ipatool(&tools.ipatool) {
        if let Some(id) = store_id {
            identifiers.push(("-i", id.to_string()));
        }
    }
    if !input.bundle_id.trim().is_empty()
        && input.bundle_id.trim() != "NONE"
        && !input.bundle_id.starts_with("app.store.")
    {
        identifiers.push(("-b", input.bundle_id.clone()));
    }

    let mut errors = Vec::new();
    for (flag, value) in identifiers {
        let attempt = format!("{flag} {value}");
        let mut args = ipatool_args(keychain);
        args.extend(["purchase".into(), flag.into(), value]);
        match run_process(&tools.ipatool, &args, Duration::from_secs(120), None).await {
            Ok(_) => {
                // Apple may need a moment to expose a newly added purchase.
                tokio::time::sleep(Duration::from_millis(700)).await;
                return Ok(None);
            }
            Err(error) => {
                // Exit code 1, but Apple just said the licence is already on the
                // account — that is the goal of this step, not a failure. Without
                // this the attempt was logged as an error and the user was told
                // to sign in with a different Apple ID.
                if is_already_owned_purchase_result(&error) {
                    return Ok(None);
                }
                // Only real storefront/auth problems must abort the whole job.
                // «app not found» is normal for delisted/region-hidden titles that
                // are still on the account — continue to download --purchase.
                if is_hard_storefront_error(&error) {
                    let friendly =
                        friendly_apple_store_error(&error).unwrap_or_else(|| error.clone());
                    return Err(with_process_log(&friendly, &error));
                }
                if error.contains("Apple ID ещё не настроен для покупок в App Store")
                {
                    return Err(error);
                }
                errors.push(format!("Попытка {attempt}:\n{error}"));
            }
        }
    }

    // Some ipatool builds return code 1 when the app is already attached to
    // the account. Treat it as success only after an explicit ownership check.
    if probe_version_on_account_for_catalog_version(
        app,
        &input.bundle_id,
        input.app_store_id.as_deref(),
        &JsonValue::Null,
    )
    .await
    .unwrap_or(false)
    {
        return Ok(None);
    }

    // Soft-fail: standalone purchase often returns «app not found» for delisted
    // apps; `download -i … --purchase` still works when the license is on the ID.
    let joined = errors.join("\n\n");
    if is_app_not_found_store_error(&joined) || !joined.is_empty() {
        return Ok(Some(if DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed) {
            format!("Не удалось привязать в магазине — пробуем скачать из покупок.\n\n{joined}")
        } else {
            "Не удалось привязать в магазине — пробуем скачать из покупок.".into()
        }));
    }

    // Do not stop here: `download --purchase` is the tool's built-in atomic
    // fallback and may succeed even when the standalone purchase exits with 1.
    Ok(Some(errors.join("\n\n")))
}

fn catalog_download_temp_dir(job_id: &str) -> Result<PathBuf, String> {
    if job_id.is_empty() || !job_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Некорректный номер задачи загрузки.".into());
    }
    Ok(std::env::temp_dir().join(format!("signum-{job_id}")))
}

fn cleanup_catalog_download(job_id: &str, ipa: &Path) {
    let Ok(dir) = catalog_download_temp_dir(job_id) else { return; };
    if ipa.parent() == Some(dir.as_path()) {
        if ipa.exists() {
            let _ = fs::remove_file(ipa);
        }
        // Only remove an empty job directory, never recursively delete temp.
        let _ = fs::remove_dir(dir);
    }
}

async fn download_catalog_ipa_inner(
    app: &AppHandle,
    job_id: &str,
    input: &CatalogInstallInput,
    progress_from: f64,
    progress_to: f64,
) -> Result<PathBuf, String> {
    let state = app.state::<AppState>();
    ensure_not_cancelled(&state, job_id)?;
    let tools = resolve_tool_paths(app)?;
    let keychain = resolve_ipatool_keychain(app)?;
    let staging_dir = catalog_download_temp_dir(job_id)?;
    fs::create_dir_all(&staging_dir).map_err(|error| format!("Не удалось создать папку загрузки: {error}"))?;
    let output_path = staging_dir.join("download.ipa");

    // Replace app.store.* with real bundle via iTunes when possible (UI + cache).
    let mut input_owned = input.clone();
    resolve_catalog_bundle_id(&mut input_owned).await;
    if !is_placeholder_bundle_id(&input_owned.bundle_id) && input_owned.bundle_id != input.bundle_id
    {
        if let Some(adam) = input_owned.app_store_id.as_deref() {
            if let Some(email) = active_account_email(app) {
                history_secrets::update_history_app_bundle(&email, adam, &input_owned.bundle_id);
            }
        }
        // Refresh job subtitle so Installs page doesn't show app.store.*
        if let Some(job) = state.jobs.lock().await.get_mut(job_id) {
            job.subtitle = catalog_job_subtitle(&input_owned);
        }
        let _ = app.emit(
            "install-progress",
            state.jobs.lock().await.get(job_id).cloned(),
        );
    }
    let input = &input_owned;

    // Paths:
    // - already_owned (история покупок): skip probe + standalone purchase → download --purchase.
    // - !in_store (REMOVED / cover из каталога): NEVER purchase. Probe ownership, then download.
    // - in_store: purchase first, soft-fail to download on «app not found».
    if let Some(email) = active_account_email(app) {
        let _ = apple_region::accept_pending_media_terms(&email).await;
    }
    if input.already_owned {
        update_job(
            app,
            job_id,
            "purchase",
            progress_from,
            "Загрузка из покупок Apple ID",
            JobStatus::Running,
            None,
        )
        .await;
    } else if !input.in_store {
        update_job(
            app,
            job_id,
            "purchase",
            progress_from,
            "Проверка покупок Apple ID",
            JobStatus::Running,
            None,
        )
        .await;
        let on_account = probe_version_on_account_for_catalog_version(
            app,
            &input.bundle_id,
            input.app_store_id.as_deref(),
            &JsonValue::Null,
        )
        .await
        .unwrap_or(false);
        if !on_account {
            return Err(format!(
                "«{}» уже нет в App Store и нет в покупках этого Apple ID.\n\n\
                 Signum ставит только уже купленные приложения.\n\n\
                 Выберите другую версию или войдите в Apple ID, на котором приложение покупалось.",
                input.title
            ));
        }
    } else {
        // Current store apps: try purchase first (IPA_Downloader: download -i … --purchase).
        update_job(
            app,
            job_id,
            "purchase",
            progress_from,
            "Привязка приложения к Apple ID",
            JobStatus::Running,
            None,
        )
        .await;
    }
    // Purchase ONLY for live store apps. REMOVED / history never hit purchase -i/-b.
    let purchase_log = if input.in_store && !input.already_owned {
        purchase_store_app(app, &tools, &keychain, input).await?
    } else {
        None
    };
    ensure_not_cancelled(&state, job_id)?;
    let anisette = resolve_anisette_exe(app);
    if !ipatool_signs_store_requests(&tools.ipatool) {
    if let (Some(email), Some(adam)) = (
        active_account_email(app),
        normalized_app_store_id(input.app_store_id.as_deref()).map(str::to_string),
    ) {
        if apple_store::helper_available(anisette.as_deref()) {
            update_job(
                app,
                job_id,
                "download",
                progress_from + (progress_to - progress_from) * 0.08,
                &format_download_progress_label(8.0),
                JobStatus::Running,
                None,
            )
            .await;
            let dest = output_path.clone();
            match apple_store::download_ipa(
                anisette.as_deref(),
                &email,
                &adam,
                input.external_version_id.as_deref(),
                input.already_owned || !input.in_store,
                &dest,
            )
            .await
            {
                Ok(path) => {
                    if validate_requested_ipa_version(&path, input).is_err() {
                        let _ = fs::remove_file(&path);
                        // Native raw IPAs have no iTunesMetadata; use ipatool to
                        // obtain a licensed archive with a verifiable build ID.
                    } else {
                        remember_real_bundle_from_ipa(app, input, &path);
                        update_job(
                            app,
                            job_id,
                            "download",
                            progress_to,
                            &format_download_progress_label(100.0),
                            JobStatus::Running,
                            None,
                        )
                        .await;
                        return Ok(path);
                    }
                }
                Err(_) => {}
            }
        }
    }
    }
    // Explicit ASCII filename avoids minizip failures on the app's display name
    // and a private directory cannot resolve a previous download as this job.
    #[cfg(windows)]
    let cli_output_path = windows_short_path(&staging_dir).join("download.ipa");
    #[cfg(not(windows))]
    let cli_output_path = output_path.clone();
    let watch_dirs = vec![staging_dir.clone()];
    let session_start_bytes = largest_matching_download_size(
        &watch_dirs,
        &input.bundle_id,
        input.app_store_id.as_deref(),
    )
    .unwrap_or(0);
    let download_start = progress_from + (progress_to - progress_from) * 0.02;
    update_job(
        app,
        job_id,
        "download",
        download_start,
        &format_download_progress_label(2.0),
        JobStatus::Running,
        None,
    )
    .await;
    // IPA_Downloader: `ipatool download -i <id> --purchase` (no --non-interactive).
    let mut args = ipatool_args_download(&keychain);
    args.extend([
        "download".into(),
        "-o".into(),
        cli_output_path.to_string_lossy().into_owned(),
    ]);
    if let Some(id) = normalized_app_store_id(input.app_store_id.as_deref()) {
        args.extend(["-i".into(), id.to_string()]);
    } else {
        args.extend(["-b".into(), input.bundle_id.clone()]);
    }
    if let Some(external_version_id) = input
        .external_version_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        args.extend([
            "--external-version-id".into(),
            external_version_id.to_string(),
        ]);
    }
    // Always pass --purchase like IPA_Downloader for latest/catalog installs.
    args.push("--purchase".into());
    let progress = ProcessProgress::new(
        app.clone(),
        job_id.into(),
        "download".into(),
        progress_from,
        progress_to,
    );
    progress.seed_raw(2.0);
    let stop_watch = Arc::new(AtomicBool::new(false));
    let watch = {
        let progress = progress.clone();
        let watch_dirs = watch_dirs.clone();
        let bundle_id = input.bundle_id.clone();
        let app_store_id = input.app_store_id.clone();
        let stop = stop_watch.clone();
        tokio::spawn(async move {
            watch_ipa_download(
                progress,
                watch_dirs,
                bundle_id,
                app_store_id,
                session_start_bytes,
                stop,
            )
            .await;
        })
    };
    let download_result = run_process(
        &tools.ipatool,
        &args,
        Duration::from_secs(900),
        Some(progress),
    )
    .await;
    stop_watch.store(true, Ordering::Relaxed);
    let _ = watch.await;
    let download_result = match download_result {
        Err(error) if is_transient_ipatool_timeout(&error) => {
            let append_failed = error.to_ascii_lowercase().contains("minizip: failed to open output ipa for append");
            update_job(
                app,
                job_id,
                "download",
                download_start,
                if append_failed { "IPA скачан, повторяем добавление данных Apple ID…" }
                else { "Сеть не ответила, повторяем загрузку…" },
                JobStatus::Running,
                None,
            )
            .await;
            if append_failed {
                // Keep download.ipa.tmp: ipatool resumes/verifies it, then retries
                // patching. Give Windows time to release the newly copied file.
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            ensure_not_cancelled(&state, job_id)?;
            let retry_progress = ProcessProgress::new(
                app.clone(),
                job_id.to_string(),
                "download".into(),
                progress_from,
                progress_to,
            );
            // seed_raw ждёт сырые 0..100, а download_start — уже процент
            // задачи в шкале from..to. Смешение давало скачок на старте ретрая.
            retry_progress.seed_raw(2.0);
            let retry_stop = Arc::new(AtomicBool::new(false));
            let retry_watch = {
                let progress = retry_progress.clone();
                let watch_dirs = watch_dirs.clone();
                let bundle_id = input.bundle_id.clone();
                let app_store_id = input.app_store_id.clone();
                let stop = retry_stop.clone();
                tokio::spawn(async move {
                    watch_ipa_download(
                        progress,
                        watch_dirs,
                        bundle_id,
                        app_store_id,
                        session_start_bytes,
                        stop,
                    )
                    .await;
                })
            };
            let result = run_process(
                &tools.ipatool,
                &args,
                Duration::from_secs(900),
                Some(retry_progress),
            )
            .await;
            retry_stop.store(true, Ordering::Relaxed);
            let _ = retry_watch.await;
            result
        }
        other => other,
    };
    download_result.map_err(|error| {
        // Hard storefront/auth only — don't turn a soft purchase note into a fatal
        // download error (that was aborting history installs on «app not found»).
        if is_hard_storefront_error(&error) {
            let friendly = friendly_apple_store_error(&error).unwrap_or_else(|| error.clone());
            return with_process_log(&friendly, &error);
        }
        if is_app_not_found_store_error(&error) || is_not_purchased_store_error(&error) {
            let base = if input.already_owned {
                format!(
                    "Не удалось скачать «{}» из покупок этого Apple ID.\n\n\
                     Приложение есть в истории, но файл скачать не получилось.\n\
                     Проверьте: тот же Apple ID, что в истории; на iPhone в App Store \
                     приняты условия; интернет стабилен.\n\
                     Если покупка из другого региона — нужен аккаунт того региона.",
                    input.title
                )
            } else {
                friendly_apple_store_error(&error).unwrap_or_else(|| {
                    format!(
                        "Не удалось скачать «{}».\n\n\
                         Если версии уже нет в App Store — она должна быть в покупках Apple ID.",
                        input.title
                    )
                })
            };
            let detailed = with_process_log(&base, &error);
            return match purchase_log.as_deref().filter(|log| {
                !log.is_empty() && DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed)
            }) {
                Some(log) => format!("{detailed}\n\nПопытки покупки:\n{log}"),
                None => detailed,
            };
        }
        if DETAILED_ERRORS_ENABLED.load(Ordering::Relaxed) {
            if let Some(purchase_log) = purchase_log.as_deref().filter(|log| !log.is_empty()) {
                return format!("{error}\n\nПредыдущие попытки покупки:\n{purchase_log}");
            }
            return error;
        }
        if error.contains("завершился с ошибкой") {
            format!(
                "Не удалось скачать «{}».\n\n\
                 Если версии уже нет в App Store — она должна быть в покупках Apple ID. \
                 Если приложение ещё в магазине — проверьте вход и регион аккаунта.",
                input.title
            )
        } else {
            error
        }
    })?;
    ensure_not_cancelled(&state, job_id)?;
    let path = resolve_completed_download(
        &output_path, &input.bundle_id, input.app_store_id.as_deref(),
    )?;
    validate_requested_ipa_version(&path, input)?;
    remember_real_bundle_from_ipa(app, input, &path);
    Ok(path)
}

/// After download: write real CFBundleIdentifier into history cache + live job subtitle.
fn remember_real_bundle_from_ipa(app: &AppHandle, input: &CatalogInstallInput, path: &Path) {
    let Some(real) = ipa_bundle_id(path).filter(|b| !is_placeholder_bundle_id(b)) else {
        return;
    };
    if let Some(adam) = input.app_store_id.as_deref() {
        if let Some(email) = active_account_email(app) {
            history_secrets::update_history_app_bundle(&email, adam, &real);
        }
    }
}

async fn demo_simulate_catalog_job(
    app: &AppHandle,
    job_id: &str,
    input: &CatalogInstallInput,
    export: bool,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    ensure_not_cancelled(&state, job_id)?;
    update_job(
        app,
        job_id,
        "device",
        0.0,
        "Проверка подключённого iPhone",
        JobStatus::Running,
        None,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    ensure_not_cancelled(&state, job_id)?;
    if input.temporary_us_region {
        update_job(
            app,
            job_id,
            "region",
            1.0,
            "Временно меняем страну магазина на США",
            JobStatus::Running,
            None,
        )
        .await;
        tokio::time::sleep(Duration::from_millis(900)).await;
        ensure_not_cancelled(&state, job_id)?;
    }
    for step in 1..=10u32 {
        ensure_not_cancelled(&state, job_id)?;
        update_job(
            app,
            job_id,
            if export { "export" } else { "download" },
            3.0 + f64::from(step) * 8.0,
            "Загружаем файл…",
            JobStatus::Running,
            None,
        )
        .await;
        tokio::time::sleep(Duration::from_millis(260)).await;
    }
    if !export {
        update_job(
            app,
            job_id,
            "install",
            86.0,
            "Устанавливаем на iPhone…",
            JobStatus::Running,
            None,
        )
        .await;
        tokio::time::sleep(Duration::from_millis(1400)).await;
        ensure_not_cancelled(&state, job_id)?;
    }
    if input.temporary_us_region {
        update_job(
            app,
            job_id,
            "region",
            97.0,
            "Возвращаем страну магазина обратно",
            JobStatus::Running,
            None,
        )
        .await;
        tokio::time::sleep(Duration::from_millis(800)).await;
        ensure_not_cancelled(&state, job_id)?;
    }
    if export {
        update_job(app, job_id, "export", 99.0, "Файл готов", JobStatus::Running, None).await;
    }
    Ok(())
}

fn friendly_region_switch_error(error: &str) -> Option<String> {
    let lowered = error.to_ascii_lowercase();
    let mentions_money = lowered.contains("outstanding")
        || lowered.contains("balance")
        || lowered.contains("funds")
        || lowered.contains("store credit")
        || error.contains("деньг")
        || error.contains("средств");
    mentions_money.then(|| {
        "На счету Apple ID есть деньги — пока они там, страну магазина сменить нельзя.\n\n\
         Потратьте баланс в App Store или проверьте счёт, затем повторите установку."
            .into()
    })
}

/// A temporary storefront move wrapped around one catalog job.
struct TemporaryRegionGuard {
    email: String,
    /// Country to return to — `None` when no move happened.
    original: Option<apple_region::GeneratedAddress>,
    /// Input the job runs with. After a move the app is, by definition, not in
    /// this account's purchases yet, so it has to be acquired first.
    operation_input: CatalogInstallInput,
}

/// Is this app already in the Apple ID's purchases? A non-empty version list is
/// the only proof, and it is what lets an owned app skip the storefront move.
async fn account_owns_catalog_app(app: &AppHandle, input: &CatalogInstallInput) -> bool {
    probe_version_on_account_for_catalog_version(
        app,
        &input.bundle_id,
        input.app_store_id.as_deref(),
        &JsonValue::Null,
    )
    .await
    .unwrap_or(false)
}

/// Move the Apple ID to a storefront that sells this app, if it needs one.
///
/// Signum works this out itself: an app the account already owns is
/// redownloaded from wherever the account lives, an app on sale in the
/// account's own country is simply acquired, and only an app missing from that
/// country is fetched by borrowing another storefront for the length of the
/// job. A country named by the caller (the Happ shortcut) still wins.
async fn begin_temporary_region(
    app: &AppHandle,
    job_id: &str,
    input: &CatalogInstallInput,
    progress: f64,
) -> Result<TemporaryRegionGuard, String> {
    if input.temporary_us_region && input.app_store_id.as_deref() != Some(HAPP_APP_STORE_ID) {
        return Err("Временная смена региона разрешена только для Happ.".into());
    }
    let email =
        active_account_email(app).ok_or_else(|| "Сначала войдите в Apple ID.".to_string())?;
    let mut guard = TemporaryRegionGuard {
        email,
        original: None,
        operation_input: input.clone(),
    };
    let requested: Option<String> = input
        .temporary_region
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .filter(|value| apple_region::generate_address(value).is_ok())
        .or_else(|| input.temporary_us_region.then(|| "USA".to_string()));

    let adam_id = input
        .app_store_id
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    let target_country = match requested {
        Some(code) => {
            if account_owns_catalog_app(app, input).await {
                None
            } else {
                Some(code)
            }
        }
        // Ordering keeps the common case to a single lookup: only an app that
        // the account's own storefront does not sell reaches the ownership
        // probe, and only an unowned one reaches the 43-storefront sweep.
        None => match storefront_missing_app(app, &guard.email, &adam_id).await {
            Some(account_region) => {
                if account_owns_catalog_app(app, input).await {
                    None
                } else {
                    storefront_selling_preferred(&adam_id, &account_region).await
                }
            }
            None => None,
        },
    };
    let Some(target_country) = target_country else {
        return Ok(guard);
    };

    let target_name = apple_region::country_name(&target_country);
    update_job(
        app,
        job_id,
        "region",
        progress,
        &format!("Временно меняем регион Apple ID: {target_name}"),
        JobStatus::Running,
        None,
    )
    .await;
    let original = apple_region::snapshot_current_region(&guard.email).await?;
    if original.country_code.eq_ignore_ascii_case(&target_country) {
        return Ok(guard);
    }
    save_temporary_region(app, &guard.email, &original)?;
    let target = apple_region::generate_address(&target_country)
        .map_err(|e| format!("Не удалось подготовить регион {target_country}: {e}"))?;
    if let Err(error) = apple_region::switch_saved_region(&guard.email, &target).await {
        let message = friendly_region_switch_error(&error).unwrap_or(error);
        if apple_region::restore_temporary_region(&guard.email, &original)
            .await
            .is_ok()
        {
            clear_temporary_region(app);
            return Err(message);
        }
        return Err(format!(
            "{message}\n\nНе удалось подтвердить возврат исходного региона. Signum повторит возврат при следующем запуске."
        ));
    }
    guard.original = Some(original);
    guard.operation_input.in_store = true;
    guard.operation_input.already_owned = false;
    Ok(guard)
}

/// Put the Apple ID back where it was. Runs whether the job succeeded or not.
async fn finish_temporary_region(
    app: &AppHandle,
    job_id: &str,
    guard: &TemporaryRegionGuard,
    progress: f64,
) -> Result<(), String> {
    let Some(original) = guard.original.as_ref() else {
        return Ok(());
    };
    update_job(
        app,
        job_id,
        "region",
        progress,
        &format!("Возвращаем регион Apple ID: {}", original.country_name),
        JobStatus::Running,
        None,
    )
    .await;
    let result = apple_region::restore_temporary_region(&guard.email, original).await;
    if result.is_ok() {
        clear_temporary_region(app);
    }
    result
}

/// A failed restore must never be silent: the account is left in a foreign
/// storefront and the user has to be told which one to undo.
fn combine_region_outcome(
    operation: Result<(), String>,
    restore: Result<(), String>,
    done_noun: &str,
) -> Result<(), String> {
    match (operation, restore) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(operation_error), Ok(())) => Err(operation_error),
        (Ok(()), Err(restore_error)) => Err(format!(
            "{done_noun}, но {restore_error}\n\nОткройте «Аккаунт» и верните исходную страну вручную."
        )),
        (Err(operation_error), Err(restore_error)) => Err(format!(
            "{operation_error}\n\nКроме того, {restore_error}\nОткройте «Аккаунт» и верните исходную страну вручную."
        )),
    }
}

async fn process_catalog(
    app: &AppHandle,
    job_id: &str,
    input: &CatalogInstallInput,
) -> Result<(), String> {
    if DEMO_MODE {
        return demo_simulate_catalog_job(app, job_id, input, false).await;
    }
    let state = app.state::<AppState>();
    ensure_not_cancelled(&state, job_id)?;
    update_job(
        app,
        job_id,
        "device",
        0.0,
        "Проверка подключённого iPhone",
        JobStatus::Running,
        None,
    )
    .await;
    ensure_not_cancelled(&state, job_id)?;
    let device = get_device_info_inner(app).await?;
    if let (Some(required), Some(current)) = (&input.min_ios, &device.product_version) {
        if compare_versions(current, required) < 0 {
            return Err(format!(
                "Нужна iOS {required} или выше; на устройстве iOS {current}"
            ));
        }
    }
    require_auth(app)?;
    ensure_not_cancelled(&state, job_id)?;
    let _guard = state.ipatool_lock.lock().await;
    ensure_active_ipatool_session(app).await?;
    let guard = begin_temporary_region(app, job_id, input, 1.0).await?;
    let operation = async {
        let ipa =
            download_catalog_ipa_inner(app, job_id, &guard.operation_input, 3.0, 85.0).await?;
        update_job(
            app,
            job_id,
            "install",
            85.0,
            "Установка на iPhone",
            JobStatus::Running,
            None,
        )
        .await;
        install_ipa(app, job_id, &ipa, 85.0, 96.0).await?;
        cleanup_catalog_download(job_id, &ipa);
        Ok(())
    }
    .await;
    let restore = finish_temporary_region(app, job_id, &guard, 97.0).await;
    clear_account_version_cache(&state);
    combine_region_outcome(operation, restore, "Приложение установлено на iPhone")
}

async fn process_catalog_export(
    app: &AppHandle,
    job_id: &str,
    input: &CatalogInstallInput,
) -> Result<(), String> {
    if DEMO_MODE {
        return demo_simulate_catalog_job(app, job_id, input, true).await;
    }
    let state = app.state::<AppState>();
    require_auth(app)?;
    ensure_not_cancelled(&state, job_id)?;
    let _guard = state.ipatool_lock.lock().await;
    ensure_active_ipatool_session(app).await?;
    // A file download needs the same storefront as an install: an app that is
    // not sold where the account lives cannot be acquired without borrowing one.
    let region = begin_temporary_region(app, job_id, input, 1.0).await?;
    let operation = async {
        let ipa =
            download_catalog_ipa_inner(app, job_id, &region.operation_input, 2.0, 92.0).await?;
        ensure_not_cancelled(&state, job_id)?;
        update_job(
            app,
            job_id,
            "export",
            93.0,
            "Подготовка файла для сохранения…",
            JobStatus::Running,
            None,
        )
        .await;
        validate_requested_ipa_version(&ipa, input)?;
        let actual_version = cached_ipa_metadata(&ipa).2;
        let version = actual_version.as_deref().unwrap_or("");
        ensure_not_cancelled(&state, job_id)?;
        // Файл от ipatool склеен дозаписью: iPhone скачает такой IPA по ссылке и
        // откажется ставить. Пересобираем архив, содержимое остаётся тем же.
        update_job(
            app,
            job_id,
            "export",
            96.0,
            "Пересобираем файл…",
            JobStatus::Running,
            None,
        )
        .await;
        let repack_target = ipa.clone();
        tokio::task::spawn_blocking(move || apple_signing::repack_ipa(&repack_target))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("Не удалось пересобрать IPA: {e}"))?;
        ensure_not_cancelled(&state, job_id)?;
        let dest = move_ipa_to_exports(app, &ipa, &input.title, version)?;
        cleanup_catalog_download(job_id, &ipa);
        let saved_msg = format!("Сохранено: {}", dest.display());
        update_job(
            app,
            job_id,
            "export",
            99.0,
            &saved_msg,
            JobStatus::Running,
            None,
        )
        .await;
        Ok(())
    }
    .await;
    let restore = finish_temporary_region(app, job_id, &region, 97.0).await;
    combine_region_outcome(operation, restore, "Файл сохранён")
}

async fn process_sign_and_install(
    app: &AppHandle,
    job_id: &str,
    input: &apple_signing::SignAndInstallInput,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    ensure_not_cancelled(&state, job_id)?;

    let requested_udid = input
        .udid
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .or_else(|| selected_device_udid(app));
    let devices = if input.install {
        list_connected_devices_inner(app).await?
    } else {
        list_connected_devices_inner(app).await.unwrap_or_default()
    };
    let connected = requested_udid
        .as_ref()
        .and_then(|udid| devices.iter().find(|device| device.udid == *udid));

    let (udid, device_name) = if input.install {
        update_job(
            app,
            job_id,
            "device",
            1.0,
            "Проверяем iPhone…",
            JobStatus::Running,
            None,
        )
        .await;
        let device = connected
            .cloned()
            .or_else(|| {
                devices
                    .iter()
                    .find(|item| item.connection != "wifi")
                    .cloned()
                    .or_else(|| devices.first().cloned())
            })
            .ok_or_else(|| device_not_found_message())?;
        remember_connected_device(app, &device);
        let udid = device.udid.clone();
        let name = device
            .device_name
            .clone()
            .unwrap_or_else(|| "iPhone".into());
        (Some(udid), name)
    } else if let Some(device) = connected {
        let name = device
            .device_name
            .clone()
            .unwrap_or_else(|| "iPhone".into());
        update_job(
            app,
            job_id,
            "sign",
            1.0,
            "Подписываем без установки…",
            JobStatus::Running,
            None,
        )
        .await;
        (Some(device.udid.clone()), name)
    } else if let Some(udid) = requested_udid {
        update_job(
            app,
            job_id,
            "sign",
            1.0,
            "Подписываем без кабеля…",
            JobStatus::Running,
            None,
        )
        .await;
        (Some(udid), "iPhone".into())
    } else {
        update_job(
            app,
            job_id,
            "sign",
            1.0,
            "Подписываем без iPhone…",
            JobStatus::Running,
            None,
        )
        .await;
        (None, "iPhone".into())
    };

    ensure_not_cancelled(&state, job_id)?;
    update_job(
        app,
        job_id,
        "auth",
        8.0,
        "Проверяем Apple ID…",
        JobStatus::Running,
        None,
    )
    .await;

    let email = active_account_email(app).ok_or_else(|| "SIGN_IPA_AUTH_REQUIRED".to_string())?;
    let password = history_secrets::load_history_password(&email)
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_default();
    // CertMode::Custom не трогает Apple ID — пароль не нужен.
    if password.is_empty() && !matches!(input.cert_mode, apple_signing::CertMode::Custom) {
        return Err(
            "SIGN_IPA_AUTH_REQUIRED: нет сохранённого пароля. Войдите снова на странице «Аккаунт»."
                .to_string(),
        );
    }

    let tools = resolve_tool_paths(app)?;
    // Free/paid sign uses isideload (codesign / internal pack), not the Windows
    // zsign helper. Keep an optional path for API compatibility; never hard-fail
    // when zsign is absent (macOS/Linux never ship it).
    let zsign = tools
        .zsign
        .as_deref()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("zsign"));
    let anisette = tools.anisette.as_deref().map(PathBuf::from);
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data: {e}"))?;
    let cache_dir = apple_signing::signing_cache_dir(&app_data);
    let signed_dir = if input.save_signed {
        input
            .output_dir
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| apple_signing::default_signed_output_dir(&app_data))
    } else {
        apple_signing::default_signed_output_dir(&app_data)
    };
    fs::create_dir_all(&signed_dir).map_err(|e| format!("Не удалось создать папку: {e}"))?;

    let ipa_path = PathBuf::from(input.ipa_path.trim());
    let stem = ipa_path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("signed");
    let output_ipa = signed_dir.join(format!("{stem}-signed.ipa"));

    let app_for_progress = app.clone();
    let job_id_owned = job_id.to_string();
    // CertMode::Custom → zsign с сохранённым/указанным сертификатом.
    // CertMode::Auto/Free/Paid → isideload через Apple ID.
    let result = if matches!(input.cert_mode, apple_signing::CertMode::Custom) {
        let app_data = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("app data: {e}"))?;
        // Либо из сохранённого имени, либо явные пути из UI.
        let (p12_path, prov_path, pass) = if let Some(name) =
            input.cert_name.as_deref().map(str::trim).filter(|v| !v.is_empty())
        {
            let (p12, prov, pass) = apple_signing::load_custom_cert(&app_data, name)?;
            (p12, prov, pass)
        } else {
            let p12 = input
                .custom_p12_path
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| "Укажите файл сертификата .p12".to_string())?;
            let prov = input
                .custom_prov_path
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| "Укажите файл .mobileprovision".to_string())?;
            let pass = input
                .custom_cert_password
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned);
            (p12, prov, pass)
        };
        let pass = pass.unwrap_or_default();
        apple_signing::sign_ipa_with_custom_cert(
            app,
            &ipa_path,
            &p12_path,
            &prov_path,
            &pass,
            &input.options,
            &zsign,
            &cache_dir,
            &output_ipa,
            |pct, msg| {
                let phase = "sign";
                let app2 = app_for_progress.clone();
                let jid = job_id_owned.clone();
                let message = msg.to_string();
                let percent = pct.min(89.0);
                tauri::async_runtime::spawn(async move {
                    update_job(
                        &app2,
                        &jid,
                        phase,
                        percent,
                        &message,
                        JobStatus::Running,
                        None,
                    )
                    .await;
                });
            },
        )
        .await?
    } else {
        apple_signing::sign_ipa_for_device(
            app,
            &ipa_path,
            udid.as_deref(),
            &device_name,
            &email,
            &password,
            apple_signing::CertMode::Auto,
            &input.options,
            anisette.as_deref(),
            &zsign,
            &cache_dir,
            &output_ipa,
            |pct, msg| {
                let phase = if pct < 15.0 {
                    "auth"
                } else if pct < 55.0 {
                    "cert"
                } else {
                    "sign"
                };
                // Fire-and-forget progress (sync callback)
                let app2 = app_for_progress.clone();
                let jid = job_id_owned.clone();
                let message = msg.to_string();
                let percent = pct.min(89.0);
                tauri::async_runtime::spawn(async move {
                    update_job(
                        &app2,
                        &jid,
                        phase,
                        percent,
                        &message,
                        JobStatus::Running,
                        None,
                    )
                    .await;
                });
            },
        )
        .await?
    };

    ensure_not_cancelled(&state, job_id)?;

    if input.install {
        update_job(
            app,
            job_id,
            "install",
            90.0,
            "Устанавливаем подписанный IPA…",
            JobStatus::Running,
            None,
        )
        .await;
        install_ipa(app, job_id, &result.signed_ipa, 90.0, 99.0)
            .await
            .map_err(|e| format!("SIGN_IPA_INSTALL_FAILED: {e}"))?;
    } else {
        update_job(
            app,
            job_id,
            "export",
            99.0,
            &format!("Подписанный IPA: {}", result.signed_ipa.display()),
            JobStatus::Running,
            None,
        )
        .await;
    }

    // Enrich job subtitle with team info
    {
        let mut jobs = state.jobs.lock().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.subtitle = format!(
                "{} · {} · {}",
                result.metadata.bundle_id, result.team_kind, result.team_name
            );
        }
    }

    remember_signed_app(
        app,
        input,
        &result.metadata.bundle_id,
        &result.metadata.name,
        &result.team_kind,
    );

    Ok(())
}

/// Bank alt install: download .plist from bank → IPA URL → download IPA → install on iPhone.
/// User signs into **App Store** with bank login/password in parallel (not Settings/iCloud).
async fn process_bank_plist_install(
    app: &AppHandle,
    job_id: &str,
    bank: bank_otp::BankKey,
    link: &str,
    title: &str,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    ensure_not_cancelled(&state, job_id)?;

    let push_phone = |phase: &str, percent: f64, message: &str, status: &str| {
        bank_creds_page::update_job_status(job_id, phase, percent, message, status);
    };

    update_job(
        app,
        job_id,
        "device",
        3.0,
        "Проверка iPhone…",
        JobStatus::Running,
        None,
    )
    .await;
    push_phone("device", 3.0, "Проверяем iPhone…", "running");
    let _device = get_device_info_inner(app).await.map_err(|e| {
        push_phone("device", 3.0, &e, "failed");
        e
    })?;

    update_job(
        app,
        job_id,
        "resolve",
        10.0,
        "Читаем .plist установки от банка…",
        JobStatus::Running,
        None,
    )
    .await;
    push_phone("resolve", 10.0, "Читаем установку от банка…", "running");

    let manifest = bank_otp::fetch_manifest_ipa(link).await.map_err(|e| {
        push_phone("resolve", 10.0, &e, "failed");
        e
    })?;
    let app_title = manifest
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if title.trim().is_empty() {
                bank.display_name().to_string()
            } else {
                title.to_string()
            }
        });

    {
        let mut jobs = state.jobs.lock().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.title = app_title.clone();
            job.subtitle = format!("{} · альтернативный способ", bank.display_name());
        }
    }

    let settings = load_settings(app)?;
    fs::create_dir_all(&settings.downloads_dir).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let safe_name = app_title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let ipa_path = PathBuf::from(&settings.downloads_dir)
        .join(format!("{safe_name}-{}-{stamp}.ipa", bank.as_str()));

    update_job(
        app,
        job_id,
        "download",
        18.0,
        &format!("Скачиваем «{app_title}»…"),
        JobStatus::Running,
        None,
    )
    .await;
    push_phone(
        "download",
        18.0,
        &format!("Скачиваем «{app_title}» на компьютер…"),
        "running",
    );

    let app_for_progress = app.clone();
    let job_id_owned = job_id.to_string();
    let title_for_progress = app_title.clone();
    bank_otp::download_ipa_file(&manifest.ipa_url, &ipa_path, move |pct| {
        let percent = 18.0 + (pct / 100.0) * 62.0;
        bank_creds_page::update_job_status(
            &job_id_owned,
            "download",
            percent.min(80.0),
            &format!("Скачиваем «{title_for_progress}»… {:.0}%", pct.min(100.0)),
            "running",
        );
        let app2 = app_for_progress.clone();
        let jid = job_id_owned.clone();
        tauri::async_runtime::spawn(async move {
            update_job(
                &app2,
                &jid,
                "download",
                percent.min(80.0),
                &format_download_progress_label(pct.min(100.0)),
                JobStatus::Running,
                None,
            )
            .await;
        });
    })
    .await
    .map_err(|e| {
        push_phone("download", 50.0, &e, "failed");
        e
    })?;

    ensure_not_cancelled(&state, job_id)?;
    update_job(
        app,
        job_id,
        "install",
        85.0,
        "Установка на iPhone…",
        JobStatus::Running,
        None,
    )
    .await;
    push_phone(
        "install",
        85.0,
        "Устанавливаем на iPhone… Войдите в App Store, если ещё не вошли.",
        "running",
    );
    match install_ipa(app, job_id, &ipa_path, 85.0, 99.0).await {
        Ok(()) => {
            push_phone(
                "done",
                100.0,
                "Готово: приложение установлено на iPhone",
                "done",
            );
            Ok(())
        }
        Err(e) => {
            push_phone("install", 90.0, &e, "failed");
            Err(e)
        }
    }
}

async fn install_worker(app: AppHandle) {
    loop {
        let state = app.state::<AppState>();
        let item = state.queue.next().await;
        let should_run = state.jobs.lock().await.get(item.id())
            .is_some_and(|job| matches!(job.status, JobStatus::Queued));
        if !should_run {
            clear_cancel_flag(&state, item.id());
            *state.queue.current.lock().await = None;
            continue;
        }
        let (id, result, is_export) = match &item {
            QueuedInstall::Catalog { job_id, input } => {
                if is_job_cancelled(&state, job_id) {
                    (job_id.clone(), Err("Установка отменена".into()), false)
                } else {
                    (
                        job_id.clone(),
                        process_catalog(&app, job_id, input).await,
                        false,
                    )
                }
            }
            QueuedInstall::Export { job_id, input } => {
                if is_job_cancelled(&state, job_id) {
                    (job_id.clone(), Err("Загрузка отменена".into()), true)
                } else {
                    (
                        job_id.clone(),
                        process_catalog_export(&app, job_id, input).await,
                        true,
                    )
                }
            }
            QueuedInstall::Local { job_id, path } => {
                if is_job_cancelled(&state, job_id) {
                    (job_id.clone(), Err("Установка отменена".into()), false)
                } else {
                    update_job(
                        &app,
                        job_id,
                        "install",
                        0.0,
                        "Установка локального IPA",
                        JobStatus::Running,
                        None,
                    )
                    .await;
                    (
                        job_id.clone(),
                        install_ipa(&app, job_id, path, 0.0, 99.0).await,
                        false,
                    )
                }
            }
            QueuedInstall::OpenExportsFolder { job_id } => {
                if is_job_cancelled(&state, job_id) {
                    (job_id.clone(), Err("Открытие папки отменено".into()), true)
                } else {
                    update_job(
                        &app,
                        job_id,
                        "export",
                        50.0,
                        "Открываем папку экспорта…",
                        JobStatus::Running,
                        None,
                    )
                    .await;
                    (job_id.clone(), open_exports_folder(app.clone()), true)
                }
            }
            QueuedInstall::SignAndInstall { job_id, input } => {
                if is_job_cancelled(&state, job_id) {
                    (job_id.clone(), Err("SIGN_IPA_CANCELLED".into()), false)
                } else {
                    (
                        job_id.clone(),
                        process_sign_and_install(&app, job_id, input).await,
                        false,
                    )
                }
            }
            QueuedInstall::BankPlistInstall {
                job_id,
                bank,
                link,
                title,
            } => {
                if is_job_cancelled(&state, job_id) {
                    (job_id.clone(), Err("Установка отменена".into()), false)
                } else {
                    (
                        job_id.clone(),
                        process_bank_plist_install(&app, job_id, *bank, link, title).await,
                        false,
                    )
                }
            }
        };
        let result = if is_job_cancelled(&state, &id) { Err("Установка отменена".into()) } else { result };
        let account_email = active_account_email(&app);
        let device_udid = selected_device_udid(&app);
        let job_snapshot = state.jobs.lock().await.get(&id).cloned();
        let done_message = match job_snapshot.as_ref().map(|job| job.kind.as_str()) {
            Some("open_exports") => "Папка Signum открыта",
            Some("export") => "IPA сохранён в папку экспорта",
            _ => "Установка завершена",
        };
        match result {
            Ok(()) => {
                update_job(
                    &app,
                    &id,
                    "complete",
                    100.0,
                    done_message,
                    JobStatus::Done,
                    None,
                )
                .await;
                if let Some(job) = job_snapshot.as_ref() {
                    if job.kind != "open_exports" {
                        record_job_history(&app, job, "done", account_email, device_udid);
                        notify_job_finished(&app, job, true);
                    }
                }
                remove_pending_job(&app, &id);
                clear_cancel_flag(&state, &id);
            }
            Err(error)
                if error == "Установка отменена"
                    || error == "Загрузка отменена"
                    || error == "Открытие папки отменено" =>
            {
                update_job(
                    &app,
                    &id,
                    "cancelled",
                    0.0,
                    &error,
                    JobStatus::Cancelled,
                    None,
                )
                .await;
                if let Some(job) = job_snapshot.as_ref() {
                    record_job_history(&app, job, "cancelled", account_email, device_udid);
                }
                remove_pending_job(&app, &id);
                clear_cancel_flag(&state, &id);
            }
            Err(error) => {
                if is_keychain_integrity_error(&error) {
                    clear_auth_state(&app);
                }
                let failed_phase = state
                    .jobs
                    .lock()
                    .await
                    .get(&id)
                    .map(|job| job.phase.clone())
                    .unwrap_or_else(|| "failed".into());
                remove_pending_job(&app, &id);
                clear_cancel_flag(&state, &id);
                if let Some(job) = job_snapshot.as_ref() {
                    record_job_history(&app, job, "failed", account_email, device_udid);
                    if matches!(job.kind.as_str(), "install" | "export") {
                        notify_job_finished(&app, job, false);
                    }
                }
                update_job(
                    &app,
                    &id,
                    &failed_phase,
                    0.0,
                    if is_export {
                        "Загрузка не выполнена"
                    } else {
                        "Установка не выполнена"
                    },
                    JobStatus::Failed,
                    Some(error.clone()),
                )
                .await;
            }
        }
        *state.queue.current.lock().await = None;
    }
}

fn require_confirmed_catalog_ownership(input: &CatalogInstallInput) -> Result<(), String> {
    // Happ has an explicit acquire-on-demand flow that buys it after a temporary
    // US storefront switch. All other catalog jobs must come from a version that
    // list-versions confirmed on the active Apple ID.
    if input.temporary_us_region && input.app_store_id.as_deref() == Some(HAPP_APP_STORE_ID) {
        return Ok(());
    }
    // По выбору пользователя в поиске регион временно меняется на подходящий
    // — процесс тот же что и у Happ: сначала купить, потом вернуть регион.
    if input.temporary_region.is_some() {
        return Ok(());
    }
    // App Store «в наличии»: это до-покупка, не «подтверждённое владение».
    // Плюс уже купленные (already_owned). И только их.
    if input.in_store && !input.already_owned {
        return Ok(());
    }
    if !input.already_owned {
        return Err(
            "Эта версия не подтверждена в покупках активного Apple ID. \
             Установить или скачать её нельзя. Откройте Apple ID, на котором приложение приобреталось, и обновите список версий."
                .into(),
        );
    }
    Ok(())
}

fn normalize_catalog_install_input(
    mut input: CatalogInstallInput,
) -> Result<CatalogInstallInput, String> {
    let store = input
        .app_store_id
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
    if input.bundle_id.trim().is_empty() || input.bundle_id.trim() == "NONE" {
        if let Some(id) = store.clone() {
            // Temporary placeholder only when adamId is known; replaced by iTunes/IPA ASAP.
            // Jobs display "App Store ID {id}" via catalog_job_subtitle — not this string.
            input.bundle_id = format!("app.store.{id}");
            input.app_store_id = Some(id);
        } else {
            return Err("Нужен bundle_id или App Store id приложения".into());
        }
    }
    if input
        .app_store_id
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        input.app_store_id = store;
    }
    if input.temporary_us_region && input.app_store_id.as_deref() != Some(HAPP_APP_STORE_ID) {
        return Err("Временная смена региона разрешена только для Happ.".into());
    }
    // NEVER force in_store=true just because adamId is present.
    // Catalog REMOVED / «Не в Store» versions must stay in_store=false → no purchase,
    // only probe ownership + download if already on the Apple ID.
    // already_owned (history) also skips purchase via its own path.
    Ok(input)
}

#[derive(Debug, Deserialize)]
struct BankAltSendSmsInput {
    bank: String,
    phone: String,
}

#[derive(Debug, Deserialize)]
struct BankAltConfirmInput {
    session_id: String,
    code: String,
}

/// Step 1: send SMS for Sovcombank / Rocketbank alternative install.
#[tauri::command]
async fn bank_alt_send_sms(
    state: State<'_, AppState>,
    input: BankAltSendSmsInput,
) -> Result<bank_otp::BankSendSmsResult, String> {
    let bank = bank_otp::BankKey::parse(&input.bank)
        .ok_or_else(|| "Укажите банк: sovcombank или rocketbank".to_string())?;
    let (session, message) = bank_otp::send_sms(bank, &input.phone).await?;
    let session_id = Uuid::new_v4().to_string();
    let phone_display = bank_otp::format_phone_display(&session.phone);
    state.bank_otp.put(session_id.clone(), session);
    Ok(bank_otp::BankSendSmsResult {
        session_id,
        phone_display,
        bank: bank.as_str().into(),
        bank_name: bank.display_name().into(),
        message,
    })
}

#[derive(Debug, Clone, Serialize)]
struct BankAltConfirmResult {
    job_id: String,
    bank: String,
    bank_name: String,
    login: String,
    password: String,
    /// Manifest / install link from bank (plist or itms-services).
    link: Option<String>,
    /// LAN page for iPhone (QR) — copy login/password on the phone.
    page_url: Option<String>,
    /// True if mini-server + LAN IP available for QR.
    page_ready: bool,
    /// Seconds left for this bank Apple ID (~30 min from obtain).
    remaining_secs: i64,
    expires_in_label: String,
    /// True if restored without new phone/SMS.
    from_cache: bool,
}

/// Step 2: SMS code → show bank Apple ID to user + queue IPA install from bank .plist.
#[tauri::command]
async fn bank_alt_confirm_and_install(
    app: AppHandle,
    state: State<'_, AppState>,
    input: BankAltConfirmInput,
) -> Result<BankAltConfirmResult, String> {
    let mut session = state
        .bank_otp
        .get_mut_clone(&input.session_id)
        .ok_or_else(|| "Сессия OTP истекла. Отправьте SMS снова.".to_string())?;
    bank_otp::validate_otp(&mut session, &input.code).await?;
    state.bank_otp.update(&input.session_id, session.clone());
    let creds = bank_otp::get_credentials(&session).await?;
    let _ = state.bank_otp.take(&input.session_id);

    let bank = session.bank;
    let link = creds
        .link
        .clone()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            "Банк не вернул ссылку на установку (.plist). Повторите попытку позже.".to_string()
        })?;

    let id = Uuid::new_v4().to_string();
    let job = InstallJob {
        id: id.clone(),
        title: bank.display_name().into(),
        subtitle: format!("{} · альтернативный способ", bank.display_name()),
        phase: "queue".into(),
        percent: 0.0,
        message: "Скачиваем IPA и ставим на iPhone…".into(),
        status: JobStatus::Queued,
        error: None,
        created_at: now(),
        kind: "install".into(),
        retryable: false,
        retry_payload: None,
    };
    state.jobs.lock().await.insert(id.clone(), job);
    register_cancel_flag(&state, &id);
    // Keep 30 minutes — reopening modal must not require phone/SMS again.
    state
        .bank_otp
        .save_creds(bank, &creds.login, &creds.password, &link);

    state
        .queue
        .send(QueuedInstall::BankPlistInstall {
            job_id: id.clone(),
            bank,
            link: link.clone(),
            title: bank.display_name().into(),
        })
        .await
        .map_err(|_| "Очередь установок остановлена".to_string())?;

    // Mini LAN server + QR page so user can copy Apple ID on iPhone.
    let (page_url, page_ready) = match bank_creds_page::publish_creds_page(
        bank.display_name(),
        &creds.login,
        &creds.password,
        &id,
    )
    .await
    {
        Ok((_token, url)) => (Some(url), true),
        Err(_) => (None, false),
    };

    let _ = app;
    Ok(BankAltConfirmResult {
        job_id: id,
        bank: bank.as_str().into(),
        bank_name: bank.display_name().into(),
        login: creds.login,
        password: creds.password,
        link: Some(link),
        page_url,
        page_ready,
        remaining_secs: bank_otp::BANK_CREDS_TTL_SECS as i64,
        expires_in_label: "ещё 30 мин".into(),
        from_cache: false,
    })
}

#[derive(Debug, Deserialize)]
struct BankAltBankInput {
    bank: String,
}

/// Peek 30‑min bank Apple ID without re-SMS (accidental close recovery).
#[tauri::command]
fn bank_alt_peek_cache(
    state: State<'_, AppState>,
    input: BankAltBankInput,
) -> Result<Option<bank_otp::BankCredsCachePeek>, String> {
    let bank = bank_otp::BankKey::parse(&input.bank)
        .ok_or_else(|| "Укажите банк: sovcombank или rocketbank".to_string())?;
    Ok(state.bank_otp.peek_creds(bank))
}

/// Resume within 30 min: show creds/QR again and re-queue IPA install from saved link.
#[tauri::command]
async fn bank_alt_resume_from_cache(
    app: AppHandle,
    state: State<'_, AppState>,
    input: BankAltBankInput,
) -> Result<BankAltConfirmResult, String> {
    let bank = bank_otp::BankKey::parse(&input.bank)
        .ok_or_else(|| "Укажите банк: sovcombank или rocketbank".to_string())?;
    let cached = state
        .bank_otp
        .get_active_creds(bank)
        .ok_or_else(|| "Учётка банка истекла (30 мин). Введите телефон и код снова.".to_string())?;

    let id = Uuid::new_v4().to_string();
    let job = InstallJob {
        id: id.clone(),
        title: bank.display_name().into(),
        subtitle: format!("{} · альтернативный способ", bank.display_name()),
        phase: "queue".into(),
        percent: 0.0,
        message: "Скачиваем IPA и ставим на iPhone…".into(),
        status: JobStatus::Queued,
        error: None,
        created_at: now(),
        kind: "install".into(),
        retryable: false,
        retry_payload: None,
    };
    state.jobs.lock().await.insert(id.clone(), job);
    register_cancel_flag(&state, &id);
    state
        .queue
        .send(QueuedInstall::BankPlistInstall {
            job_id: id.clone(),
            bank,
            link: cached.link.clone(),
            title: bank.display_name().into(),
        })
        .await
        .map_err(|_| "Очередь установок остановлена".to_string())?;

    let (page_url, page_ready) = match bank_creds_page::publish_creds_page(
        bank.display_name(),
        &cached.login,
        &cached.password,
        &id,
    )
    .await
    {
        Ok((_token, url)) => (Some(url), true),
        Err(_) => (None, false),
    };

    let rem = cached.remaining_secs().max(0);
    let _ = app;
    Ok(BankAltConfirmResult {
        job_id: id,
        bank: bank.as_str().into(),
        bank_name: bank.display_name().into(),
        login: cached.login,
        password: cached.password,
        link: Some(cached.link),
        page_url,
        page_ready,
        remaining_secs: rem,
        expires_in_label: format_remaining_label_ui(rem),
        from_cache: true,
    })
}

/// Drop cached bank credentials (force new phone/SMS).
#[tauri::command]
fn bank_alt_clear_cache(state: State<'_, AppState>, input: BankAltBankInput) -> Result<(), String> {
    let bank = bank_otp::BankKey::parse(&input.bank)
        .ok_or_else(|| "Укажите банк: sovcombank или rocketbank".to_string())?;
    state.bank_otp.clear_creds(bank);
    Ok(())
}

/// Stop QR mini-site immediately (user closed the window / finished early).
#[tauri::command]
fn bank_alt_stop_page() -> Result<(), String> {
    bank_creds_page::stop_all();
    Ok(())
}

fn format_remaining_label_ui(secs: i64) -> String {
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

async fn enqueue_catalog_install_inner(
    app: AppHandle,
    state: &AppState,
    input: CatalogInstallInput,
) -> Result<String, String> {
    let mut input = normalize_catalog_install_input(input)?;
    require_confirmed_catalog_ownership(&input)?;
    // Resolve real bundle before the job appears in the UI when possible.
    resolve_catalog_bundle_id(&mut input).await;
    let id = Uuid::new_v4().to_string();
    let job = InstallJob {
        id: id.clone(),
        title: input.title.clone(),
        subtitle: catalog_job_subtitle(&input),
        phase: "queue".into(),
        percent: 0.0,
        message: "Добавлено в очередь".into(),
        status: JobStatus::Queued,
        error: None,
        created_at: now(),
        kind: "install".into(),
        retryable: true,
        retry_payload: Some(RetryPayload::Catalog(input.clone())),
    };
    state.jobs.lock().await.insert(id.clone(), job.clone());
    register_cancel_flag(&state, &id);
    persist_pending_job(
        &app,
        PendingJobRecord {
            id: id.clone(),
            kind: "install".into(),
            title: job.title.clone(),
            subtitle: job.subtitle.clone(),
            catalog_input: Some(input.clone()),
            local_path: None,
            sign_input: None,
            created_at: job.created_at,
        },
    );
    state
        .queue
        .send(QueuedInstall::Catalog {
            job_id: id.clone(),
            input,
        })
        .await
        .map_err(|_| "Очередь установок остановлена".to_string())?;
    Ok(id)
}

#[tauri::command]
async fn enqueue_catalog_install(
    app: AppHandle,
    state: State<'_, AppState>,
    input: CatalogInstallInput,
) -> Result<String, String> {
    enqueue_catalog_install_inner(app, state.inner(), input).await
}

#[tauri::command]
async fn enqueue_catalog_export(
    app: AppHandle,
    state: State<'_, AppState>,
    input: CatalogInstallInput,
) -> Result<String, String> {
    let mut input = normalize_catalog_install_input(input)?;
    require_confirmed_catalog_ownership(&input)?;
    resolve_catalog_bundle_id(&mut input).await;
    let id = Uuid::new_v4().to_string();
    let job = InstallJob {
        id: id.clone(),
        title: input.title.clone(),
        subtitle: format!("Экспорт · {}", catalog_job_subtitle(&input)),
        phase: "queue".into(),
        percent: 0.0,
        message: "Добавлено в очередь загрузки".into(),
        status: JobStatus::Queued,
        error: None,
        created_at: now(),
        kind: "export".into(),
        retryable: true,
        retry_payload: Some(RetryPayload::Export(input.clone())),
    };
    state.jobs.lock().await.insert(id.clone(), job.clone());
    register_cancel_flag(&state, &id);
    persist_pending_job(
        &app,
        PendingJobRecord {
            id: id.clone(),
            kind: "export".into(),
            title: job.title.clone(),
            subtitle: job.subtitle.clone(),
            catalog_input: Some(input.clone()),
            local_path: None,
            sign_input: None,
            created_at: job.created_at,
        },
    );
    state
        .queue
        .send(QueuedInstall::Export {
            job_id: id.clone(),
            input,
        })
        .await
        .map_err(|_| "Очередь загрузок остановлена".to_string())?;
    Ok(id)
}

#[tauri::command]
async fn enqueue_open_exports_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let exports_dir = configured_exports_dir(&app)?;
    let id = Uuid::new_v4().to_string();
    let job = InstallJob {
        id: id.clone(),
        title: "Открыть папку Signum".into(),
        subtitle: exports_dir.to_string_lossy().into_owned(),
        phase: "queue".into(),
        percent: 0.0,
        message: "Добавлено в очередь".into(),
        status: JobStatus::Queued,
        error: None,
        created_at: now(),
        kind: "open_exports".into(),
        retryable: false,
        retry_payload: Some(RetryPayload::OpenExportsFolder),
    };
    state.jobs.lock().await.insert(id.clone(), job.clone());
    register_cancel_flag(&state, &id);
    persist_pending_job(
        &app,
        PendingJobRecord {
            id: id.clone(),
            kind: "open_exports".into(),
            title: job.title.clone(),
            subtitle: job.subtitle.clone(),
            catalog_input: None,
            local_path: None,
            sign_input: None,
            created_at: job.created_at,
        },
    );
    state
        .queue
        .send(QueuedInstall::OpenExportsFolder { job_id: id.clone() })
        .await
        .map_err(|_| "Очередь остановлена".to_string())?;
    Ok(id)
}

#[tauri::command]
fn get_exports_dir(app: AppHandle) -> Result<String, String> {
    configured_exports_dir(&app).map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn open_exports_folder(app: AppHandle) -> Result<(), String> {
    let dir = configured_exports_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        StdCommand::new("explorer")
            .arg(dir.as_os_str())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        StdCommand::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        StdCommand::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn cancel_install_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<(), String> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err("Не указана задача установки".into());
    }
    let mut jobs = state.jobs.lock().await;
    let Some(job) = jobs.get(&job_id) else {
        return Err("Задача не найдена".into());
    };
    if matches!(
        job.status,
        JobStatus::Done | JobStatus::Failed | JobStatus::Cancelled
    ) {
        return Err("Эту задачу уже нельзя отменить".into());
    }
    if let Some(flag) = state.cancel_flags.lock().unwrap().get(&job_id) {
        flag.store(true, Ordering::Relaxed);
    }
    let mut updated = job.clone();
    updated.status = JobStatus::Cancelled;
    updated.phase = "cancelled".into();
    updated.message = "Установка отменена".into();
    updated.percent = 0.0;
    updated.error = None;
    jobs.insert(job_id.clone(), updated);
    drop(jobs);
    remove_pending_job(&app, &job_id);
    state.queue.pending.lock().await.retain(|item| item.id() != job_id);
    if state.queue.current.lock().await.as_deref() != Some(job_id.as_str()) {
        clear_cancel_flag(&state, &job_id);
    }
    let _ = app.emit(
        "install-progress",
        ProgressEvent {
            job_id,
            phase: "cancelled".into(),
            percent: 0.0,
            message: "Установка отменена".into(),
            status: JobStatus::Cancelled,
            error: None,
        },
    );
    Ok(())
}

#[tauri::command]
async fn retry_install_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<String, String> {
    if state.queue.current.lock().await.as_deref() == Some(job_id.as_str()) {
        return Err("Дождитесь завершения отмены текущей операции.".into());
    }
    let original = state
        .jobs
        .lock()
        .await
        .get(&job_id)
        .cloned()
        .ok_or_else(|| "Задача не найдена".to_string())?;
    if !matches!(&original.status, JobStatus::Failed | JobStatus::Cancelled) {
        return Err("Повторить можно только задачу с ошибкой или отменённую задачу".into());
    }
    let payload = original
        .retry_payload
        .clone()
        .ok_or_else(|| "Для этой задачи не сохранены параметры повтора".to_string())?;
    if let RetryPayload::Local(path) = &payload {
        if !path.exists() {
            return Err(format!("Исходный IPA больше не найден: {}", path.display()));
        }
    }

    let job = InstallJob {
        id: job_id.clone(),
        title: original.title.clone(),
        subtitle: original.subtitle.clone(),
        phase: "queue".into(),
        percent: 0.0,
        message: "Задача запущена повторно".into(),
        status: JobStatus::Queued,
        error: None,
        created_at: original.created_at,
        kind: original.kind.clone(),
        retryable: original.retryable,
        retry_payload: Some(payload.clone()),
    };
    let (catalog_input, local_path, sign_input, queued) = match payload {
        RetryPayload::Catalog(input) => (
            Some(input.clone()),
            None,
            None,
            QueuedInstall::Catalog {
                job_id: job_id.clone(),
                input,
            },
        ),
        RetryPayload::Export(input) => (
            Some(input.clone()),
            None,
            None,
            QueuedInstall::Export {
                job_id: job_id.clone(),
                input,
            },
        ),
        RetryPayload::Local(path) => (
            None,
            Some(path.to_string_lossy().into_owned()),
            None,
            QueuedInstall::Local {
                job_id: job_id.clone(),
                path,
            },
        ),
        RetryPayload::OpenExportsFolder => (
            None,
            None,
            None,
            QueuedInstall::OpenExportsFolder {
                job_id: job_id.clone(),
            },
        ),
        RetryPayload::SignAndInstall(input) => (
            None,
            None,
            Some(input.clone()),
            QueuedInstall::SignAndInstall {
                job_id: job_id.clone(),
                input,
            },
        ),
    };

    {
        let mut jobs = state.jobs.lock().await;
        if !jobs.get(&job_id).is_some_and(|job| matches!(job.status, JobStatus::Failed | JobStatus::Cancelled)) {
            return Err("Задача уже поставлена в очередь.".into());
        }
        jobs.insert(job_id.clone(), job.clone());
    }
    register_cancel_flag(&state, &job_id);
    remove_action_history(&app, &job_id);
    persist_pending_job(
        &app,
        PendingJobRecord {
            id: job_id.clone(),
            kind: if sign_input.is_some() {
                "sign_install".into()
            } else {
                job.kind.clone()
            },
            title: job.title,
            subtitle: job.subtitle,
            catalog_input,
            local_path,
            sign_input,
            created_at: job.created_at,
        },
    );
    state
        .queue
        .send(queued)
        .await
        .map_err(|_| "Очередь установок остановлена".to_string())?;
    Ok(job_id)
}

#[tauri::command]
async fn enqueue_local_install(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let path = PathBuf::from(path);
    if path
        .extension()
        .is_none_or(|v| !v.eq_ignore_ascii_case("ipa"))
    {
        return Err("Выберите файл с расширением .ipa".into());
    }
    if !path.exists() {
        return Err("Выбранный IPA не существует".into());
    }
    let id = Uuid::new_v4().to_string();
    let title = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("Локальный IPA")
        .to_string();
    let job = InstallJob {
        id: id.clone(),
        title,
        subtitle: path.to_string_lossy().into_owned(),
        phase: "queue".into(),
        percent: 0.0,
        message: "Добавлено в очередь".into(),
        status: JobStatus::Queued,
        error: None,
        created_at: now(),
        kind: "install".into(),
        retryable: true,
        retry_payload: Some(RetryPayload::Local(path.clone())),
    };
    state.jobs.lock().await.insert(id.clone(), job.clone());
    register_cancel_flag(&state, &id);
    persist_pending_job(
        &app,
        PendingJobRecord {
            id: id.clone(),
            kind: "install".into(),
            title: job.title.clone(),
            subtitle: job.subtitle.clone(),
            catalog_input: None,
            local_path: Some(path.to_string_lossy().into_owned()),
            sign_input: None,
            created_at: job.created_at,
        },
    );
    state
        .queue
        .send(QueuedInstall::Local {
            job_id: id.clone(),
            path,
        })
        .await
        .map_err(|_| "Очередь установок остановлена".to_string())?;
    Ok(id)
}

#[tauri::command]
fn list_action_history(app: AppHandle) -> Result<Vec<ActionHistoryEntry>, String> {
    let cache = load_cache(&app);
    let selected = selected_device_udid(&app);
    let mut items = cache.action_history;
    if let Some(udid) = selected {
        let scoped: Vec<_> = items
            .iter()
            .filter(|entry| entry.device_udid.as_deref() == Some(udid.as_str()))
            .cloned()
            .collect();
        if !scoped.is_empty() {
            items = scoped;
        }
    }
    items.sort_by_key(|entry| std::cmp::Reverse(entry.finished_at));
    Ok(items)
}

#[tauri::command]
async fn list_install_jobs(state: State<'_, AppState>) -> Result<Vec<InstallJob>, String> {
    let mut jobs: Vec<_> = state.jobs.lock().await.values().cloned().collect();
    let order = state.queue.pending.lock().await.iter().map(|item| item.id().to_string()).collect::<Vec<_>>();
    jobs.sort_by_key(|job| (
        match job.status { JobStatus::Running => 0, JobStatus::Queued => 1, _ => 2 },
        if matches!(job.status, JobStatus::Queued) { order.iter().position(|id| id == &job.id).unwrap_or(usize::MAX) } else { 0 },
        std::cmp::Reverse(job.created_at),
    ));
    Ok(jobs)
}

#[derive(Serialize)]
struct QueueStatus { paused: bool, pending: usize }

#[tauri::command]
async fn get_install_queue_status(state: State<'_, AppState>) -> Result<QueueStatus, String> {
    Ok(QueueStatus { paused: state.queue.paused.load(Ordering::Relaxed), pending: state.queue.pending.lock().await.len() })
}

#[tauri::command]
async fn set_install_queue_paused(state: State<'_, AppState>, paused: bool) -> Result<(), String> {
    let _guard = state.queue.pending.lock().await;
    state.queue.paused.store(paused, Ordering::Relaxed);
    state.queue.changed.notify_one();
    Ok(())
}

fn move_queue_item(pending: &mut VecDeque<QueuedInstall>, job_id: &str, direction: i32) -> Result<(), String> {
    if !matches!(direction, -1 | 1) { return Err("Недопустимое направление".into()); }
    let index = pending.iter().position(|item| item.id() == job_id).ok_or("Задача уже выполняется или не найдена")?;
    let next = if direction < 0 { index.saturating_sub(1) } else { (index + 1).min(pending.len() - 1) };
    pending.swap(index, next);
    Ok(())
}

#[tauri::command]
async fn move_install_job(app: AppHandle, state: State<'_, AppState>, job_id: String, direction: i32) -> Result<(), String> {
    let mut pending = state.queue.pending.lock().await;
    move_queue_item(&mut pending, &job_id, direction)?;
    let mut cache = load_cache(&app);
    cache.pending_jobs.sort_by_key(|record| pending.iter().position(|item| item.id() == record.id).unwrap_or(0));
    save_cache(&app, &cache)
}

#[tauri::command]
async fn cancel_pending_install_jobs(app: AppHandle) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let ids: Vec<_> = state.queue.pending.lock().await.drain(..).map(|item| item.id().to_string()).collect();
    let mut cancelled = 0;
    for id in ids {
        if cancel_install_job(app.clone(), app.state::<AppState>(), id.clone()).await.is_ok() { cancelled += 1; }
        clear_cancel_flag(&state, &id);
    }
    Ok(cancelled)
}

#[tauri::command]
async fn retry_failed_install_jobs(app: AppHandle) -> Result<Vec<String>, String> {
    let failed: Vec<_> = app.state::<AppState>().jobs.lock().await.values()
        .filter(|job| matches!(job.status, JobStatus::Failed) && job.retryable).map(|job| job.id.clone()).collect();
    let mut errors = Vec::new();
    for id in failed {
        if let Err(error) = retry_install_job(app.clone(), app.state::<AppState>(), id).await { errors.push(error); }
    }
    Ok(errors)
}

#[tauri::command]
async fn clear_finished_install_jobs(state: State<'_, AppState>) -> Result<usize, String> {
    let mut jobs = state.jobs.lock().await;
    let before = jobs.len();
    jobs.retain(|_, job| matches!(job.status, JobStatus::Queued | JobStatus::Running));
    Ok(before - jobs.len())
}

async fn resume_pending_jobs_on_startup(app: AppHandle, queue: Arc<InstallQueue>) {
    let cache = load_cache(&app);
    let state = app.state::<AppState>();
    for record in cache.pending_jobs {
        let job = InstallJob {
            id: record.id.clone(),
            title: record.title.clone(),
            subtitle: record.subtitle.clone(),
            phase: "queue".into(),
            percent: 0.0,
            message: "Возобновление после перезапуска…".into(),
            status: JobStatus::Queued,
            error: None,
            created_at: record.created_at,
            kind: record.kind.clone(),
            retryable: matches!(record.kind.as_str(), "install" | "export" | "sign_install"),
            retry_payload: match record.kind.as_str() {
                "export" => record.catalog_input.clone().map(RetryPayload::Export),
                "install" if record.local_path.is_some() => record
                    .local_path
                    .as_ref()
                    .map(PathBuf::from)
                    .map(RetryPayload::Local),
                "install" => record.catalog_input.clone().map(RetryPayload::Catalog),
                "open_exports" => Some(RetryPayload::OpenExportsFolder),
                "sign_install" => record.sign_input.clone().map(RetryPayload::SignAndInstall),
                _ => None,
            },
        };
        state.jobs.lock().await.insert(record.id.clone(), job);
        register_cancel_flag(&state, &record.id);
        let queued = match record.kind.as_str() {
            "export" => record.catalog_input.map(|input| QueuedInstall::Export {
                job_id: record.id.clone(),
                input,
            }),
            "open_exports" => Some(QueuedInstall::OpenExportsFolder {
                job_id: record.id.clone(),
            }),
            "install" if record.local_path.is_some() => {
                record.local_path.map(|path| QueuedInstall::Local {
                    job_id: record.id.clone(),
                    path: PathBuf::from(path),
                })
            }
            "install" => record.catalog_input.map(|input| QueuedInstall::Catalog {
                job_id: record.id.clone(),
                input,
            }),
            "sign_install" => record
                .sign_input
                .map(|input| QueuedInstall::SignAndInstall {
                    job_id: record.id.clone(),
                    input,
                }),
            _ => None,
        };
        if let Some(item) = queued {
            let _ = queue.send(item).await;
        }
    }
}

#[tauri::command]
async fn reverse_backup(
    app: AppHandle,
    bundle_id: String,
    output_dir: String,
) -> Result<String, String> {
    let tools = resolve_tool_paths(&app)?;
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    run_process(
        &tools.ideviceinstaller,
        &["archive".into(), bundle_id, format!("--copy={output_dir}")],
        Duration::from_secs(360),
        None,
    )
    .await?;
    let mut files: Vec<_> = fs::read_dir(&output_dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|v| v.eq_ignore_ascii_case("ipa")))
        .collect();
    files.sort_by_key(|p| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH)
    });
    files
        .pop()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "Архив создан без IPA-файла".into())
}

#[tauri::command]
fn get_cache_size(app: AppHandle) -> Result<u64, String> {
    let dir = PathBuf::from(load_settings(&app)?.downloads_dir);
    if !dir.exists() {
        return Ok(0);
    }
    Ok(fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_download_cache_file(path))
        .filter_map(|path| fs::metadata(&path).ok())
        .map(|meta| meta.len())
        .sum())
}

#[tauri::command]
async fn list_cached_ipas(app: AppHandle) -> Result<Vec<CachedIpa>, String> {
    let dir = PathBuf::from(load_settings(&app)?.downloads_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let device_ios = get_device_info_inner(&app)
        .await
        .ok()
        .and_then(|device| device.product_version);
    let mut items = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ipa"))
        {
            continue;
        }
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let (bundle_id, name, version, min_ios) = cached_ipa_metadata(&path);
        items.push(CachedIpa {
            filename: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            bundle_id,
            name,
            version,
            apple_id: ipa_apple_id(&path),
            size: metadata.len(),
            modified_at: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_secs())
                .unwrap_or_default(),
            device_compatible: device_supports_min_ios(device_ios.as_deref(), min_ios.as_deref()),
        });
    }
    items.sort_by_key(|item| std::cmp::Reverse(item.modified_at));
    Ok(items)
}

#[derive(Deserialize)]
struct CachedIpaInput {
    filename: String,
}

#[tauri::command]
fn delete_cached_ipa(app: AppHandle, input: CachedIpaInput) -> Result<(), String> {
    let filename = input.filename.trim();
    let name = Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| *value == filename && value.to_ascii_lowercase().ends_with(".ipa"))
        .ok_or_else(|| "Некорректное имя IPA-файла".to_string())?;
    let path = PathBuf::from(load_settings(&app)?.downloads_dir).join(name);
    if path.is_file() {
        fs::remove_file(path).map_err(|e| format!("Не удалось удалить IPA: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
fn clear_cache(app: AppHandle) -> Result<(), String> {
    let dir = PathBuf::from(load_settings(&app)?.downloads_dir);
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_file() && is_download_cache_file(&path) {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn compare_versions(left: &str, right: &str) -> i8 {
    let parts = |value: &str| -> Vec<u64> {
        value
            .split(|c: char| !c.is_ascii_digit())
            .filter(|v| !v.is_empty())
            .map(|v| v.parse().unwrap_or(0))
            .collect()
    };
    let (mut a, mut b) = (parts(left), parts(right));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a.cmp(&b) as i8
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<OtaUpdateInfo, String> {
    let value = api_get(&app, "ota.php", &[]).await?;
    Ok(parse_ota(&value))
}

async fn smart_auto_updater(app: AppHandle) {
    if DEMO_MODE {
        return;
    }
    let mut notified: HashSet<(String, String)> = HashSet::new();
    tokio::time::sleep(Duration::from_secs(SMART_UPDATE_INITIAL_DELAY_SECS)).await;
    loop {
        if let Err(error) = smart_auto_update_once(&app, &mut notified).await {
            let _ = app.emit(
                "background-status",
                serde_json::json!({ "source": "smart-updater", "error": error }),
            );
        }
        tokio::time::sleep(Duration::from_secs(SMART_UPDATE_INTERVAL_SECS)).await;
    }
}

/// Refresh one Apple ID's RAP history session. Fully isolated by email key —
/// never reads/writes another account's cookies or apps cache.
async fn refresh_one_history_session(email: &str) {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() {
        return;
    }
    let Some(cookies) = history_secrets::load_history_session(&email) else {
        return;
    };
    // Soft fail: keep previous cookies (do not wipe — that would force 2FA).
    let Some(fresh) = apple_history::refresh_history_session(&cookies).await else {
        return;
    };
    let _ = history_secrets::store_history_session(&email, &fresh.cookies);
    // Only this account's apps-cache tokens.
    if let Some(mut cache) = history_secrets::load_history_apps_cache(&email) {
        if let Some(d) = fresh.dsid.filter(|s| !s.trim().is_empty()) {
            cache.rap_dsid = Some(d);
        }
        if let Some(x) = fresh.xsrf.filter(|s| !s.trim().is_empty()) {
            cache.rap_xsrf = Some(x);
        }
        let _ = history_secrets::store_history_apps_cache(&cache);
    }
}

/// Periodically touch reportaproblem cookies for **every** saved Apple ID.
/// Sessions are keyed by email and never mixed (no shared jar across accounts).
async fn history_session_keepalive(app: AppHandle) {
    tokio::time::sleep(Duration::from_secs(HISTORY_SESSION_KEEPALIVE_INITIAL_SECS)).await;
    loop {
        let keep = load_settings(&app)
            .map(|s| s.keep_history_session)
            .unwrap_or(true);
        if keep {
            // Snapshot account list so we don't hold cache locks during network I/O.
            let emails: Vec<String> = {
                let cache = load_cache(&app);
                let mut list: Vec<String> = cache
                    .apple_accounts
                    .iter()
                    .map(|a| a.email.trim().to_ascii_lowercase())
                    .filter(|e| !e.is_empty())
                    .collect();
                // Include active even if missing from list (legacy caches).
                if let Some(active) = cache
                    .active_account_email
                    .as_ref()
                    .map(|e| e.trim().to_ascii_lowercase())
                    .filter(|e| !e.is_empty())
                {
                    if !list.iter().any(|e| e == &active) {
                        list.push(active);
                    }
                }
                list.sort();
                list.dedup();
                list
            };
            for email in emails {
                refresh_one_history_session(&email).await;
                // Small gap so Apple isn't hit by parallel multi-account bursts.
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
        tokio::time::sleep(Duration::from_secs(HISTORY_SESSION_KEEPALIVE_SECS)).await;
    }
}

fn remember_auto_purchased_store_id(app: &AppHandle, store_id: &str) {
    let store_id = store_id.trim();
    if store_id.is_empty() {
        return;
    }
    let mut cache = load_cache(app);
    if !cache
        .auto_purchased_store_ids
        .iter()
        .any(|id| id == store_id)
    {
        cache.auto_purchased_store_ids.push(store_id.to_string());
        cache.auto_purchased_store_ids.sort();
        cache.auto_purchased_store_ids.dedup();
        let _ = save_cache(app, &cache);
    }
}

fn remember_auto_purchase_fingerprint(app: &AppHandle, fingerprint: &str) {
    let mut cache = load_cache(app);
    cache.auto_purchase_fingerprint = Some(fingerprint.to_string());
    let _ = save_cache(app, &cache);
}

/// Purchase one App Store build onto Apple ID (no iPhone required).
async fn background_auto_purchase_store_item(
    app: &AppHandle,
    item: &JsonValue,
) -> Result<(), String> {
    if stored_auth_email(app).is_none() {
        return Err("Нужен вход в Apple ID для автопокупки".into());
    }
    let app_store_id = item
        .get("app_store_id")
        .or_else(|| item.get("key"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty() && *v != "NONE" && v.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .ok_or_else(|| "Нет App Store ID для покупки".to_string())?;
    let bundle_id = item
        .get("bundle_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty() && *v != "NONE")
        .map(str::to_string)
        .unwrap_or_else(|| app_store_id.clone());
    let title = item
        .get("title")
        .or_else(|| item.get("app_title"))
        .and_then(|v| v.as_str())
        .unwrap_or("Приложение")
        .to_string();
    let app_id = item
        .get("app_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let state = app.state::<AppState>();
    let _guard = state.ipatool_lock.lock().await;
    ensure_active_ipatool_session(app).await?;

    let input = CatalogInstallInput {
        title: title.clone(),
        bundle_id,
        app_store_id: Some(app_store_id.clone()),
        in_store: true,
        min_ios: item
            .get("min_ios")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        version: item
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        external_version_id: None,
        already_owned: false,
        temporary_us_region: false,
        temporary_region: None,
    };
    let tools = resolve_tool_paths(app)?;
    let keychain = resolve_ipatool_keychain(app)?;
    let _ = purchase_store_app(app, &tools, &keychain, &input).await?;
    remember_auto_purchased_store_id(app, &app_store_id);
    let _ = app.emit(
        "auto-purchase-done",
        serde_json::json!({
            "app_id": app_id,
            "app_store_id": app_store_id,
            "title": title,
            "ok": true,
            "message": "Приложение банков/сервисов добавлено в покупки Apple ID. Можно установить позже.",
        }),
    );
    Ok(())
}

/// Poll API check.php?type=auto_purchase for new App Store bank/service apps.
async fn run_auto_purchase_watch(app: &AppHandle) -> Result<(), String> {
    let auto_on = load_settings(app)
        .map(|s| s.auto_purchase_new_apps)
        .unwrap_or(false);
    if !auto_on {
        return Ok(());
    }

    let cache = load_cache(app);
    let known_fp = cache.auto_purchase_fingerprint.clone().unwrap_or_default();
    let already: HashSet<String> = cache
        .auto_purchased_store_ids
        .iter()
        .cloned()
        .chain(cache.auto_purchased_app_ids.iter().cloned())
        .collect();

    let mut query: Vec<(&str, &str)> = vec![("type", "auto_purchase")];
    if !known_fp.is_empty() {
        query.push(("known", known_fp.as_str()));
    }
    let value = api_get(app, "check.php", &query).await?;
    if !json_bool(value.get("enabled"), true) {
        return Ok(());
    }
    let fingerprint = value
        .get("fingerprint")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // First snapshot: remember fingerprint, do NOT buy the whole catalog.
    if known_fp.is_empty() {
        if !fingerprint.is_empty() {
            remember_auto_purchase_fingerprint(app, &fingerprint);
        }
        // Seed purchased keys so we only buy *new* store ids later.
        for item in &items {
            if let Some(key) = item
                .get("key")
                .or_else(|| item.get("app_store_id"))
                .and_then(|v| v.as_str())
            {
                remember_auto_purchased_store_id(app, key);
            }
        }
        return Ok(());
    }

    // Diff by App Store id — only purchase items not seen before.
    for item in &items {
        let key = item
            .get("key")
            .or_else(|| item.get("app_store_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if key.is_empty() || already.contains(&key) {
            continue;
        }
        match background_auto_purchase_store_item(app, item).await {
            Ok(()) => {}
            Err(error) => {
                let title = item
                    .get("title")
                    .or_else(|| item.get("app_title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Приложение");
                let _ = app.emit(
                    "auto-purchase-done",
                    serde_json::json!({
                        "app_store_id": key,
                        "title": title,
                        "ok": false,
                        "message": error,
                    }),
                );
            }
        }
    }

    if !fingerprint.is_empty() {
        remember_auto_purchase_fingerprint(app, &fingerprint);
    }
    Ok(())
}

async fn smart_auto_update_once(
    app: &AppHandle,
    notified: &mut HashSet<(String, String)>,
) -> Result<(), String> {
    renew_expiring_signatures(app).await;

    // API-driven auto-purchase (check.php?type=auto_purchase).
    if let Err(error) = run_auto_purchase_watch(app).await {
        let _ = app.emit(
            "background-status",
            serde_json::json!({ "source": "auto-purchase", "error": error }),
        );
    }

    for update in collect_app_updates(app).await? {
        let key = (update.bundle_id.clone(), update.remote_version.clone());
        if !notified.insert(key) {
            continue;
        }
        let title = update.title.clone();
        let version = update.remote_version.clone();
        let _ = app.emit("app-update-found", update);
        if app.state::<AppState>().background_mode.load(Ordering::Relaxed) || window_is_hidden(app) {
            show_signum_notification(app, "Доступно обновление", &format!("{title} · версия {version}"));
        }
    }
    Ok(())
}

/// Every catalog app the iPhone has in an older version than the catalog offers.
///
/// Restore-only rows are skipped on purpose: their «versions» are different
/// apps sharing one catalog entry, so a newer row is not an update of what is
/// installed — offering it would replace the app with an unrelated one.
async fn collect_app_updates(app: &AppHandle) -> Result<Vec<AppUpdateFoundEvent>, String> {
    let mut updates = Vec::new();
    let catalog = fetch_catalog(app.clone()).await?;
    let cache = load_cache(app);
    let known: HashSet<String> = cache.known_app_ids.iter().cloned().collect();
    let current_ids: HashSet<String> = catalog.iter().filter_map(catalog_item_id).collect();

    if known.is_empty() {
        cache_remember_app_ids(app, current_ids.clone());
    } else if known != current_ids {
        cache_remember_app_ids(app, current_ids.clone());
    }

    let installed = match list_installed_apps_inner(app).await {
        Ok(apps) => apps,
        Err(_) => return Ok(updates), // no device — purchase path handled by the caller
    };
    let installed_map: HashMap<String, InstalledApp> = installed
        .into_iter()
        .filter(|app| !app.is_placeholder)
        .map(|app| (app.bundle_id.clone(), app))
        .collect();

    // The update watch runs on a timer, so a per-app info.php call meant the
    // whole catalogue's worth of requests firing in the background — competing
    // for the connection with whatever the user was doing. One request instead.
    let scan_ids: Vec<String> = catalog.iter().filter_map(catalog_item_id).collect();
    let scanned = fetch_catalog_versions_batch(app, &scan_ids).await?;

    for item in catalog {
        let app_id =
            catalog_item_id(&item).ok_or_else(|| "Каталог вернул приложение без id".to_string())?;
        // Absent means the catalogue lists no installable version, which the
        // in_store check below would have rejected anyway.
        let Some((info, _)) = scanned.get(&app_id) else {
            continue;
        };
        let info = info.clone();
        let latest_in_store =
            latest_version_bool(&info, "in_store", json_bool(info.get("in_store"), false));
        if !latest_in_store {
            continue;
        }
        let bundle_id = latest_version_string(&info, "bundle_id");
        let remote_version = info
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if bundle_id.is_empty() || !looks_like_version(&remote_version) {
            continue;
        }
        let Some(installed_app) = installed_map.get(&bundle_id) else {
            continue;
        };
        let installed_version = installed_app
            .version
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string();
        if !looks_like_version(&installed_version) {
            continue;
        }
        if compare_versions(&remote_version, &installed_version) <= 0 {
            continue;
        }
        updates.push(AppUpdateFoundEvent {
            app_id,
            title: info
                .get("title")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| catalog_item_title(&item)),
            bundle_id,
            installed_version,
            remote_version,
            in_store: latest_version_bool(&info, "in_store", json_bool(info.get("in_store"), false)),
            app_store_id: {
                let from_version = latest_version_string(&info, "app_store_id");
                if from_version.is_empty() {
                    None
                } else {
                    Some(from_version)
                }
            },
            min_ios: {
                let from_version = latest_version_string(&info, "min_ios");
                if from_version.is_empty() {
                    None
                } else {
                    Some(from_version)
                }
            },
        });
    }
    Ok(updates)
}

/// Pin or unpin a catalog app. Read-modify-write lives here so two windows
/// cannot race the whole settings file over one toggle.
#[tauri::command]
fn toggle_favorite_app(app: AppHandle, app_id: String) -> Result<Vec<String>, String> {
    let mut settings = load_settings(&app)?;
    let app_id = app_id.trim().to_string();
    if app_id.is_empty() {
        return Ok(settings.favorite_app_ids);
    }
    match settings
        .favorite_app_ids
        .iter()
        .position(|item| item == &app_id)
    {
        Some(at) => {
            settings.favorite_app_ids.remove(at);
        }
        None => settings.favorite_app_ids.push(app_id),
    }
    let favorites = settings.favorite_app_ids.clone();
    save_settings(app, settings)?;
    Ok(favorites)
}

/// Everything the catalog says is newer than what the iPhone has.
#[tauri::command]
async fn list_app_updates(app: AppHandle) -> Result<Vec<AppUpdateFoundEvent>, String> {
    require_auth(&app)?;
    collect_app_updates(&app).await
}

/// Ceiling on one bulk request. Each app still costs a full ownership probe,
/// so this is about keeping one click from turning into an hour of work.
const MAX_BULK_INSTALL: usize = 25;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogInstallManyInput {
    app_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogInstallSkipped {
    app_id: String,
    reason: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogInstallManyResult {
    job_ids: Vec<String>,
    skipped: Vec<CatalogInstallSkipped>,
}

/// Queue several catalog apps at once.
///
/// Each one is resolved exactly the way opening its card would, so an app with
/// no confirmed version is reported back instead of being queued to fail later.
#[tauri::command]
async fn enqueue_catalog_installs(
    app: AppHandle,
    input: CatalogInstallManyInput,
) -> Result<CatalogInstallManyResult, String> {
    require_auth(&app)?;
    let mut job_ids = Vec::new();
    let mut skipped = Vec::new();
    for app_id in input.app_ids.iter().take(MAX_BULK_INSTALL) {
        let app_id = app_id.trim().to_string();
        if app_id.is_empty() {
            continue;
        }
        let offer = match resolve_catalog_install(
            app.clone(),
            app.state::<AppState>(),
            ResolveCatalogInput {
                app_id: app_id.clone(),
                restore_only: false,
            },
        )
        .await
        {
            Ok(offer) => offer,
            Err(reason) => {
                skipped.push(CatalogInstallSkipped { app_id, reason });
                continue;
            }
        };
        if offer.source != "restore" {
            skipped.push(CatalogInstallSkipped {
                app_id,
                reason: "Нет подтверждённой версии в покупках этого Apple ID".into(),
            });
            continue;
        }
        let queued = enqueue_catalog_install_inner(
            app.clone(),
            app.state::<AppState>().inner(),
            CatalogInstallInput {
                title: offer.title.clone(),
                bundle_id: offer.bundle_id.clone(),
                app_store_id: offer.app_store_id.clone(),
                in_store: offer.in_store,
                min_ios: offer.min_ios.clone(),
                version: Some(offer.version.clone()),
                external_version_id: None,
                already_owned: true,
                temporary_us_region: false,
                temporary_region: None,
            },
        )
        .await;
        match queued {
            Ok(id) => job_ids.push(id),
            Err(reason) => skipped.push(CatalogInstallSkipped { app_id, reason }),
        }
    }
    Ok(CatalogInstallManyResult { job_ids, skipped })
}

async fn startup_ota_check(app: AppHandle) {
    // Min-client check ASAP — do not wait for auto_update toggle.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let update = match check_for_update(app.clone()).await {
        Ok(value) => value,
        Err(_) => return,
    };
    if !update.client_supported {
        emit_client_outdated(
            &app,
            &update,
            update
                .notes
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Приложение недоступно. Эта версия Signum устарела."),
        );
        return;
    }
    let settings = match load_settings(&app) {
        Ok(value) => value,
        Err(_) => return,
    };
    if !settings.auto_update || !update.has_update {
        return;
    }
    let _ = app.emit(
        "ota-available",
        serde_json::json!({
            "update": update,
            "auto_install": true
        }),
    );
}

#[derive(Debug, Clone, Serialize)]
struct IcloudEnsureResult {
    /// ready | not_needed | installed | failed
    status: String,
    installed: bool,
    message: Option<String>,
}

fn emit_icloud_progress(app: &AppHandle, phase: &str, percent: f64, message: &str) {
    let _ = app.emit(
        "icloud-setup-progress",
        serde_json::json!({
            "phase": phase,
            "percent": percent,
            "message": message,
        }),
    );
}

#[cfg(target_os = "windows")]
fn windows_program_files_paths() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for key in [
        "ProgramFiles(x86)",
        "ProgramFiles",
        "PROGRAMFILES(X86)",
        "PROGRAMFILES",
    ] {
        if let Ok(value) = std::env::var(key) {
            let path = PathBuf::from(value);
            if path.is_dir() && !dirs.iter().any(|existing: &PathBuf| existing == &path) {
                dirs.push(path);
            }
        }
    }
    // Fallbacks for restricted environments without env vars.
    for fallback in [r"C:\Program Files (x86)", r"C:\Program Files"] {
        let path = PathBuf::from(fallback);
        if path.is_dir() && !dirs.iter().any(|existing: &PathBuf| existing == &path) {
            dirs.push(path);
        }
    }
    dirs
}

#[cfg(target_os = "windows")]
fn windows_icloud_installed() -> bool {
    // Real desktop iCloud only (Internet Services). APSDaemon from iTunes alone is NOT enough
    // for anisette — counting it caused false "iCloud ready" while login still failed.
    for root in windows_program_files_paths() {
        for relative in [
            r"Common Files\Apple\Internet Services\iCloudServices.exe",
            r"Common Files\Apple\Internet Services\iCloud.exe",
            r"Common Files\Apple\Internet Services\iCloudServices_main.dll",
            r"Common Files\Apple\Internet Services\iCloudPhotoLibrary.exe",
        ] {
            if root.join(relative).is_file() {
                return true;
            }
        }
    }

    // InstallDir from Apple registry → check real files exist (avoids "installed" false positive).
    for hive in [
        r"HKLM\SOFTWARE\Apple Inc.\Internet Services",
        r"HKLM\SOFTWARE\WOW6432Node\Apple Inc.\Internet Services",
    ] {
        let output = StdCommand::new("reg")
            .args(["query", hive, "/s"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let lower = line.to_ascii_lowercase();
                if !(lower.contains("installdir") || lower.contains("install path")) {
                    continue;
                }
                // REG_SZ value is last token(s); take everything after REG_SZ.
                if let Some(idx) = lower.find("reg_sz") {
                    let path = line[idx + 6..].trim();
                    if !path.is_empty() {
                        let p = PathBuf::from(path);
                        if p.join("iCloudServices.exe").is_file()
                            || p.join("iCloud.exe").is_file()
                            || p.is_dir()
                                && p.read_dir()
                                    .map(|mut d| d.any(|e| e.is_ok()))
                                    .unwrap_or(false)
                        {
                            // Only accept if at least one known binary is present.
                            if p.join("iCloudServices.exe").is_file()
                                || p.join("iCloud.exe").is_file()
                                || p.join("iCloudServices_main.dll").is_file()
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    // Uninstall display name — only if DisplayIcon / InstallLocation points to existing file.
    for hive in [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        let output = StdCommand::new("reg")
            .args(["query", hive, "/s", "/f", "iCloud", "/t", "REG_SZ"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let lower = line.to_ascii_lowercase();
                if !(lower.contains("displayicon") || lower.contains("installlocation")) {
                    continue;
                }
                if let Some(idx) = lower.find("reg_sz") {
                    let raw = line[idx + 6..].trim().trim_matches('"');
                    // DisplayIcon may be "C:\...\iCloud.exe,0"
                    let path = raw.split(',').next().unwrap_or(raw).trim();
                    if !path.is_empty() && Path::new(path).is_file() {
                        return true;
                    }
                    if !path.is_empty() {
                        let dir = Path::new(path);
                        if dir.join("iCloudServices.exe").is_file()
                            || dir.join("iCloud.exe").is_file()
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

#[cfg(not(target_os = "windows"))]
fn windows_icloud_installed() -> bool {
    false
}

/// Mirror base for offline installers (never shown in UI).
///
/// Файлы лежат в подпапке `files/`, а адрес собирался без неё — и зеркало
/// отвечало 404 на каждый запрос. Замечено при проверке живого сервера:
/// `/signum/iCloudSetup.exe` → 404, `/signum/files/iCloudSetup.exe` → 206.
/// Значит запасной источник для iCloud, iTunes и компонентов Windows не
/// работал вовсе, и при недоступности основного адреса установка срывалась.
#[cfg(target_os = "windows")]
fn signum_mirror_url(file_name: &str) -> String {
    format!(
        "{}/files/{}",
        DEFAULT_API.trim_end_matches('/'),
        file_name.trim_start_matches('/')
    )
}

/// OS architecture for MFP package selection (x64 / x32).
#[cfg(target_os = "windows")]
fn windows_mfp_arch_name() -> &'static str {
    // 32-bit process on 64-bit Windows
    if std::env::var("PROCESSOR_ARCHITEW6432")
        .map(|v| v.eq_ignore_ascii_case("AMD64") || v.eq_ignore_ascii_case("ARM64"))
        .unwrap_or(false)
    {
        return "x64";
    }
    match std::env::var("PROCESSOR_ARCHITECTURE")
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "AMD64" | "ARM64" => "x64",
        "X86" => "x32",
        _ if cfg!(target_arch = "x86_64") || cfg!(target_arch = "aarch64") => "x64",
        _ => "x32",
    }
}

/// Primary (Apple) is abandoned if connect/TTFB is slow or sustained speed is tiny.
#[cfg(target_os = "windows")]
const DOWNLOAD_PRIMARY_CONNECT_SECS: u64 = 12;
#[cfg(target_os = "windows")]
const DOWNLOAD_PRIMARY_TTFB_SECS: u64 = 15;
/// After this many seconds of body download, measure average speed.
#[cfg(target_os = "windows")]
const DOWNLOAD_SPEED_CHECK_AFTER_SECS: u64 = 8;
/// Below this average B/s (after the window) → switch to next source (~100 KB/s).
#[cfg(target_os = "windows")]
const DOWNLOAD_MIN_BYTES_PER_SEC: u64 = 100 * 1024;
/// If fewer bytes arrived in the check window, treat as stall even if B/s math is noisy.
#[cfg(target_os = "windows")]
const DOWNLOAD_MIN_BYTES_IN_WINDOW: u64 = 256 * 1024;
#[cfg(target_os = "windows")]
const DOWNLOAD_MIRROR_CONNECT_SECS: u64 = 30;
#[cfg(target_os = "windows")]
const DOWNLOAD_MIRROR_TTFB_SECS: u64 = 60;

#[cfg(target_os = "windows")]
#[derive(Debug)]
enum DownloadAttemptError {
    /// Network/slow — try next URL silently.
    SwitchSource,
    /// User-facing message (no host/url).
    Fail(String),
}

/// Build HTTP client: plain WebPKI everywhere (no pinning).
#[cfg(target_os = "windows")]
fn download_http_client(url: &str, connect_secs: u64) -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(900))
        .connect_timeout(Duration::from_secs(connect_secs))
        .redirect(reqwest::redirect::Policy::limited(10));
    if api_download_urls(url).len() > 1 {
        builder
            .https_only(true)
            .build()
            .map_err(|e| format!("HTTP client: {e}"))
    } else {
        builder.build().map_err(|e| format!("HTTP client: {e}"))
    }
}

/// Download a file. Tries URLs in order (Apple primary → mirror).
/// Switches early if primary is slow to answer or transfer speed is very low.
/// Never exposes URL/host in user-facing messages.
#[cfg(target_os = "windows")]
async fn download_setup_with_fallback(
    app: &AppHandle,
    urls: &[&str],
    target: &Path,
    label: &str,
    min_bytes: u64,
    progress: Option<(f64, f64)>,
) -> Result<(), String> {
    if urls.is_empty() {
        return Err(format!("Не удалось скачать {label}. Повторите попытку."));
    }
    if let Some((from, _)) = progress {
        emit_icloud_progress(app, "downloading", from, &format!("Скачиваем {label}…"));
    }

    let mut last_fail = format!("Не удалось скачать {label}. Проверьте интернет и повторите.");
    let mut sources = Vec::new();
    for url in urls {
        for source in api_download_urls(url) {
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
    }
    let total_urls = sources.len();
    for (index, url) in sources.iter().enumerate() {
        let is_last = index + 1 >= total_urls;
        // Speed-based switch only when a fallback remains.
        let allow_switch = !is_last;
        let connect_secs = if index == 0 && allow_switch {
            DOWNLOAD_PRIMARY_CONNECT_SECS
        } else {
            DOWNLOAD_MIRROR_CONNECT_SECS
        };
        let ttfb_secs = if index == 0 && allow_switch {
            DOWNLOAD_PRIMARY_TTFB_SECS
        } else {
            DOWNLOAD_MIRROR_TTFB_SECS
        };

        let _ = tokio::fs::remove_file(target).await;
        match download_setup_once(
            app,
            url,
            target,
            label,
            min_bytes,
            progress,
            connect_secs,
            ttfb_secs,
            allow_switch,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(DownloadAttemptError::SwitchSource) => {
                // Silent switch — user only still sees "Скачиваем …"
                emit_icloud_progress(
                    app,
                    "downloading",
                    progress.map(|(f, _)| f + 1.0).unwrap_or(0.0),
                    &format!("Скачиваем {label}…"),
                );
                continue;
            }
            Err(DownloadAttemptError::Fail(msg)) => {
                if is_last {
                    return Err(msg);
                }
                last_fail = msg;
                continue;
            }
        }
    }
    Err(last_fail)
}

#[cfg(target_os = "windows")]
async fn download_setup_once(
    app: &AppHandle,
    url: &str,
    target: &Path,
    label: &str,
    min_bytes: u64,
    progress: Option<(f64, f64)>,
    connect_secs: u64,
    ttfb_secs: u64,
    allow_switch_if_slow: bool,
) -> Result<(), DownloadAttemptError> {
    let client = download_http_client(url, connect_secs).map_err(|_| {
        DownloadAttemptError::Fail(format!(
            "Не удалось скачать {label}. Проверьте интернет и повторите."
        ))
    })?;

    let send_fut = client.get(url).send();

    let response = match tokio::time::timeout(Duration::from_secs(ttfb_secs), send_fut).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(_)) => {
            return if allow_switch_if_slow {
                Err(DownloadAttemptError::SwitchSource)
            } else {
                Err(DownloadAttemptError::Fail(format!(
                    "Не удалось скачать {label}. Проверьте интернет и повторите."
                )))
            };
        }
        Err(_) => {
            // Slow to answer / TTFB timeout
            return if allow_switch_if_slow {
                Err(DownloadAttemptError::SwitchSource)
            } else {
                Err(DownloadAttemptError::Fail(format!(
                    "Не удалось скачать {label}. Сервис не ответил вовремя."
                )))
            };
        }
    };

    if !response.status().is_success() {
        return if allow_switch_if_slow {
            Err(DownloadAttemptError::SwitchSource)
        } else {
            Err(DownloadAttemptError::Fail(format!(
                "Не удалось скачать {label}. Повторите попытку чуть позже."
            )))
        };
    }

    let total = response.content_length().unwrap_or(0);
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|_| {
            DownloadAttemptError::Fail(format!("Не удалось подготовить файл {label}."))
        })?;
    }
    let mut file = tokio::fs::File::create(target)
        .await
        .map_err(|_| DownloadAttemptError::Fail(format!("Не удалось сохранить {label}.")))?;
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    use tokio::io::AsyncWriteExt;
    let body_started = std::time::Instant::now();
    let mut speed_checked = false;
    let mut last_emit = std::time::Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(std::time::Instant::now);

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(_) => {
                let _ = file.flush().await;
                let _ = tokio::fs::remove_file(target).await;
                return if allow_switch_if_slow && downloaded < min_bytes {
                    Err(DownloadAttemptError::SwitchSource)
                } else {
                    Err(DownloadAttemptError::Fail(format!(
                        "Загрузка {label} прервалась. Проверьте интернет и повторите."
                    )))
                };
            }
        };
        file.write_all(&chunk)
            .await
            .map_err(|_| DownloadAttemptError::Fail(format!("Ошибка записи {label}.")))?;
        downloaded += chunk.len() as u64;

        // Abandon slow primary mid-transfer when fallback exists.
        if allow_switch_if_slow && !speed_checked {
            let elapsed = body_started.elapsed();
            if elapsed >= Duration::from_secs(DOWNLOAD_SPEED_CHECK_AFTER_SECS) {
                speed_checked = true;
                let secs = elapsed.as_secs_f64().max(0.001);
                let bps = (downloaded as f64 / secs) as u64;
                let too_slow =
                    downloaded < DOWNLOAD_MIN_BYTES_IN_WINDOW || bps < DOWNLOAD_MIN_BYTES_PER_SEC;
                // Only switch if we are still far from done (don't drop near-complete files).
                let far_from_done = total == 0 || downloaded < total.saturating_mul(3) / 4;
                if too_slow && far_from_done {
                    let _ = file.flush().await;
                    drop(file);
                    let _ = tokio::fs::remove_file(target).await;
                    return Err(DownloadAttemptError::SwitchSource);
                }
            }
        }

        if let Some((from, to)) = progress {
            if last_emit.elapsed() >= Duration::from_millis(200) {
                last_emit = std::time::Instant::now();
                let fraction = if total > 0 {
                    downloaded as f64 / total as f64
                } else {
                    0.05
                };
                let span = (to - from).max(1.0);
                let percent = from + span * fraction.min(1.0);
                emit_icloud_progress(
                    app,
                    "downloading",
                    percent,
                    &format!(
                        "Скачиваем {label}… {:.0}%",
                        if total > 0 { fraction * 100.0 } else { 0.0 }
                    ),
                );
            }
        }
    }

    file.flush()
        .await
        .map_err(|_| DownloadAttemptError::Fail(format!("Не удалось сохранить {label}.")))?;
    drop(file);
    let meta = tokio::fs::metadata(target)
        .await
        .map_err(|_| DownloadAttemptError::Fail(format!("Не удалось проверить {label}.")))?;
    if meta.len() < min_bytes {
        let _ = tokio::fs::remove_file(target).await;
        return if allow_switch_if_slow {
            Err(DownloadAttemptError::SwitchSource)
        } else {
            Err(DownloadAttemptError::Fail(format!(
                "Файл {label} повреждён или неполный. Повторите загрузку."
            )))
        };
    }
    Ok(())
}

/// True when both desktop iCloud and iTunes are present on disk.
/// Microsoft Store iCloud alone is NOT enough (anisette returns "iCloud Not Found").
#[cfg(target_os = "windows")]
fn windows_apple_stack_installed() -> bool {
    // Для iPhone по USB нужна служба Apple Mobile Device. Её ставит наш
    // драйвер, iTunes (.exe или из Microsoft Store) и «Устройства Apple».
    let device_support = windows_amds_installed() || usbmuxd_reachable();
    device_support && (!SIGNUM_NEEDS_ICLOUD || windows_icloud_installed())
}

/// Служба Apple Mobile Device Support на диске — своя или из iTunes.
#[cfg(target_os = "windows")]
fn windows_amds_installed() -> bool {
    windows_program_files_paths().iter().any(|root| {
        root.join(r"Common Files\Apple\Mobile Device Support\AppleMobileDeviceService.exe")
            .is_file()
    })
}

/// Откуда брать то, что даёт Windows связь с iPhone. `driver_only` — один
/// Apple Mobile Device Support (его Apple отдельно не раздаёт, он только на
/// нашем зеркале), иначе iTunes целиком.
#[cfg(target_os = "windows")]
fn device_support_sources(driver_only: bool) -> Vec<String> {
    if driver_only {
        vec![signum_mirror_url(AMDS_MIRROR_NAME)]
    } else {
        let (primary, mirror_name) = itunes_setup_sources();
        vec![primary.to_string(), signum_mirror_url(mirror_name)]
    }
}

#[cfg(target_os = "windows")]
fn device_support_installed(driver_only: bool) -> bool {
    if driver_only {
        windows_amds_installed()
    } else {
        windows_itunes_installed()
    }
}

/// Windows 7/8/8.1: свежий iTunes на них не ставится.
#[cfg(target_os = "windows")]
fn windows_before_10() -> bool {
    StdCommand::new("cmd")
        .args(["/C", "ver"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()
        .and_then(|out| windows_major_from_ver(&String::from_utf8_lossy(&out.stdout)))
        .is_some_and(|major| major < 10)
}

/// "Microsoft Windows [Version 6.3.9600]" → 6. Слово «Version» бывает
/// переведено, поэтому берём последнее слово в скобках.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn windows_major_from_ver(text: &str) -> Option<u32> {
    let inside = text.split('[').nth(1)?.split(']').next()?;
    inside.split_whitespace().last()?.split('.').next()?.parse().ok()
}

/// Установщик iTunes под эту Windows: (адрес Apple, имя на нашем зеркале).
#[cfg(target_os = "windows")]
fn itunes_setup_sources() -> (&'static str, &'static str) {
    if windows_before_10() {
        (ITUNES_LEGACY_SETUP_URL, ITUNES_LEGACY_MIRROR_NAME)
    } else {
        (ITUNES_SETUP_URL_PRIMARY, ITUNES_SETUP_MIRROR_NAME)
    }
}

/// Microsoft Store package `AppleInc.iCloud` — cannot provide ADI/anisette for login.
#[cfg(target_os = "windows")]
fn windows_icloud_msstore_installed() -> bool {
    let output = StdCommand::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "if (Get-AppxPackage -Name 'AppleInc.iCloud' -ErrorAction SilentlyContinue) { '1' } else { '0' }",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .output();
    output
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|s| s.contains('1'))
}

/// Remove Store iCloud so classic desktop installer can provide Internet Services.
#[cfg(target_os = "windows")]
fn remove_windows_icloud_msstore() -> bool {
    let output = StdCommand::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "Get-AppxPackage -Name 'AppleInc.iCloud' -ErrorAction SilentlyContinue | Remove-AppxPackage -ErrorAction SilentlyContinue; if (Get-AppxPackage -Name 'AppleInc.iCloud' -ErrorAction SilentlyContinue) { '0' } else { '1' }",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .output();
    output
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|s| s.contains('1'))
}

/// Fully uninstall desktop iCloud and/or iTunes via Windows Uninstall registry.
/// Elevated. Exit codes: 0 ok / nothing to do, 1223 UAC cancel, else error.
/// product: "icloud" | "itunes" | "both"
#[cfg(target_os = "windows")]
async fn uninstall_windows_apple_desktop_products(product: &str) -> Result<i32, String> {
    let script_body = r#"
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$target = $env:SIGNUM_UNINSTALL_TARGET
if ([string]::IsNullOrWhiteSpace($target)) { $target = 'both' }
$target = $target.ToLowerInvariant()

function Match-Product([string]$name) {
  if ([string]::IsNullOrWhiteSpace($name)) { return $false }
  if ($target -eq 'icloud' -or $target -eq 'both') {
    if ($name -match '(?i)^iCloud(\s|$|for)') { return $true }
  }
  if ($target -eq 'itunes' -or $target -eq 'both') {
    if ($name -match '(?i)^iTunes(\s|$)') { return $true }
  }
  if ($target -eq 'amds') {
    if ($name -match '(?i)^Apple Mobile Device Support') { return $true }
  }
  return $false
}

function Run-Uninstall([string]$cmd) {
  if ([string]::IsNullOrWhiteSpace($cmd)) { return }
  $c = $cmd.Trim()
  # msiexec /I{GUID} or /X{GUID} or /x {GUID}
  if ($c -match '(?i)msiexec(\.exe)?\s+') {
    $guid = $null
    if ($c -match '\{[0-9A-Fa-f-]{36}\}') { $guid = $Matches[0] }
    if ($guid) {
      $p = Start-Process -FilePath "$env:SystemRoot\System32\msiexec.exe" `
        -ArgumentList @('/x', $guid, '/qn', '/norestart') `
        -Wait -PassThru -WindowStyle Hidden
      return
    }
  }
  # QuietUninstallString or bare UninstallString
  try {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    if ($c.StartsWith('"')) {
      $end = $c.IndexOf('"', 1)
      if ($end -gt 1) {
        $psi.FileName = $c.Substring(1, $end - 1)
        $psi.Arguments = $c.Substring($end + 1).Trim()
      } else {
        $psi.FileName = $c.Trim('"')
      }
    } else {
      $parts = $c -split '\s+', 2
      $psi.FileName = $parts[0]
      if ($parts.Count -gt 1) { $psi.Arguments = $parts[1] }
    }
    # Prefer silent if msiexec without /qn
    if ($psi.FileName -match '(?i)msiexec' -and $psi.Arguments -notmatch '(?i)/qn|/quiet') {
      if ($psi.Arguments -notmatch '(?i)/x') {
        # leave as-is
      }
      if ($psi.Arguments -notmatch '(?i)/qn|/quiet') {
        $psi.Arguments = ($psi.Arguments + ' /qn /norestart').Trim()
      }
    } elseif ($psi.Arguments -notmatch '(?i)/quiet|/qn|/s\b') {
      # many Apple uninstallers accept /qn when wrapped via msiexec only
    }
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    if ($null -ne $p) { $p.WaitForExit(600000) | Out-Null }
  } catch {
    try {
      $p = Start-Process -FilePath 'cmd.exe' -ArgumentList @('/c', $c) -Wait -PassThru -WindowStyle Hidden
    } catch {}
  }
}

$roots = @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
)
$entries = @()
foreach ($root in $roots) {
  try {
    Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue | ForEach-Object {
      try {
        $dn = [string]$_.GetValue('DisplayName')
        if (-not (Match-Product $dn)) { return }
        $quiet = [string]$_.GetValue('QuietUninstallString')
        $uninst = [string]$_.GetValue('UninstallString')
        $entries += [pscustomobject]@{
          Name = $dn
          Quiet = $quiet
          Uninstall = $uninst
        }
      } catch {}
    }
  } catch {}
}

foreach ($e in $entries) {
  $cmd = $e.Quiet
  if ([string]::IsNullOrWhiteSpace($cmd)) { $cmd = $e.Uninstall }
  if ([string]::IsNullOrWhiteSpace($cmd)) { continue }
  Run-Uninstall $cmd
}

# Best-effort cleanup leftover folders when uninstall left them
$pf = @($env:ProgramFiles, ${env:ProgramFiles(x86)}) | Where-Object { $_ }
foreach ($root in $pf) {
  if ($target -eq 'icloud' -or $target -eq 'both') {
    $is = Join-Path $root 'Common Files\Apple\Internet Services'
    # do not force-delete Internet Services if other Apple apps need it; only if iCloud gone
  }
  if ($target -eq 'itunes' -or $target -eq 'both') {
    $it = Join-Path $root 'iTunes'
    if (Test-Path -LiteralPath $it) {
      try {
        if (-not (Test-Path -LiteralPath (Join-Path $it 'iTunes.exe'))) {
          Remove-Item -LiteralPath $it -Recurse -Force -ErrorAction SilentlyContinue
        }
      } catch {}
    }
  }
}

exit 0
"#;

    let script_path = std::env::temp_dir().join("Signum-Uninstall-Apple.ps1");
    tokio::fs::write(&script_path, script_body)
        .await
        .map_err(|_| "Не удалось подготовить удаление. Повторите.".to_string())?;
    let script_escaped = script_path.to_string_lossy().replace('\'', "''");
    let target = product.to_ascii_lowercase();
    let runner = format!(
        "$ErrorActionPreference = 'Stop'; \
         $env:SIGNUM_UNINSTALL_TARGET = '{target}'; \
         $p = Start-Process -FilePath 'powershell.exe' \
           -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{script_escaped}') \
           -Verb RunAs -PassThru -Wait -WindowStyle Hidden; \
         if ($null -eq $p) {{ exit 1 }}; \
         exit $p.ExitCode"
    );

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &runner,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|_| "Не удалось запустить удаление. Повторите.".to_string())?;

    Ok(output.status.code().unwrap_or(-1))
}

/// Force-remove desktop iCloud + iTunes so a clean reinstall can proceed.
#[cfg(target_os = "windows")]
async fn uninstall_windows_icloud_and_itunes(app: &AppHandle, driver_only: bool) -> Result<(), String> {
    // Без SIGNUM_NEEDS_ICLOUD переустанавливаем только своё: драйвер Apple
    // (`driver_only`) или iTunes. iCloud человека — в том числе из Microsoft
    // Store — и его iTunes при переустановке драйвера Signum не трогает.
    let device = if driver_only { "amds" } else { "itunes" };
    let device_label = if driver_only { "драйвер Apple" } else { "iTunes" };
    let products = if SIGNUM_NEEDS_ICLOUD { "both" } else { device };
    let icloud_installed = || SIGNUM_NEEDS_ICLOUD && windows_icloud_installed();
    let device_installed = || device_support_installed(driver_only);
    emit_icloud_progress(
        app,
        "installing",
        3.0,
        &if SIGNUM_NEEDS_ICLOUD {
            "Удаляем iCloud и iTunes…\nЕсли Windows спросит разрешение — нажмите «Да».".to_string()
        } else {
            format!("Удаляем {device_label}…\nЕсли Windows спросит разрешение — нажмите «Да».")
        },
    );

    // Store package first — blocks classic desktop stack.
    if SIGNUM_NEEDS_ICLOUD && windows_icloud_msstore_installed() {
        emit_icloud_progress(app, "installing", 4.0, "Убираем iCloud из Microsoft Store…");
        let _ = remove_windows_icloud_msstore();
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Uninstall desktop products (elevated).
    let code = uninstall_windows_apple_desktop_products(products).await?;
    if code == 1223 {
        return Err(
            "Удаление отменено. Нажмите «Да» в запросе Windows и повторите «Переустановить»."
                .into(),
        );
    }

    // Wait until detection no longer sees them (or timeout).
    emit_icloud_progress(app, "installing", 8.0, "Проверяем удаление…");
    for attempt in 0..15u32 {
        let icloud_gone = !icloud_installed();
        let itunes_gone = !device_installed();
        if icloud_gone && itunes_gone {
            break;
        }
        if attempt == 5 || attempt == 10 {
            // Second pass if first uninstall did not clear everything.
            let _ = uninstall_windows_apple_desktop_products(products).await;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Best-effort: kill leftover processes that lock files.
    let _ = StdCommand::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "Get-Process iTunes,iCloud,iCloudServices,AppleMobileDeviceProcess,APSDaemon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue",
        ])
        .creation_flags(0x0800_0000)
        .status();

    // If still detected, try product-specific second pass.
    if icloud_installed() {
        emit_icloud_progress(app, "installing", 10.0, "Доудаляем iCloud…");
        let _ = uninstall_windows_apple_desktop_products("icloud").await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    if device_installed() {
        emit_icloud_progress(app, "installing", 12.0, &format!("Доудаляем {device_label}…"));
        let _ = uninstall_windows_apple_desktop_products(device).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Soft failure: still proceed to install even if residual keys remain —
    // installer repair usually fixes it. Hard-fail only if UAC cancelled (above).
    if icloud_installed() || device_installed() {
        emit_icloud_progress(
            app,
            "installing",
            14.0,
            "Часть файлов ещё на месте — ставим поверх…",
        );
    } else {
        emit_icloud_progress(
            app,
            "installing",
            14.0,
            "Старые компоненты удалены. Устанавливаем…",
        );
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn windows_icloud_msstore_installed() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
fn remove_windows_icloud_msstore() -> bool {
    true
}

/// Wait until installed flag becomes true (installer may finish before files appear).
#[cfg(target_os = "windows")]
async fn wait_until_installed(
    app: &AppHandle,
    label: &str,
    check: impl Fn() -> bool,
    progress: f64,
    attempts: u32,
) -> bool {
    for attempt in 0..attempts {
        if check() {
            return true;
        }
        if attempt == 0 {
            emit_icloud_progress(
                app,
                "installing",
                progress,
                &format!("Проверяем установку {label}…"),
            );
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    check()
}

#[cfg(target_os = "windows")]
fn icloud_install_exit_message(code: i32) -> String {
    match code {
        1223 => {
            "Установка отменена. Если Windows спросит разрешение — нажмите «Да» и повторите.".into()
        }
        1603 => "Не удалось установить iCloud.\n\n\
                 Часто не хватает мультимедийных компонентов Windows.\n\
                 Нажмите «Повторить» или установите iCloud вручную, затем перезапустите Signum."
            .into(),
        1618 => "Уже идёт другая установка. Дождитесь окончания и нажмите «Повторить».".into(),
        1612 | 1613 => "Файл установки повреждён. Нажмите «Повторить».".into(),
        1638 => "iCloud уже установлен. Перезапустите Signum.".into(),
        _ => "Не удалось установить iCloud. Нажмите «Повторить» или установите вручную.".into(),
    }
}

/// Media stack ready for Apple iCloud.
/// Important: MFP can be "installed" while Windows Media Player is still **disabled**
/// (optional feature off) — iCloud then refuses to install. We require WMP **enabled**.
///
/// exit 10 = ready, 11 = not ready, 12 = WMP present but disabled (needs enable).
#[cfg(target_os = "windows")]
fn windows_media_stack_status() -> u8 {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$wmpDisabled = $false
$wmpEnabled = $false

# 1) Optional feature: "Проигрыватель Windows Media" must be Enabled (not just installed).
try {
  $feat = Get-WindowsOptionalFeature -Online -FeatureName 'WindowsMediaPlayer' -ErrorAction SilentlyContinue
  if ($null -ne $feat) {
    if ($feat.State -eq 'Enabled') { $wmpEnabled = $true }
    else {
      # Disabled / DisabledWithPayloadRemoved / etc.
      $wmpDisabled = $true
    }
  }
} catch {}

# 2) Capabilities (N editions + FoD)
$mfpMissing = $false
$mfpInstalled = $false
$wmpCapMissing = $false
$wmpCapInstalled = $false
try {
  $caps = @(Get-WindowsCapability -Online -ErrorAction SilentlyContinue | Where-Object {
    $_.Name -like 'Media.MediaFeaturePack*' -or $_.Name -like 'Media.WindowsMediaPlayer*'
  })
  foreach ($c in $caps) {
    if ($c.Name -like 'Media.MediaFeaturePack*') {
      if ($c.State -eq 'Installed') { $mfpInstalled = $true }
      if ($c.State -eq 'NotPresent') { $mfpMissing = $true }
    }
    if ($c.Name -like 'Media.WindowsMediaPlayer*') {
      if ($c.State -eq 'Installed') { $wmpCapInstalled = $true }
      if ($c.State -eq 'NotPresent') { $wmpCapMissing = $true }
    }
  }
} catch {}

# 3) DLLs iCloud actually probes
$sys = $env:SystemRoot
$dllOk = $false
foreach ($d in @('wmp.dll','wmvcore.dll','mfplat.dll')) {
  if ((Test-Path (Join-Path $sys "System32\$d")) -or (Test-Path (Join-Path $sys "SysWOW64\$d"))) {
    $dllOk = $true; break
  }
}

# WMP optional feature known but OFF → this is the common false-positive case.
if ($wmpDisabled) { exit 12 }

# Capability says WMP/MFP NotPresent → missing
if ($mfpMissing -or $wmpCapMissing) { exit 11 }

# Ready: WMP enabled (or not listed as optional) AND dlls OR capabilities installed
if ($wmpEnabled -and $dllOk) { exit 10 }
if ($wmpCapInstalled -and $dllOk) { exit 10 }
if ($mfpInstalled -and $dllOk -and -not $wmpDisabled) { exit 10 }
if ($dllOk -and -not $wmpDisabled -and -not $mfpMissing -and -not $wmpCapMissing) { exit 10 }

exit 11
"#;
    StdCommand::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(0x0800_0000)
        .status()
        .ok()
        .and_then(|s| s.code())
        .unwrap_or(11) as u8
}

/// True when media stack is good enough for iCloud (WMP enabled + MFP/DLLs).
#[cfg(target_os = "windows")]
fn windows_media_feature_pack_present() -> bool {
    windows_media_stack_status() == 10
}

/// WMP optional feature is installed but not turned on.
#[cfg(target_os = "windows")]
fn windows_media_player_disabled() -> bool {
    windows_media_stack_status() == 12
}

/// Elevated: enable Windows Media Player + install media capabilities (no user-facing hosts).
/// Exit: 0 ok, 3010 reboot, 1223 UAC cancel, else error.
#[cfg(target_os = "windows")]
async fn enable_windows_media_player_elevated() -> Result<i32, String> {
    let script_body = r#"
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$restart = $false
$any = $false

# Enable legacy Windows Media Player optional feature (the usual iCloud blocker).
try {
  $feat = Get-WindowsOptionalFeature -Online -FeatureName 'WindowsMediaPlayer' -ErrorAction SilentlyContinue
  if ($null -ne $feat -and $feat.State -ne 'Enabled') {
    $r = Enable-WindowsOptionalFeature -Online -FeatureName 'WindowsMediaPlayer' -All -NoRestart -ErrorAction Stop
    $any = $true
    if ($r.RestartNeeded) { $restart = $true }
  } elseif ($null -ne $feat -and $feat.State -eq 'Enabled') {
    $any = $true
  }
} catch {}

# FoD capabilities
foreach ($name in @(
  'Media.WindowsMediaPlayer~~~~0.0.12.0',
  'Media.MediaFeaturePack~~~~0.0.1.0'
)) {
  try {
    $cap = Get-WindowsCapability -Online -Name $name -ErrorAction SilentlyContinue
    if ($null -eq $cap) { continue }
    if ($cap.State -eq 'Installed') { $any = $true; continue }
    $r = Add-WindowsCapability -Online -Name $name -ErrorAction Stop
    $any = $true
    if ($r.RestartNeeded) { $restart = $true }
  } catch {
    try {
      $p = Start-Process -FilePath "$env:SystemRoot\System32\dism.exe" `
        -ArgumentList "/Online","/Enable-Feature","/FeatureName:WindowsMediaPlayer","/All","/NoRestart" `
        -Wait -PassThru -WindowStyle Hidden
      if ($p.ExitCode -eq 0 -or $p.ExitCode -eq 3010) {
        $any = $true
        if ($p.ExitCode -eq 3010) { $restart = $true }
      }
    } catch {}
    try {
      $p = Start-Process -FilePath "$env:SystemRoot\System32\dism.exe" `
        -ArgumentList "/Online","/Add-Capability","/CapabilityName:$name","/NoRestart" `
        -Wait -PassThru -WindowStyle Hidden
      if ($p.ExitCode -eq 0 -or $p.ExitCode -eq 3010) {
        $any = $true
        if ($p.ExitCode -eq 3010) { $restart = $true }
      }
    } catch {}
  }
}

if ($restart) { exit 3010 }
if ($any) { exit 0 }
exit 1
"#;

    let script_path = std::env::temp_dir().join("Signum-Enable-WMP.ps1");
    tokio::fs::write(&script_path, script_body)
        .await
        .map_err(|_| "Не удалось подготовить установку. Повторите.".to_string())?;
    let script_escaped = script_path.to_string_lossy().replace('\'', "''");
    let runner = format!(
        "$ErrorActionPreference = 'Stop'; \
         $p = Start-Process -FilePath 'powershell.exe' \
           -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{script_escaped}') \
           -Verb RunAs -PassThru -Wait -WindowStyle Hidden; \
         if ($null -eq $p) {{ exit 1 }}; \
         exit $p.ExitCode"
    );

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &runner,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|_| "Не удалось запустить установку. Повторите.".to_string())?;

    Ok(output.status.code().unwrap_or(-1))
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn windows_is_n_or_kn_edition() -> bool {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$p = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -ErrorAction SilentlyContinue)
$name = [string]$p.ProductName
$edition = [string]$p.EditionID
if ($edition -match '(?i)(^|_)N($|_)|KN') { exit 10 }
if ($name -match '(?i)\bN\b|\bKN\b') { exit 10 }
exit 0
"#;
    StdCommand::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(0x0800_0000)
        .status()
        .map(|s| s.code() == Some(10))
        .unwrap_or(false)
}

/// Detect desktop iTunes. Must not false-negative — otherwise we re-launch the installer.
#[cfg(target_os = "windows")]
fn windows_itunes_installed() -> bool {
    // Fast path: standard install locations.
    for root in windows_program_files_paths() {
        if root.join(r"iTunes\iTunes.exe").is_file() {
            return true;
        }
    }

    // Robust registry / path check (handles spaces in "Program Files", WOW64, uninstall keys).
    // exit 10 = installed, 11 = missing
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$candidates = @(
  (Join-Path $env:ProgramFiles 'iTunes\iTunes.exe'),
  (Join-Path ${env:ProgramFiles(x86)} 'iTunes\iTunes.exe')
)
foreach ($p in $candidates) {
  if ($p -and (Test-Path -LiteralPath $p)) { exit 10 }
}

function Test-ExePath([string]$raw) {
  if ([string]::IsNullOrWhiteSpace($raw)) { return $false }
  $path = ($raw -split ',')[0].Trim().Trim('"')
  if ([string]::IsNullOrWhiteSpace($path)) { return $false }
  return (Test-Path -LiteralPath $path)
}

foreach ($key in @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\iTunes.exe',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths\iTunes.exe'
)) {
  try {
    $item = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
    if ($null -eq $item) { continue }
    $def = $item.'(default)'
    if (Test-ExePath $def) { exit 10 }
    if (Test-ExePath $item.Path) { exit 10 }
  } catch {}
}

$uninstallRoots = @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
)
foreach ($root in $uninstallRoots) {
  try {
    Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue | ForEach-Object {
      $dn = [string]$_.GetValue('DisplayName')
      if ($dn -notmatch '(?i)itunes') { return }
      # Skip unrelated Apple entries that only mention iTunes in description.
      if ($dn -notmatch '(?i)^itunes(\s|$)') { return }
      if (Test-ExePath ([string]$_.GetValue('DisplayIcon'))) { exit 10 }
      $loc = [string]$_.GetValue('InstallLocation')
      if ($loc) {
        $exe = Join-Path $loc 'iTunes.exe'
        if (Test-Path -LiteralPath $exe) { exit 10 }
      }
      # Uninstall entry with DisplayName iTunes is enough (repair/reinstall still bad UX).
      exit 10
    }
  } catch {}
}

# where.exe fallback
try {
  $w = & where.exe iTunes.exe 2>$null | Select-Object -First 1
  if ($w -and (Test-Path -LiteralPath $w)) { exit 10 }
} catch {}

exit 11
"#;
    StdCommand::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(0x0800_0000)
        .status()
        .map(|s| s.code() == Some(10))
        .unwrap_or(false)
}

/// Ensure media stack for iCloud:
/// 1) Enable Windows Media Player if installed-but-disabled (main real-world issue)
/// 2) Add media capabilities if missing
/// 3) Fall back to arch-specific .msu when still incomplete
/// Returns Ok(true) if we changed something, Ok(false) if already ready.
#[cfg(target_os = "windows")]
async fn install_media_feature_pack(app: &AppHandle) -> Result<bool, String> {
    if windows_media_feature_pack_present() {
        emit_icloud_progress(app, "installing", 12.0, "Проверка компонентов…");
        return Ok(false);
    }

    let _only_disabled_wmp = windows_media_player_disabled();
    emit_icloud_progress(
        app,
        "installing",
        6.0,
        "Готовим компоненты Windows…\nЕсли появится запрос — нажмите «Да».",
    );

    // Step A: enable WMP + capabilities (often enough — no download).
    let enable_code = enable_windows_media_player_elevated().await?;
    if enable_code == 1223 {
        return Err("Установка отменена. Нажмите «Да» в запросе Windows и повторите.".into());
    }

    tokio::time::sleep(Duration::from_secs(2)).await;
    if windows_media_feature_pack_present() {
        if enable_code == 3010 {
            return Err(
                "Почти готово.\n\nПерезагрузите компьютер, откройте Signum и нажмите «Повторить»."
                    .into(),
            );
        }
        emit_icloud_progress(app, "installing", 18.0, "Компоненты готовы. Проверяем…");
        return Ok(true);
    }

    // Step B: offline MSU when enable alone was not enough (typical N/KN).
    let arch = windows_mfp_arch_name();
    let msu_name = if arch == "x32" {
        MFP_X32_MIRROR_NAME
    } else {
        MFP_X64_MIRROR_NAME
    };
    let msu_url = signum_mirror_url(msu_name);
    let msu_target = std::env::temp_dir().join(format!("Signum-{msu_name}"));

    emit_icloud_progress(app, "downloading", 10.0, "Скачиваем компоненты…");

    download_setup_with_fallback(
        app,
        &[msu_url.as_str()],
        &msu_target,
        "компоненты",
        50_000,
        Some((10.0, 18.0)),
    )
    .await?;

    emit_icloud_progress(
        app,
        "installing",
        20.0,
        "Устанавливаем компоненты…\nЕсли появится запрос — нажмите «Да».",
    );

    let msu_path = msu_target.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $wusa = Join-Path $env:SystemRoot 'System32\\wusa.exe'; \
         $p = Start-Process -FilePath $wusa \
           -ArgumentList @('{msu_path}','/quiet','/norestart') \
           -Verb RunAs -PassThru -Wait; \
         if ($null -eq $p) {{ exit 1 }}; \
         exit $p.ExitCode"
    );

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|_| "Не удалось установить компоненты. Повторите.".to_string())?;

    let code = output.status.code().unwrap_or(-1);
    let ok_code = matches!(code, 0 | 3010 | 2359302);

    if code == 1223 {
        return Err("Установка отменена. Нажмите «Да» в запросе Windows и повторите.".into());
    }

    // Step C: enable WMP again after MSU (package can land disabled).
    emit_icloud_progress(app, "installing", 22.0, "Завершаем настройку…");
    let enable_after = enable_windows_media_player_elevated().await.unwrap_or(-1);

    tokio::time::sleep(Duration::from_secs(3)).await;
    let present_now = windows_media_feature_pack_present();
    let need_reboot = code == 3010 || enable_code == 3010 || enable_after == 3010;

    if present_now && !need_reboot {
        emit_icloud_progress(app, "installing", 24.0, "Компоненты готовы. Проверяем…");
        return Ok(true);
    }

    if present_now || ok_code || need_reboot {
        return Err(
            "Почти готово.\n\n1) Перезагрузите компьютер\n2) Откройте Signum и нажмите «Повторить»"
                .into(),
        );
    }

    Err("Не удалось подготовить систему для iCloud.\n\n\
         Откройте параметры Windows → дополнительные компоненты и включите мультимедиа, \
         перезагрузите ПК и нажмите «Повторить»."
        .into())
}

/// Run Apple setup EXE interactively (NO quiet/silent flags) with UAC elevation.
#[cfg(target_os = "windows")]
async fn run_apple_installer_interactive(
    app: &AppHandle,
    setup_path: &Path,
    label: &str,
    progress: f64,
) -> Result<(), String> {
    emit_icloud_progress(
        app,
        "installing",
        progress,
        &format!("Устанавливаем {label}…\nЕсли Windows спросит — нажмите «Да»."),
    );
    let setup = setup_path.to_string_lossy().replace('\'', "''");
    let is_msi = setup_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("msi"));
    // .msi ставит msiexec с полосой прогресса (/passive), без вопросов.
    let launch = if is_msi {
        format!(
            "$p = Start-Process -FilePath \"$env:SystemRoot\\System32\\msiexec.exe\" \
             -ArgumentList @('/i', '\"{setup}\"', '/passive', '/norestart') -Verb RunAs -PassThru -Wait;"
        )
    } else {
        // IMPORTANT: no /qn /quiet — user asked for non-silent iCloud install.
        format!("$p = Start-Process -FilePath '{setup}' -Verb RunAs -PassThru -Wait;")
    };
    // Запускаем с правами администратора файл, пришедший в том числе с нашего
    // зеркала, — только если его подписала Apple. Иначе код 9001.
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $sig = Get-AuthenticodeSignature -LiteralPath '{setup}'; \
         if ($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'O=Apple Inc\\.') {{ exit 9001 }}; \
         {launch} \
         if ($null -eq $p) {{ exit 1 }}; \
         exit $p.ExitCode"
    );
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|e| format!("Не удалось запустить установку {label}: {e}"))?;

    let code = output.status.code().unwrap_or(-1);
    if code == 0 || code == 3010 || code == 1641 {
        return Ok(());
    }
    if code == 9001 {
        // Битый или подменённый файл: удаляем, чтобы «Повторить» скачал заново.
        let _ = std::fs::remove_file(setup_path);
        return Err(format!(
            "Установщик ({label}) не прошёл проверку подписи Apple. Нажмите «Повторить» — файл скачается заново."
        ));
    }
    if is_msi && windows_amds_installed() {
        return Ok(());
    }
    // Some Apple installers return non-zero even after successful UI install.
    if label.to_ascii_lowercase().contains("icloud") && windows_icloud_installed() {
        return Ok(());
    }
    if label.to_ascii_lowercase().contains("itunes") && windows_itunes_installed() {
        return Ok(());
    }
    if code == 1223 {
        return Err(format!(
            "Установка {label} отменена. Нажмите «Да» в запросе Windows и повторите."
        ));
    }
    Err(icloud_install_exit_message(code).replace("iCloud", label))
}

#[tauri::command]
fn open_icloud_manual_install() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Apple removed the standalone installer from its support page. The
        // Signum mirror keeps the desktop package needed by anisette; Store
        // iCloud is only a last resort because it cannot provide those APIs.
        let standalone = signum_mirror_url(ICLOUD_SETUP_MIRROR_NAME);
        let direct = StdCommand::new("cmd")
            .args(["/C", "start", "", &standalone])
            .creation_flags(0x0800_0000)
            .status();
        if direct.map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
        let store = StdCommand::new("cmd")
            .args(["/C", "start", "", ICLOUD_MS_STORE_URL])
            .creation_flags(0x0800_0000)
            .status();
        if store.map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
        StdCommand::new("cmd")
            .args(["/C", "start", "", ICLOUD_APPLE_SUPPORT_URL])
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|e| format!("Не удалось открыть страницу загрузки: {e}"))?;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

/// Opens Microsoft Media Feature Pack help (required by iCloud on Windows N/KN).
#[tauri::command]
fn open_media_feature_pack() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        StdCommand::new("cmd")
            .args(["/C", "start", "", MEDIA_FEATURE_PACK_URL])
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|e| format!("Не удалось открыть страницу Media Feature Pack: {e}"))?;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

/// Opens Windows Settings → Optional features (where Media Feature Pack is installed).
#[tauri::command]
fn open_optional_features() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        StdCommand::new("cmd")
            .args(["/C", "start", "", "ms-settings:optionalfeatures"])
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|e| format!("Не удалось открыть «Дополнительные компоненты»: {e}"))?;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

/// Ensures Apple iCloud + iTunes for Windows are present (ipatool v3 / anisette).
/// Flow:
/// 1. Check iCloud / iTunes / MFP
/// 2. If MFP missing → download arch-specific .msu → install → recheck
/// 3. Download iCloud + iTunes in parallel (primary then mirror; host never shown)
/// 4. Install both interactively → verify on disk
///
/// `force`: reinstall/repair even when already detected on disk (Settings button).
#[tauri::command]
async fn ensure_icloud_dependency(
    app: AppHandle,
    force: Option<bool>,
) -> Result<IcloudEnsureResult, String> {
    let force = force.unwrap_or(false);
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, force);
        return Ok(IcloudEnsureResult {
            status: "not_needed".into(),
            installed: true,
            message: Some("На этом компьютере дополнительные компоненты не нужны.".into()),
        });
    }

    #[cfg(target_os = "windows")]
    {
        if !force && windows_apple_stack_installed() {
            return Ok(IcloudEnsureResult {
                status: "ready".into(),
                installed: true,
                message: None,
            });
        }

        if ICLOUD_ENSURE_RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(IcloudEnsureResult {
                status: "failed".into(),
                installed: false,
                message: Some("Установка уже идёт. Подождите…".into()),
            });
        }

        let ensure_result: Result<IcloudEnsureResult, String> = async {
            // Always re-check on disk — do not trust a single stale snapshot.
            // force=true → full uninstall then reinstall both products.
            // Windows 10+ получает один драйвер Apple вместо iTunes целиком; на
            // 7/8/8.1 — iTunes 12.10.11 (в нём свой USB-драйвер). Если драйвер
            // не скачается с зеркала, ниже откатимся на iTunes с сайта Apple.
            let mut driver_only = !windows_before_10();
            let mut need_icloud = SIGNUM_NEEDS_ICLOUD && (force || !windows_icloud_installed());
            let mut need_itunes = force || !device_support_installed(driver_only);
            let mfp_ok = windows_media_feature_pack_present();

            emit_icloud_progress(
                &app,
                if force { "installing" } else { "missing" },
                0.0,
                if force {
                    "Готовим полную переустановку…"
                } else {
                    "Проверяем…"
                },
            );

            // Settings «Переустановить»: wipe desktop iCloud + iTunes (and Store iCloud)
            // before downloading/installing again.
            if force {
                uninstall_windows_icloud_and_itunes(&app, driver_only).await?;
                need_icloud = SIGNUM_NEEDS_ICLOUD;
                need_itunes = true;
            }

            if !need_icloud && !need_itunes {
                // Store-only iCloud can coexist with nothing classic — still not ready for login.
                if SIGNUM_NEEDS_ICLOUD && windows_icloud_msstore_installed() && !windows_icloud_installed() {
                    need_icloud = true;
                } else {
                    emit_icloud_progress(&app, "ready", 100.0, "Готово");
                    return Ok(IcloudEnsureResult {
                        status: "ready".to_string(),
                        installed: true,
                        message: Some("Всё уже установлено.".into()),
                    });
                }
            }

            // Store iCloud conflicts with desktop installer and does not work for Apple ID login.
            // (force path already tried; keep for normal install when Store-only is present.)
            if need_icloud && windows_icloud_msstore_installed() {
                emit_icloud_progress(
                    &app,
                    "installing",
                    2.0,
                    "Убираем iCloud из Microsoft Store — нужна версия для компьютера…",
                );
                let removed = remove_windows_icloud_msstore();
                tokio::time::sleep(Duration::from_secs(2)).await;
                if !removed && windows_icloud_msstore_installed() {
                    return Err(
                        "Не удалось убрать iCloud из Microsoft Store.\n\n\
                         Удалите «iCloud» в параметрах Windows → Приложения, \
                         затем снова нажмите «Переустановить» в Signum."
                            .into(),
                    );
                }
                // After Store removal we always need the desktop installer.
                need_icloud = true;
            }

            // 1) Media stack first (MFP + Windows Media Player must be ENABLED).
            // iTunes на редакциях N без него тоже не ставится; драйверу он не нужен.
            if (need_icloud || (need_itunes && !driver_only)) && !mfp_ok {
                emit_icloud_progress(
                    &app,
                    "installing",
                    4.0,
                    "Готовим компоненты Windows…",
                );
                install_media_feature_pack(&app).await?;

                emit_icloud_progress(&app, "installing", 26.0, "Проверяем…");
                tokio::time::sleep(Duration::from_secs(2)).await;

                if !windows_media_feature_pack_present() {
                    return Err(
                        "Не удалось подготовить систему.\n\n\
                         Перезагрузите компьютер и нажмите «Повторить»."
                            .into(),
                    );
                }

                // Fresh enable/install often needs reboot before iCloud sees it.
                return Err(
                    "Почти готово.\n\nПерезагрузите компьютер, откройте Signum и нажмите «Повторить»."
                        .into(),
                );
            }

            if need_icloud {
                emit_icloud_progress(
                    &app,
                    "installing",
                    28.0,
                    if need_itunes {
                        "Готовим iCloud и iTunes…"
                    } else {
                        "Готовим iCloud…"
                    },
                );
            } else if need_itunes {
                emit_icloud_progress(
                    &app,
                    "installing",
                    28.0,
                    if driver_only { "Готовим драйвер Apple…" } else { "Готовим iTunes…" },
                );
            }

            let icloud_target = std::env::temp_dir().join("Signum-iCloudSetup.exe");
            // Имя временного файла по источнику: на 8.1 не подхватить свежий
            // iTunes, а .msi нужен msiexec, а не запуск как программы.
            let itunes_target_for = |driver_only: bool| {
                let name = if driver_only { AMDS_MIRROR_NAME } else { itunes_setup_sources().1 };
                std::env::temp_dir().join(format!("Signum-{name}"))
            };
            let mut itunes_target = itunes_target_for(driver_only);

            // Re-check right before download (user may have installed while modal was open).
            // force keeps both installers on the path for repair/reinstall.
            need_icloud = SIGNUM_NEEDS_ICLOUD && (force || !windows_icloud_installed());
            need_itunes = force || !device_support_installed(driver_only);

            // On force reinstall always re-download fresh installers.
            let reuse_icloud = !force
                && (!need_icloud
                    || tokio::fs::metadata(&icloud_target)
                        .await
                        .ok()
                        .is_some_and(|meta| meta.len() > 5_000_000));
            let reuse_itunes = !force
                && (!need_itunes
                    || tokio::fs::metadata(&itunes_target)
                        .await
                        .ok()
                        .is_some_and(|meta| meta.len() > 500_000));

            // 2) Download only what is still missing (or everything when force).
            if (need_icloud && !reuse_icloud) || (need_itunes && !reuse_itunes) {
                emit_icloud_progress(
                    &app,
                    "downloading",
                    30.0,
                    match (need_icloud && !reuse_icloud, need_itunes && !reuse_itunes) {
                        (true, true) => "Скачиваем…",
                        (true, false) => "Скачиваем iCloud…",
                        (false, true) if driver_only => "Скачиваем драйвер Apple…",
                        (false, true) => "Скачиваем iTunes…",
                        (false, false) => "Подготовка…",
                    },
                );

                let icloud_mirror = signum_mirror_url(ICLOUD_SETUP_MIRROR_NAME);
                let itunes_sources = device_support_sources(driver_only);
                let app_icloud = app.clone();
                let app_itunes = app.clone();
                let icloud_path = icloud_target.clone();
                let itunes_path = itunes_target.clone();
                let do_icloud = need_icloud && !reuse_icloud;
                let do_itunes = need_itunes && !reuse_itunes;

                let (icloud_dl, itunes_dl) = tokio::join!(
                    async {
                        if do_icloud {
                             download_setup_with_fallback(
                                 &app_icloud,
                                 &[icloud_mirror.as_str(), ICLOUD_SETUP_URL_PRIMARY],
                                &icloud_path,
                                "iCloud",
                                5_000_000,
                                None,
                            )
                            .await
                        } else {
                            Ok(())
                        }
                    },
                    async {
                        if do_itunes {
                            let sources: Vec<&str> = itunes_sources.iter().map(String::as_str).collect();
                            download_setup_with_fallback(
                                &app_itunes,
                                &sources,
                                &itunes_path,
                                if driver_only { "драйвер Apple" } else { "iTunes" },
                                500_000,
                                None,
                            )
                            .await
                        } else {
                            Ok(())
                        }
                    },
                );
                // Драйвер лежит только на нашем зеркале. Недоступно оно — берём
                // iTunes целиком у Apple, как раньше.
                let itunes_dl = match itunes_dl {
                    Err(error) if driver_only => {
                        ipatool_debug_log(&format!("amds mirror failed, falling back to iTunes: {error}"));
                        driver_only = false;
                        itunes_target = itunes_target_for(false);
                        let sources = device_support_sources(false);
                        let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
                        download_setup_with_fallback(&app, &sources, &itunes_target, "iTunes", 500_000, None)
                            .await
                    }
                    other => other,
                };

                match (icloud_dl, itunes_dl) {
                    (Ok(()), Ok(())) => {
                        emit_icloud_progress(
                            &app,
                            "downloading",
                            55.0,
                            "Загрузка завершена. Устанавливаем…",
                        );
                    }
                    (Err(_), Ok(())) => {
                        return Err("Не удалось скачать. Проверьте интернет и повторите.".into());
                    }
                    (Ok(()), Err(_)) => {
                        return Err("Не удалось скачать. Проверьте интернет и повторите.".into());
                    }
                    (Err(_), Err(_)) => {
                        return Err("Не удалось скачать. Проверьте интернет и повторите.".into());
                    }
                }
            } else {
                emit_icloud_progress(&app, "downloading", 55.0, "Устанавливаем…");
            }

            // 3) Install interactively (no silent). Re-check before each installer
            // unless force reinstall was requested from Settings.
            need_icloud = SIGNUM_NEEDS_ICLOUD && (force || !windows_icloud_installed());
            if need_icloud {
                match run_apple_installer_interactive(&app, &icloud_target, "iCloud", 60.0).await {
                    Ok(()) => {}
                    Err(first_error) => {
                        let lower = first_error.to_ascii_lowercase();
                        if lower.contains("1603")
                            || lower.contains("media feature")
                            || lower.contains("мультимед")
                        {
                            emit_icloud_progress(
                                &app,
                                "installing",
                                62.0,
                                "Нужны компоненты Windows. Настраиваем…",
                            );
                            let _ = install_media_feature_pack(&app).await;
                            if !windows_media_feature_pack_present() {
                                return Err(
                                    "Перезагрузите компьютер и нажмите «Повторить».".into(),
                                );
                            }
                            return Err(
                                "Почти готово. Перезагрузите компьютер и нажмите «Повторить»."
                                    .into(),
                            );
                        }
                        return Err(first_error);
                    }
                }

                let icloud_ok = wait_until_installed(
                    &app,
                    "iCloud",
                    windows_icloud_installed,
                    72.0,
                    20,
                )
                .await;
                if !icloud_ok {
                    if windows_media_feature_pack_present() {
                        return Err(
                            "Установка iCloud не завершена. Завершите шаги установщика и повторите."
                                .into(),
                        );
                    }
                    return Err(
                        "iCloud не установился. Нажмите «Повторить», затем перезагрузите ПК."
                            .into(),
                    );
                }
                emit_icloud_progress(&app, "installing", 78.0, "iCloud готов.");
            }

            // Critical: re-check iTunes AFTER iCloud.
            // 1) Initial detect may have been wrong.
            // 2) iCloudSetup can install/upgrade Apple stack components.
            // force=true always re-runs iTunes installer for repair.
            let device_label = if driver_only { "драйвер Apple" } else { "iTunes" };
            need_itunes = force || !device_support_installed(driver_only);
            if need_itunes && !force {
                emit_icloud_progress(
                    &app,
                    "installing",
                    80.0,
                    &format!("Проверяем {device_label}…"),
                );
                // One more disk/registry pass after a short delay (installers settle).
                tokio::time::sleep(Duration::from_secs(1)).await;
                need_itunes = !device_support_installed(driver_only);
            }

            if need_itunes {
                // Download only now if we skipped earlier because of a false "already installed".
                let have_itunes_setup = tokio::fs::metadata(&itunes_target)
                    .await
                    .ok()
                    .is_some_and(|meta| meta.len() > 500_000);
                if !have_itunes_setup {
                    let sources = device_support_sources(driver_only);
                    let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
                    download_setup_with_fallback(
                        &app,
                        &sources,
                        &itunes_target,
                        device_label,
                        500_000,
                        Some((80.0, 88.0)),
                    )
                    .await?;
                }

                // Final gate right before launching the UI installer.
                if !force && device_support_installed(driver_only) {
                    emit_icloud_progress(
                        &app,
                        "installing",
                        90.0,
                        &format!("{device_label} уже установлен — пропускаем."),
                    );
                } else {
                    run_apple_installer_interactive(&app, &itunes_target, device_label, 90.0).await?;
                    let itunes_ok = wait_until_installed(
                        &app,
                        device_label,
                        || device_support_installed(driver_only),
                        94.0,
                        20,
                    )
                    .await;
                    if !itunes_ok {
                        return Err(format!(
                            "Установка ({device_label}) не завершена. Завершите шаги установщика и повторите."
                        ));
                    }
                    emit_icloud_progress(&app, "installing", 96.0, &format!("{device_label} готов."));
                }
            } else {
                emit_icloud_progress(
                    &app,
                    "installing",
                    92.0,
                    &format!("{device_label} уже установлен — пропускаем."),
                );
            }

            // 4) Final verification (fresh checks).
            let icloud_ok = !SIGNUM_NEEDS_ICLOUD || windows_icloud_installed();
            let itunes_ok = device_support_installed(driver_only);
            if icloud_ok && itunes_ok {
                emit_icloud_progress(
                    &app,
                    "ready",
                    100.0,
                    if force {
                        "Переустановка завершена"
                    } else {
                        "Готово"
                    },
                );
                Ok(IcloudEnsureResult {
                    status: "installed".to_string(),
                    installed: true,
                    message: Some(
                        if force {
                            "Компоненты переустановлены. Полностью закройте Signum и войдите снова."
                                .into()
                        } else {
                            "Готово".into()
                        },
                    ),
                })
            } else if icloud_ok && !itunes_ok {
                Err(format!("Не найден {device_label}. Нажмите «Повторить»."))
            } else if !icloud_ok && itunes_ok {
                Err("iCloud не найден. Нажмите «Повторить».".into())
            } else {
                Err("Установка не завершена. Нажмите «Повторить».".into())
            }
        }
        .await;

        ICLOUD_ENSURE_RUNNING.store(false, Ordering::SeqCst);

        match ensure_result {
            Ok(value) => Ok(value),
            Err(error) => {
                emit_icloud_progress(&app, "failed", 100.0, &error);
                Ok(IcloudEnsureResult {
                    status: "failed".to_string(),
                    installed: windows_apple_stack_installed(),
                    message: Some(error),
                })
            }
        }
    }
}

#[tauri::command]
fn check_icloud_installed() -> IcloudEnsureResult {
    #[cfg(not(target_os = "windows"))]
    {
        return IcloudEnsureResult {
            status: "not_needed".into(),
            installed: true,
            message: None,
        };
    }
    #[cfg(target_os = "windows")]
    {
        // Ready only when both iCloud and iTunes are present.
        let installed = windows_apple_stack_installed();
        IcloudEnsureResult {
            status: if installed {
                "ready".into()
            } else {
                "missing".into()
            },
            installed,
            message: None,
        }
    }
}

#[tauri::command]
async fn download_update(
    app: AppHandle,
    url: String,
    expected_sha256: Option<String>,
) -> Result<String, String> {
    let target = std::env::temp_dir().join(if cfg!(windows) {
        "SignumSetup.exe"
    } else if cfg!(target_os = "linux") {
        if url.split('?').next().unwrap_or(&url).ends_with(".deb") { "SignumUpdate.deb" } else { "SignumUpdate.AppImage" }
    } else {
        "SignumUpdate.dmg"
    });
    let mut last_error = "Не удалось скачать обновление".to_string();
    for source in api_download_urls(&url) {
        match download_update_once(&app, &source, expected_sha256.as_deref(), &target).await {
            Ok(path) => return Ok(path),
            Err(error) => {
                let _ = tokio::fs::remove_file(&target).await;
                last_error = error;
            }
        }
    }
    Err(last_error)
}

async fn download_update_once(
    app: &AppHandle,
    url: &str,
    expected_sha256: Option<&str>,
    target: &Path,
) -> Result<String, String> {
    let client = update_http_client(Duration::from_secs(60))?;
    download_update_file(&client, url, expected_sha256, target, |percent| {
        let _ = app.emit("ota-progress", serde_json::json!({"percent": percent}));
    }).await
}

fn update_http_client(read_timeout: Duration) -> Result<reqwest::Client, String> {
    // A large installer may take many minutes. Limit connection/read inactivity,
    // not the total duration of a download that is still making progress.
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(read_timeout)
        .build()
        .map_err(|error| http_transport_message(&error, "Не удалось начать загрузку обновления"))
}

async fn download_update_file(
    client: &reqwest::Client,
    url: &str,
    expected_sha256: Option<&str>,
    target: &Path,
    mut progress: impl FnMut(f64),
) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| http_transport_message(&error, "Не удалось скачать обновление"))?;
    if !response.status().is_success() {
        return Err(http_status_message(
            response.status(),
            "Не удалось скачать обновление",
        ));
    }
    let total = response.content_length().unwrap_or(0);
    let expected_sha256 = expected_sha256
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    if let Some(expected) = expected_sha256.as_deref() {
        if !expected.chars().all(|value| value.is_ascii_hexdigit()) || expected.len() != 64 {
            return Err("Обновление не прошло проверку целостности".into());
        }
    }
    let mut file = tokio::fs::File::create(&target)
        .await
        .map_err(|e| e.to_string())?;
    let mut downloaded = 0u64;
    let mut hasher = Sha256::new();
    let mut stream = response.bytes_stream();
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|error| http_transport_message(&error, "Загрузка обновления прервана"))?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            downloaded as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        progress(percent);
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    if downloaded == 0 || (total > 0 && downloaded != total) {
        let _ = tokio::fs::remove_file(&target).await;
        return Err("Загрузка обновления прервана".into());
    }
    if let Some(expected) = expected_sha256 {
        let actual = hasher
            .finalize()
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        if actual != expected {
            let _ = tokio::fs::remove_file(&target).await;
            return Err("Обновление не прошло проверку целостности".into());
        }
    }
    Ok(target.to_string_lossy().into_owned())
}

#[tauri::command]
async fn install_downloaded_update(path: String) -> Result<(), String> {
    let downloaded = PathBuf::from(path);
    if !downloaded.exists() {
        return Err("Файл обновления не найден".into());
    }
    #[cfg(target_os = "windows")]
    {
        // Signum self-update: run the Signum setup silently (no wizard UI).
        // iCloud/iTunes installers are separate and stay interactive on purpose.
        let name = downloaded
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let looks_like_setup =
            name.contains("setup") || name.ends_with(".exe") || name.contains("signum");

        if looks_like_setup {
            let installer = downloaded.to_string_lossy().replace('\'', "''");
            // Relaunch the app once the installer is done. Without this the
            // update looked like a crash: the download reached 100%, Signum
            // exited to free its files, and nothing ever started it again.
            let app_exe = std::env::current_exe()
                .map(|path| path.to_string_lossy().replace('\'', "''"))
                .unwrap_or_default();
            // The Signum setup takes /S (and --silent as an alias). Both are
            // tried because an older downloaded build may only know one.
            let script = format!(
                "$ErrorActionPreference = 'Continue'; \
                 $exe = '{installer}'; \
                 $app = '{app_exe}'; \
                 $ok = $false; \
                 foreach ($a in @('/S', '--silent')) {{ \
                   $p = Start-Process -FilePath $exe -ArgumentList @($a) -PassThru -Wait -WindowStyle Hidden; \
                   if ($null -ne $p -and $p.ExitCode -eq 0) {{ $ok = $true; break }} \
                 }} \
                 Start-Sleep -Seconds 1; \
                 if ($app -ne '' -and (Test-Path -LiteralPath $app)) {{ Start-Process -FilePath $app }} \
                 Remove-Item -LiteralPath $exe -Force -ErrorAction SilentlyContinue; \
                 if ($ok) {{ exit 0 }} else {{ exit 1 }}"
            );
            let script_path = std::env::temp_dir().join("signum_silent_ota.ps1");
            fs::write(&script_path, &script).map_err(|e| e.to_string())?;
            Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-WindowStyle",
                    "Hidden",
                    "-File",
                    &script_path.to_string_lossy(),
                ])
                .creation_flags(0x0800_0000)
                .spawn()
                .map_err(|e| format!("Не удалось запустить обновление: {e}"))?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            std::process::exit(0);
        }

        // Fallback: replace running binary (portable / non-setup payload).
        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let script_path = std::env::temp_dir().join("signum_ota_update.ps1");
        let script = r#"param(
    [string]$CurrentExe,
    [string]$DownloadedExe
)
Start-Sleep -Seconds 2
for ($i = 0; $i -lt 60; $i++) {
    try {
        Copy-Item -LiteralPath $DownloadedExe -Destination $CurrentExe -Force
        Start-Process -FilePath $CurrentExe
        Remove-Item -LiteralPath $DownloadedExe -Force -ErrorAction SilentlyContinue
        break
    } catch {
        Start-Sleep -Seconds 1
    }
}
"#;
        fs::write(&script_path, script).map_err(|e| e.to_string())?;
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
                &script_path.to_string_lossy(),
                &current_exe.to_string_lossy(),
                &downloaded.to_string_lossy(),
            ])
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| format!("Не удалось запустить обновление: {e}"))?;
        tokio::time::sleep(Duration::from_millis(400)).await;
        std::process::exit(0);
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&downloaded)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if downloaded.extension().and_then(|value| value.to_str()) == Some("deb") {
            Command::new("xdg-open").arg(&downloaded).spawn()
                .map_err(|_| "Не удалось открыть установщик обновления. Откройте скачанный пакет SignumUpdate.deb вручную.".to_string())?;
            return Ok(());
        }
        let current = std::env::var_os("APPIMAGE").map(PathBuf::from)
            .ok_or("Для этой установки скачайте пакет .deb с сайта Signum и откройте его.")?;
        let staging = current.with_extension("AppImage.next");
        let backup = current.with_extension("AppImage.previous");
        fs::copy(&downloaded, &staging).map_err(|_| "Не удалось сохранить обновление рядом с Signum. Проверьте права на папку.".to_string())?;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755)).map_err(|error| error.to_string())?;
        fs::copy(&current, &backup).map_err(|error| error.to_string())?;
        fs::rename(&staging, &current).map_err(|error| error.to_string())?;
        if Command::new(&current).env_remove("APPIMAGE").env_remove("APPDIR")
            .env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD").spawn().is_err() {
            let _ = fs::rename(&backup, &current);
            return Err("Не удалось запустить обновление. Предыдущая версия восстановлена.".into());
        }
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ota_package_selection_keeps_matching_url_and_checksum() {
        let value = serde_json::json!({"download_url":"https://example.test/update.AppImage", "sha256":"image", "deb_download_url":"https://example.test/update.deb", "deb_sha256":"deb"});
        assert_eq!(parse_ota_for_package(&value, true).sha256.as_deref(), Some("deb"));
        assert!(parse_ota_for_package(&value, true).download_url.unwrap().ends_with(".deb"));
        assert_eq!(parse_ota_for_package(&value, false).sha256.as_deref(), Some("image"));
        let legacy = serde_json::json!({"download_url":"https://example.test/update.AppImage", "sha256":"image"});
        assert_eq!(parse_ota_for_package(&legacy, true).sha256.as_deref(), Some("image"));
    }
    #[tokio::test]
    async fn ota_download_keeps_active_stream_and_rejects_stalled_or_corrupt_files() {
        use std::io::{Read, Write};
        fn server(stall: bool) -> (String, std::thread::JoinHandle<()>) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/update.exe", listener.local_addr().unwrap());
            let worker = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut request = [0; 4096];
                socket.read(&mut request).unwrap();
                if socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n").is_err() { return; }
                for _ in 0..10 {
                    if socket.write_all(b"x").is_err() { break; }
                    std::thread::sleep(Duration::from_millis(if stall { 1200 } else { 100 }));
                }
            });
            (url, worker)
        }
        let target = std::env::temp_dir().join(format!("signum-ota-test-{}-{}.exe", std::process::id(), unix_now()));
        let expected = format!("{:x}", Sha256::digest(b"xxxxxxxxxx"));
        let client = update_http_client(Duration::from_millis(700)).unwrap();
        let (url, worker) = server(false);
        let mut percentages = Vec::new();
        let started = std::time::Instant::now();
        download_update_file(&client, &url, Some(&expected), &target, |value| percentages.push(value)).await.unwrap();
        worker.join().unwrap();
        assert!(started.elapsed() > Duration::from_millis(700));
        assert_eq!(std::fs::read(&target).unwrap(), b"xxxxxxxxxx");
        assert_eq!(percentages.last(), Some(&100.0));

        // Reproduce the old total-deadline failure on the same active stream.
        let old_client = reqwest::Client::builder().timeout(Duration::from_millis(700)).build().unwrap();
        let (url, worker) = server(false);
        assert!(download_update_file(&old_client, &url, Some(&expected), &target, |_| {}).await.is_err());
        worker.join().unwrap();

        let (url, worker) = server(false);
        assert!(download_update_file(&client, &url, Some(&"0".repeat(64)), &target, |_| {}).await.is_err());
        assert!(!target.exists());
        worker.join().unwrap();

        let (url, worker) = server(true);
        assert!(download_update_file(&client, &url, Some(&expected), &target, |_| {}).await.is_err());
        worker.join().unwrap();
        let _ = std::fs::remove_file(&target);
    }
    use base64::Engine as _;

    #[test]
    fn installed_app_parser_reads_offloaded_status_and_store_id_from_binary_metadata() {
        let mut metadata = plist::Dictionary::new();
        metadata.insert("itemId".into(), plist::Value::Integer(123456789.into()));
        let mut data = Vec::new();
        plist::Value::Dictionary(metadata).to_writer_binary(&mut data).unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let xml = format!(
            r#"<?xml version="1.0"?><plist version="1.0"><array><dict><key>CFBundleIdentifier</key><string>com.example.offloaded</string><key>IsPlaceholder</key><true/><key>iTunesMetadata</key><data>{encoded}</data></dict></array></plist>"#
        );
        let apps = apps_from_plist(&xml);
        assert_eq!(apps.len(), 1);
        assert!(apps[0].is_placeholder);
        assert_eq!(apps[0].app_store_id.as_deref(), Some("123456789"));
        let stock_xml = r#"<?xml version="1.0"?><plist version="1.0"><array><dict><key>CFBundleIdentifier</key><string>com.example.installed</string><key>CFBundleShortVersionString</key><string>1.2</string></dict></array></plist>"#;
        let apps = apps_from_plist(stock_xml);
        assert_eq!(apps.len(), 1);
        assert!(!apps[0].is_placeholder);
        assert_eq!(apps[0].app_store_id, None);
        let mut metadata = plist::Dictionary::new();
        metadata.insert("artistId".into(), plist::Value::Integer(999.into()));
        assert_eq!(plist_item_id(&plist::Value::Dictionary(metadata.clone())), None);
        metadata.insert("itemId".into(), plist::Value::String("".into()));
        assert_eq!(plist_item_id(&plist::Value::Dictionary(metadata.clone())), None);
        metadata.insert("itemId".into(), plist::Value::String("123456789".into()));
        assert_eq!(plist_item_id(&plist::Value::Dictionary(metadata)), Some("123456789".into()));
    }

    #[test]
    fn installed_app_list_uses_compatible_placeholder_order_and_stock_fallback() {
        let attempts = installed_app_list_attempts(None);
        assert_eq!(&attempts[0][..4], ["list", "--user", "--placeholders", "--xml"]);
        let without_flag: Vec<_> = attempts[0].iter().filter(|arg| arg.as_str() != "--placeholders").cloned().collect();
        assert_eq!(attempts[1], without_flag);
        assert_eq!(attempts[2], ["list", "--user"]);
        let targeted = installed_app_list_attempts(Some("test-udid"));
        assert_eq!(&targeted[0][..6], ["-u", "test-udid", "list", "--user", "--placeholders", "--xml"]);
        assert!(targeted.iter().any(|args| args[0] == "-n" && !args.contains(&"--placeholders".to_string())));
    }

    #[test]
    fn catalog_cache_refreshes_on_success_and_never_hides_client_gate() {
        let dir = std::env::temp_dir().join(format!("signum-feed-{}", Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("catalog.json");
        let first = serde_json::json!({"ok":true,"apps":[{"id":"bank","title":"Old"}],"groups":[],"banners":[]});
        let second = serde_json::json!({"ok":true,"apps":[{"id":"bank","title":"New"}],"groups":[],"banners":[]});
        for feed in [first, second] {
            catalog_reply_with_cache(Some(&path), "main.php", Ok((reqwest::StatusCode::OK, feed.clone()))).unwrap();
            let (_, cached) = catalog_reply_with_cache(Some(&path), "main.php", Err("both servers offline".into())).unwrap();
            assert_eq!(cached["apps"], feed["apps"]);
            assert_eq!(cached["_signum_from_cache"], true);
            assert!(cached["_signum_cached_at"].as_u64().is_some());
        }
        let gate = serde_json::json!({"ok":false,"error":"client_outdated"});
        let (status, value) = catalog_reply_with_cache(Some(&path), "main.php", Ok((reqwest::StatusCode::UPGRADE_REQUIRED, gate.clone()))).unwrap();
        assert_eq!(status, reqwest::StatusCode::UPGRADE_REQUIRED);
        assert_eq!(value, gate);
        assert!(!valid_catalog_response("main.php", &gate));
        assert!(!valid_catalog_response("main.php", &serde_json::json!({"apps":[{}]})));
        fs::write(&path, "broken JSON").unwrap();
        assert!(catalog_reply_with_cache(Some(&path), "main.php", Err("offline".into())).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn queue_pause_reorder_and_cancel_do_not_start_waiting_jobs() {
        let queue = InstallQueue::default();
        for id in ["a", "b", "c"] {
            queue.send(QueuedInstall::OpenExportsFolder { job_id: id.into() }).await.unwrap();
        }
        assert_eq!(queue.next().await.id(), "a");
        queue.paused.store(true, Ordering::Relaxed);
        move_queue_item(&mut *queue.pending.lock().await, "c", -1).unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(30), queue.next()).await.is_err());
        assert_eq!(queue.pending.lock().await.len(), 2);
        assert!(move_queue_item(&mut *queue.pending.lock().await, "a", 1).is_err());
        assert!(move_queue_item(&mut *queue.pending.lock().await, "c", 2).is_err());
        queue.paused.store(false, Ordering::Relaxed);
        assert_eq!(queue.next().await.id(), "c");
        queue.pending.lock().await.retain(|item| item.id() != "b");
        assert!(queue.pending.lock().await.is_empty());
        assert_eq!(queue.current.lock().await.as_deref(), Some("c"));
    }

    #[test]
    fn api_urls_keep_primary_then_configured_fallback() {
        assert_eq!(api_urls("main.php"), [
            "https://api.appbank.pw/signum/main.php",
            "https://apiz.appbank.pw/signum/main.php",
        ]);
        assert_eq!(api_urls("/logs.php")[0], "https://api.appbank.pw/signum/logs.php");
        assert_eq!(api_urls("ota.php"), [
            "https://api.appbank.pw/signum/ota.php",
            "https://apiz.appbank.pw/signum/ota.php",
        ]);
        assert_eq!(api_urls("files/iCloudSetup.exe")[1], "https://apiz.appbank.pw/signum/files/iCloudSetup.exe");
        assert_eq!(api_urls("files/tools/windows-x64/ipatool.exe")[1], "https://apiz.appbank.pw/signum/files/tools/windows-x64/ipatool.exe");
    }

    #[test]
    fn api_downloads_keep_path_query_and_source_order() {
        for path in ["files/iTunes64Setup.exe", "files/iCloudSetup.exe", "releases/1.1.9-b37.exe", "lib.php?name=assets"] {
            let urls = api_urls(path);
            assert_eq!(api_download_urls(&urls[0]), urls);
            assert_eq!(api_download_urls(&urls[1]), [urls[1].clone(), urls[0].clone()]);
        }
        for external in [
            ICLOUD_SETUP_URL_PRIMARY,
            "https://api.appbank.pw.example.com/signum/files/iCloudSetup.exe",
            "https://api.appbank.pw/other/files/iCloudSetup.exe",
            "https://api.appbank.pw/signum-other/files/iCloudSetup.exe",
            "https://api.appbank.pw:8443/signum/files/iCloudSetup.exe",
            "http://api.appbank.pw/signum/files/iCloudSetup.exe",
        ] {
            assert_eq!(api_download_urls(external), [external]);
        }
    }

    #[tokio::test]
    async fn api_json_fails_over_on_broken_responses_but_preserves_client_gate() {
        fn server(reply: &str) -> (String, std::thread::JoinHandle<()>) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/ota.php", listener.local_addr().unwrap());
            let reply = reply.to_string();
            let worker = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut request = [0; 4096];
                socket.read(&mut request).unwrap();
                socket.write_all(reply.as_bytes()).unwrap();
            });
            (url, worker)
        }
        let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(3)).build().unwrap();
        for broken in [
            "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 408 Timeout\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 6\r\n\r\n<html>",
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\n<html>",
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nnull",
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{}",
        ] {
            let (primary, first) = server(broken);
            let (reserve, second) = server("HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}");
            let (status, value) = api_get_json(&client, &[primary, reserve], &[], "ota.php").await.unwrap();
            assert!(status.is_success());
            assert_eq!(value["ok"], true);
            first.join().unwrap();
            second.join().unwrap();
        }
        for status in [403, 426] {
            let body = "{\"error\":\"client_outdated\"}";
            let (primary, worker) = server(&format!("HTTP/1.1 {status} Error\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
            let (actual, value) = api_get_json(&client, &[primary, "http://127.0.0.1:0/ota.php".into()], &[], "ota.php").await.unwrap();
            assert_eq!(actual.as_u16(), status);
            assert_eq!(value["error"], "client_outdated");
            worker.join().unwrap();
        }
    }

    #[test]
    fn ipatool_min_ios_matches_exact_build_and_platform() {
        let output = "debug: querying metadata\n{\n\"success\":true,\n\"externalVersionID\":\"890000001\",\n\"minimumOSVersion\":\" 14.0 \" ,\n\"supportedPlatforms\":[\"iPhoneOS\"]\n}";
        assert_eq!(ipatool_metadata_min_ios(output, "890000001"), Some("14.0".into()));
        assert_eq!(ipatool_metadata_min_ios(output, "890000002"), None);
        assert_eq!(ipatool_metadata_min_ios(&output.replace("iPhoneOS", "macOS"), "890000001"), None);
        assert_eq!(ipatool_metadata_min_ios(&output.replace("true", "false"), "890000001"), None);
        assert_eq!(ipatool_metadata_min_ios(r#"{"success":true,"externalVersionID":890000001,"minimumOSVersion":"15.2.1"}"#, "890000001"), Some("15.2.1".into()));
    }

    #[test]
    fn ipatool_metadata_success_does_not_imply_known_min_ios() {
        for raw in [
            r#"{"success":true,"externalVersionID":"890000001","displayVersion":"2.0"}"#,
            r#"{"success":true,"externalVersionID":"890000001","minimumOSVersion":""}"#,
            r#"{"success":true,"externalVersionID":"890000001","minimumOSVersion":"unknown"}"#,
            r#"{"success":true,"externalVersionID":"890000001","minimumOSVersion":"0"}"#,
            r#"{"success":true,"externalVersionID":"890000001","minimumOSVersion":"14..0"}"#,
            r#"{"success":true,"externalVersionID":"890000001","minimumOSVersion":"14.0","supportedPlatforms":[]}"#,
            r#"{"success":true,"minimumOSVersion":"14.0"}"#,
            "get-version-metadata error: session expired",
        ] {
            assert_eq!(ipatool_metadata_min_ios(raw, "890000001"), None, "{raw}");
        }
    }

    #[tokio::test]
    async fn min_ios_reserve_runs_only_when_primary_cannot_answer() {
        let value = min_ios_with_reserve(async { Some("16.0".into()) }, || async {
            panic!("native reserve must not run after ipatool success");
        }).await;
        assert_eq!(value.as_deref(), Some("16.0"));
        for primary in [None, Some("".into()), Some("unavailable".into())] {
            assert_eq!(min_ios_with_reserve(async { primary }, || async { Some("14.0".into()) }).await.as_deref(), Some("14.0"));
        }
        assert_eq!(min_ios_with_reserve(async { None }, || async { None }).await, None);
        assert_eq!(min_ios_with_reserve(async { None }, || async { Some("invalid".into()) }).await, None);
        assert!(require_known_min_ios(None).is_err());
        assert!(require_known_min_ios(Some("unknown".into())).is_err());
        assert_eq!(require_known_min_ios(Some("14.0".into())).unwrap(), "14.0");
    }

    #[test]
    fn empty_store_id_uses_bundle_cache_key() {
        for id in [None, Some(""), Some("  "), Some("NONE"), Some("12a"), Some("１２３")] {
            assert_eq!(normalized_app_store_id(id), None);
            assert_eq!(version_probe_cache_key(" app.t.guide ", id), "bundle:app.t.guide");
        }
        assert_eq!(normalized_app_store_id(Some(" 6756361699 ")), Some("6756361699"));
        assert_eq!(version_probe_cache_key("app.t.guide", Some(" 6756361699 ")), "6756361699");
    }

    #[test]
    fn catalog_download_uses_its_own_temp_directory() {
        let job_id = uuid::Uuid::new_v4().to_string();
        let dir = super::catalog_download_temp_dir(&job_id).unwrap();
        assert_eq!(dir, std::env::temp_dir().join(format!("signum-{job_id}")));
        assert!(super::catalog_download_temp_dir("../other").is_err());
        assert!(super::catalog_download_temp_dir("").is_err());
        std::fs::create_dir(&dir).unwrap();
        let ipa = dir.join("download.ipa");
        std::fs::write(&ipa, b"completed").unwrap();
        // Progress must also work before any metadata can be read from the IPA.
        assert_eq!(super::largest_matching_download_size(&[dir.clone()], "app.store.123", Some("123")), Some(9));
        super::cleanup_catalog_download(&job_id, &ipa);
        assert!(!dir.exists());
    }

    #[test]
    fn completed_ipa_resolves_history_without_filename_or_bundle_match() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("signum-resolve-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let output = dir.join("download.ipa");
        let named = dir.join("АЗС Татнефть.ipa");
        for item_id in ["<integer>1514207975</integer>", "<string>1514207975</string>"] {
            let mut archive = zip::ZipWriter::new(std::fs::File::create(&output).unwrap());
            archive.start_file("iTunesMetadata.plist", zip::write::SimpleFileOptions::default()).unwrap();
            let metadata = format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>itemId</key>{item_id}</dict></plist>");
            archive.write_all(metadata.as_bytes()).unwrap();
            archive.finish().unwrap();
            assert_eq!(super::resolve_completed_download(&output, "app.store.1514207975", Some("1514207975")).unwrap(), output);
            std::fs::rename(&output, &named).unwrap();
            assert_eq!(super::resolve_completed_download(&output, "app.store.1514207975", Some("1514207975")).unwrap(), named);
            // A different app must not be selected just because it is the only IPA.
            assert!(super::resolve_completed_download(&output, "app.store.999", Some("999")).is_err());
            std::fs::remove_file(&named).unwrap();
        }
        // Temporary downloads and directories must never count as completed IPAs.
        std::fs::write(dir.join("1514207975.ipa.tmp"), b"partial").unwrap();
        std::fs::create_dir(dir.join("1514207975.ipa")).unwrap();
        assert!(super::resolve_completed_download(&output, "app.store.1514207975", Some("1514207975")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn completed_ipa_uses_explicit_output_without_store_metadata() {
        let dir = std::env::temp_dir().join(format!("signum-output-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let output = dir.join("download.ipa");
        let archive = zip::ZipWriter::new(std::fs::File::create(&output).unwrap());
        archive.finish().unwrap();
        assert_eq!(super::resolve_completed_download(&output, "app.store.1514207975", Some("1514207975")).unwrap(), output);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_ipa_append_is_retried_but_not_permission_failures() {
        assert!(super::is_transient_ipatool_timeout("Download error: minizip: failed to open output IPA for append"));
        assert!(!super::is_transient_ipatool_timeout("Download error: minizip: cannot create output IPA"));
    }
    #[test]
    fn ipa_cache_must_match_selected_build_and_account() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("signum-ipa-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("123456789_2.0.ipa");
        let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        archive.start_file("Payload/Demo.app/Info.plist", zip::write::SimpleFileOptions::default()).unwrap();
        archive.write_all(br#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>test.demo</string><key>CFBundleShortVersionString</key><string>2.0</string></dict></plist>"#).unwrap();
        archive.start_file("iTunesMetadata.plist", zip::write::SimpleFileOptions::default()).unwrap();
        archive.write_all(br#"<?xml version="1.0"?><plist version="1.0"><dict><key>softwareVersionExternalIdentifier</key><integer>222</integer><key>apple-id</key><string>test@example.com</string></dict></plist>"#).unwrap();
        archive.finish().unwrap();
        assert!(super::try_cached_complete_ipa(&dir, "test.demo", Some("123456789"), Some("test@example.com"), Some("222")).is_some());
        assert!(super::try_cached_complete_ipa(&dir, "test.demo", Some("123456789"), Some("test@example.com"), Some("111")).is_none());
        assert!(super::try_cached_complete_ipa(&dir, "test.demo", Some("123456789"), Some("other@example.com"), Some("222")).is_none());
        assert!(super::try_cached_complete_ipa(&dir, "test.demo", Some("123456789"), Some("test@example.com"), None).is_none());
        let mut input = super::CatalogInstallInput {
            title: "Demo".into(), bundle_id: "test.demo".into(), app_store_id: Some("123456789".into()),
            external_version_id: Some("111".into()), version: Some("1.0".into()), in_store: true,
            already_owned: true, min_ios: None, temporary_region: None, temporary_us_region: false,
        };
        assert!(super::validate_requested_ipa_version(&path, &input).is_err());
        input.external_version_id = Some("222".into());
        assert!(super::validate_requested_ipa_version(&path, &input).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "downloads IPA using the saved Apple ID session"]
    async fn live_windows_explicit_ipa_output() {
        let adam = std::env::var("SIGNUM_TEST_ADAM").unwrap_or_else(|_| "6805045741".into());
        let ext = std::env::var("SIGNUM_TEST_BUILD").unwrap_or_else(|_| "890277967".into());
        let version = std::env::var("SIGNUM_TEST_VERSION").unwrap_or_else(|_| "1.0".into());
        let key = super::read_passphrase_from_file().unwrap();
        let program = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap())
            .join("Signum/resources/apple-tools/windows-x64/v5/ipatool.exe");
        let dir = std::env::temp_dir().join(format!("signum-ipa-check-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = super::windows_short_path(&dir).join("download.ipa");
        let args = vec!["--keychain-passphrase".into(), key, "--format".into(), "json".into(),
            "download".into(), "-i".into(), adam, "--external-version-id".into(),
            ext.clone(), "-o".into(), path.to_string_lossy().into_owned()];
        let mut result = super::run_process_raw_error(program.to_str().unwrap(), &args, std::time::Duration::from_secs(900), None).await;
        if result.as_ref().err().is_some_and(|error| error.contains("minizip: failed to open output IPA for append")) {
            println!("First append failed; retrying existing download.ipa.tmp");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            result = super::run_process_raw_error(program.to_str().unwrap(), &args, std::time::Duration::from_secs(900), None).await;
        }
        assert!(result.is_ok(), "{}", result.unwrap_err());
        assert_eq!(super::ipa_external_version_id(&path).as_deref(), Some(ext.as_str()));
        assert_eq!(super::cached_ipa_metadata(&path).2.as_deref(), Some(version.as_str()));
        println!("Fresh IPA: build {ext}, version {version}, explicit output verified");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_release_114_bundled_engines_pass_without_external_assets() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for arch in ["macos-arm64", "macos-x64"] {
            let engine = root.join("apple-tools").join(arch).join("v5/ipatool");
            let hash = super::sha256_file_hex(&engine).unwrap();
            assert!(super::macos_ipatool_embeds_kbsync(&hash));
            super::ensure_macos_kbsync_sidecars(engine.to_str().unwrap()).unwrap();
        }
    }
    #[test]
    fn macos_release_114_does_not_require_external_kbsync_assets() {
        for hash in [
            "8c8fc8984a33f88e4a4de915280801be3e486f9159fea2301ca9b707990cbefd",
            "c95ba7a45b330074090064d8dca4fb323a6bdf46126532561dc51a2e36d077f7",
            "fc034e14afc66418f9afd608fbc2b0425086bce7a9ed43fca284a34a37fb754a",
            "9900a16212e0697b64094fa4dec0d9da367b76f8d01b84604b21d9808d86347e",
            "2d6b4391fed1e4f3778af730a5b95f0aaee139f8f0aede4c3e62b6cd66ff2c6d",
            "2a5f789bbbc94e78eac1ae31fd55b883374b4244a23c9c65bf1240fad8b29594",
        ] {
            assert!(super::macos_ipatool_embeds_kbsync(hash));
        }
        assert!(!super::macos_ipatool_embeds_kbsync("unknown"));
    }
    #[test]
    fn tray_notice_is_limited_across_restarts() {
        let path = std::env::temp_dir().join(format!("signum-tray-{}.txt", uuid::Uuid::new_v4()));
        assert!(super::take_tray_notice(&path));
        assert!(super::take_tray_notice(&path));
        assert!(!super::take_tray_notice(&path));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "2");
        std::fs::remove_file(path).unwrap();
    }
    /// Тесты идут параллельно, а флаг подробных ошибок — глобальный.
    /// Установка его в начале теста ничего не гарантирует: сосед успевает
    /// переключить между записью и проверкой. Держим замок на всё время.
    static DETAILED_ERRORS_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Взять флаг под себя. Замок держится, пока жив возвращённый страж.
    fn with_detailed_errors(on: bool) -> std::sync::MutexGuard<'static, ()> {
        let guard = DETAILED_ERRORS_GUARD
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        DETAILED_ERRORS_ENABLED.store(on, Ordering::Relaxed);
        guard
    }

    use super::*;

    #[test]
    fn catalog_cache_is_account_bound_and_expires() {
        let mut cache = AccountCatalogCache {
            email: "owner@example.com".into(), saved_at: now(), ..Default::default()
        };
        assert!(account_catalog_cache_matches(&cache, " Owner@Example.com "));
        assert!(!account_catalog_cache_matches(&cache, "other@example.com"));
        cache.saved_at = now() - 24 * 3600;
        assert!(!account_catalog_cache_matches(&cache, "owner@example.com"));
        cache.saved_at = now() + 60;
        assert!(!account_catalog_cache_matches(&cache, "owner@example.com"));
    }

    #[test]
    fn store_download_failure_does_not_require_a_phone() {
        let _guard = with_detailed_errors(false);
        let message = format_process_error("ipatool.exe", 1, "unexpected upstream failure");
        assert!(message.contains("App Store"));
        assert!(!message.contains("iPhone"));
        let settings: AppSettings = serde_json::from_value(serde_json::json!({
            "downloads_dir": "downloads", "theme": "dark", "auto_update": true
        })).unwrap();
        assert!(!settings.background_enabled);
        let mut value = serde_json::to_value(settings).unwrap();
        value["background_enabled"] = serde_json::json!(false);
        assert!(!serde_json::from_value::<AppSettings>(value).unwrap().background_enabled);
    }

    #[test]
    fn ipatool_metadata_version_reads_both_output_shapes() {
        // This is the label source the version picker falls back to when the
        // native Store signer cannot run, so both ipatool output modes count.
        assert_eq!(
            ipatool_metadata_version(
                r#"{"displayVersion":"8.0.1","externalVersionID":"889765096","success":true}"#,
                "889765096"
            ),
            "8.0.1"
        );
        assert_eq!(
            ipatool_metadata_version(
                "INF displayVersion=10.4 externalVersionID=918273645 success=true",
                "918273645"
            ),
            "10.4"
        );
        // No label available: the caller must see the build number unchanged so
        // it is not cached as if it were a marketing version.
        assert_eq!(
            ipatool_metadata_version("Error: invalid response: empty songList", "889765096"),
            "889765096"
        );
    }

    #[test]
    fn two_factor_channel_marker_is_parsed() {
        // Our ipatool fork prints the channel marker to stderr before the
        // 2FA prompt; Signum turns it into an explicit log + user hint.
        assert_eq!(
            two_factor_channel("[2fa] channel=trusted-device: Apple pushed the code"),
            Some("trusted-device")
        );
        assert_eq!(
            two_factor_channel("[2fa] channel=sms: Apple sent the code by SMS"),
            Some("sms")
        );
        // Upstream ipatool (v4) prints no marker at all.
        assert_eq!(two_factor_channel("2FA code is required"), None);
        // Format path: the friendly message must mention the channel when
        // known and stay valid (SIGNUM_2FA_REQUIRED prefix) when unknown.
        let known = format_login_error(
            "[2fa] channel=sms: Apple sent the code by SMS\n2FA code is required",
            "pw",
        );
        assert!(known.starts_with("SIGNUM_2FA_REQUIRED"));
        assert!(known.contains("SMS"));
        let unknown = format_login_error("2FA code is required; supply a code", "pw");
        assert!(unknown.starts_with("SIGNUM_2FA_REQUIRED"));
    }

    #[test]
    fn account_version_match_uses_exact_id_when_catalog_provides_one() {
        let without_id = serde_json::json!({ "version": "1.2.3" });
        assert!(version_available_on_account(
            &without_id,
            &["123456".into()]
        ));
        assert!(!version_available_on_account(&without_id, &[]));

        let matching = serde_json::json!({ "external_version_id": "123456" });
        assert!(version_available_on_account(
            &matching,
            &["123456".into(), "999999".into()]
        ));

        let different = serde_json::json!({ "external_version_id": "654321" });
        assert!(!version_available_on_account(
            &different,
            &["123456".into()]
        ));
    }

    #[test]
    fn unanswerable_list_versions_is_not_treated_as_not_owned() {
        // Broken session / pending code / unreadable keychain: ipatool never
        // reached the Store, so ownership is unknown (None), not "not owned".
        for raw in [
            r#"11:50PM ERR error="auth code is required" success=false"#,
            r#"{"level":"error","error":"failed to get account: failed to get item: illegal base64 data at input byte 0","success":false}"#,
            r#"{"level":"error","error":"failed to get account: failed to get item: The specified item could not be found in the keyring","success":false}"#,
            "Not logged in.",
        ] {
            assert_eq!(parse_ipatool_list_versions(raw), None, "misread: {raw}");
        }
        // A real "no license" answer still means an empty list.
        assert_eq!(
            parse_ipatool_list_versions("Error: invalid response: empty songList"),
            Some(Vec::new())
        );
        // ipatool-cpp prints this for an unowned app. Treat it as a conclusive
        // negative so each catalog row does not launch the same probe again.
        let missing_license = "Error: you must purchase this app first.\nRun: ipatool search to find the bundle ID, then: ipatool purchase -b BUNDLE_ID\nOr re-run with --purchase to acquire the license automatically.";
        assert_eq!(parse_ipatool_list_versions(missing_license), Some(Vec::new()));
        assert!(is_not_purchased_store_error(missing_license));
        assert_eq!(parse_ipatool_list_versions(&format!("Not logged in.\n{missing_license}")), None);
    }

    #[test]
    fn empty_song_list_reads_as_not_purchased() {
        // Apple's wording for "no license on this Apple ID". Seen on both the
        // list-versions and the download path.
        for raw in [
            "Error: invalid response: empty songList",
            "Download error: invalid response: empty songList",
        ] {
            assert!(is_not_purchased_store_error(raw), "missed: {raw}");
            let friendly = friendly_apple_store_error(raw)
                .unwrap_or_else(|| panic!("no friendly text for: {raw}"));
            // Plain wording only — no ipatool/songList/internal names in the UI.
            let lowered = friendly.to_ascii_lowercase();
            assert!(!lowered.contains("songlist"));
            assert!(!lowered.contains("ipatool"));
        }
        // Must not swallow unrelated failures.
        assert!(!is_not_purchased_store_error("dial tcp: connection reset"));
    }

    #[test]
    fn purchaseable_store_apps_pass_ownership_gate() {
        let base = CatalogInstallInput {
            title: "App".into(),
            bundle_id: "com.example".into(),
            app_store_id: Some("123".into()),
            in_store: true,
            min_ios: None,
            version: None,
            external_version_id: None,
            already_owned: false,
            temporary_us_region: false,
            temporary_region: None,
        };
        // Store listing + not owned yet = purchase flow, must pass.
        assert!(require_confirmed_catalog_ownership(&base).is_ok());
        // Explicitly owned / chosen region switch also pass.
        let owned = CatalogInstallInput {
            already_owned: true,
            ..base.clone()
        };
        assert!(require_confirmed_catalog_ownership(&owned).is_ok());
        let region = CatalogInstallInput {
            temporary_region: Some("USA".into()),
            ..base.clone()
        };
        assert!(require_confirmed_catalog_ownership(&region).is_ok());
        // Delisted / history restore without ownership is the only real block.
        let restored = CatalogInstallInput {
            in_store: false,
            already_owned: false,
            ..base
        };
        assert!(require_confirmed_catalog_ownership(&restored).is_err());
    }

    #[test]
    fn a_failed_region_restore_is_never_silent() {
        // Job fine, restore failed: the account is sitting in a foreign
        // storefront, so both facts have to reach the user.
        let left_abroad =
            combine_region_outcome(Ok(()), Err("страна осталась США".into()), "Файл сохранён")
                .unwrap_err();
        assert!(left_abroad.contains("Файл сохранён"));
        assert!(left_abroad.contains("страна осталась США"));
        assert!(left_abroad.contains("Аккаунт"));
        // Both failed: neither message may be dropped.
        let both = combine_region_outcome(
            Err("не удалось скачать".into()),
            Err("страна осталась США".into()),
            "Файл сохранён",
        )
        .unwrap_err();
        assert!(both.contains("не удалось скачать"));
        assert!(both.contains("страна осталась США"));
        // Only the job failed: plain error, no region noise added.
        assert_eq!(
            combine_region_outcome(Err("нет сети".into()), Ok(()), "Файл сохранён"),
            Err("нет сети".to_string())
        );
        assert!(combine_region_outcome(Ok(()), Ok(()), "Файл сохранён").is_ok());
    }

    #[test]
    fn a_granted_licence_is_not_a_purchase_failure() {
        // ipatool-cpp, appstore.cpp: FAILURE_ALREADY_PURCHASED and a bare 500
        // both surface as this. Exit code 1, but the licence is on the account.
        assert!(is_already_owned_purchase_result(
            "Purchase error: license already exists"
        ));
        // Go wording, and its exit-0 line.
        assert!(is_already_owned_purchase_result("app already purchased"));
        assert!(is_already_owned_purchase_result(
            "7:32PM INF alreadyOwned=true success=true"
        ));
        // A real failure stays a failure.
        assert!(!is_already_owned_purchase_result("Purchase error: app not found"));
        assert!(!is_already_owned_purchase_result("license is required"));
    }

    #[test]
    fn the_raw_log_is_attached_once() {
        let _detailed = with_detailed_errors(true);
        // What format_process_error already produced, headline and all.
        let already_formatted =
            "Приложение не найдено.

Подробный лог:
Download error: storefront blocked";
        let out = with_process_log("Не удалось скачать «Мобио».", already_formatted);
        assert_eq!(out.matches("Подробный лог:").count(), 1);
        assert!(out.ends_with("Download error: storefront blocked"));
        assert!(!out.contains("Приложение не найдено."));
        // A raw line still gets its block.
        let raw = with_process_log("Не удалось скачать.", "exit=1 :: boom");
        assert!(raw.contains("Подробный лог:"));
        assert!(raw.ends_with("exit=1 :: boom"));
        // Nothing to add.
        assert_eq!(with_process_log("База", "База"), "База");
        assert_eq!(with_process_log("База", "   "), "База");
        // Замок уже у нас — второй with_detailed_errors встал бы сам на себя.
        DETAILED_ERRORS_ENABLED.store(false, Ordering::Relaxed);
        assert_eq!(with_process_log("База", "exit=1"), "База");
    }

    #[test]
    fn a_withdrawn_app_says_so_instead_of_blaming_the_region() {
        // Apple's own dialog text, captured live for adamId 6789069556.
        let raw = "Download error: “SpeakaBoo” No Longer Ava";
        assert!(is_withdrawn_from_sale_error(raw));
        assert!(is_withdrawn_from_sale_error(
            "dialogId MZCommerce.ProductNotForSale.WithItemName"
        ));
        // It must win over the storefront wording, which would send the user
        // hunting for a region problem that does not exist.
        let message = friendly_apple_store_error(raw).unwrap();
        assert!(message.contains("снято с продажи"));
        assert!(!message.contains("страны"));
        // A genuinely region-blocked app keeps its own message.
        let region = "license was granted but download was blocked";
        assert!(!is_withdrawn_from_sale_error(region));
        assert!(friendly_apple_store_error(region).unwrap().contains("вашей страны"));
    }

    #[test]
    fn an_owned_app_outside_the_storefront_is_told_apart() {
        // ipatool-cpp, appstore.cpp: jingleDocType == "purchaseSuccess" with an
        // empty songList. Owning it is not enough to list the history.
        let raw = "Error: app is not available for download in your region/storefront                    (license was granted but download was blocked)";
        assert!(is_region_locked_owned_app_error(raw));
        // An empty list for an app simply not bought yet is a different story.
        assert!(!is_region_locked_owned_app_error("Error: license is required"));
        assert!(!is_region_locked_owned_app_error(
            "Error: invalid response: empty songList"
        ));
        // Both wordings still read as "no listing" for the region classifier.
        assert!(is_app_not_found_store_error(raw));

        // The probe answers the ownership question only when no particular
        // build was asked about — that is the shape the install job uses.
        assert!(version_external_id(&JsonValue::Null).is_none());
        assert_eq!(
            version_external_id(&serde_json::json!({"external_version_id": "889765096"})),
            Some("889765096".to_string())
        );
        // An empty list on its own still means "not owned".
        assert!(!version_available_on_account(&JsonValue::Null, &[]));
        assert!(version_available_on_account(
            &JsonValue::Null,
            &["889765096".to_string()]
        ));
    }

    #[test]
    fn cpp_region_wording_reads_as_a_missing_listing() {
        assert!(is_app_not_found_store_error(
            "Error: app is not available for download in your region/storefront"
        ));
        // Go wording still lands in the same bucket.
        assert!(is_app_not_found_store_error("app not found"));
        assert!(!is_app_not_found_store_error("license is required"));
    }

    #[test]
    fn purchase_only_offers_app_id_to_the_engine_that_has_it() {
        // Go builds have no -i on `purchase`; ipatool-cpp does. Guarding on the
        // engine keeps a guaranteed "unknown shorthand flag: 'i' in -i" run off
        // every purchase on macOS and Windows alike.
        assert!(is_cpp_ipatool("apple-tools/macos-x64/v5/ipatool"));
        assert!(!is_cpp_ipatool(
            "tools/macos-x64/ipatool-v2-60403b20f6c6d7dc"
        ));
        assert!(is_cpp_ipatool("tools/macos-x64/ipatool-v5-1.0.0"));
    }

    #[test]
    fn a_missing_licence_reads_as_not_purchased() {
        // ipatool.h: struct LicenseRequired : IpaError("license is required")
        assert!(is_not_purchased_store_error("Error: license is required"));
        assert!(is_not_purchased_store_error(
            "7:17PM ERR error=\"license is required\" success=false"
        ));
        // The wordings already handled stay handled.
        assert!(is_not_purchased_store_error("invalid response: empty songList"));
        assert!(!is_not_purchased_store_error("Error: password token is expired"));
        // An unowned app must yield "no versions", not an unreadable-list error.
        assert_eq!(
            parse_ipatool_list_versions("Error: license is required"),
            Some(Vec::new())
        );
    }

    #[test]
    fn a_failure_reason_survives_the_progress_counter() {
        // Так и выглядел журнал у человека: тысячи строк счётчика, а причина —
        // последней строкой. Раньше показ обрезался по началу, и в окне ошибки
        // не было ничего, кроме «PROGRESS:0 PROGRESS:0 …».
        assert!(is_progress_only_line("PROGRESS:0"));
        assert!(is_progress_only_line("PROGRESS:100"));
        assert!(!is_progress_only_line("PROGRESS:7 failed to open file"));
        assert!(!is_progress_only_line("Error: license is required"));

        let mut noisy = String::new();
        for pct in 0..3000 {
            noisy.push_str(&format!("PROGRESS:{}\n", pct % 100));
        }
        noisy.push_str("Error: license is required");
        let shown = condense_log(&noisy, 12_000);
        assert!(shown.contains("license is required"));
        assert!(!shown.contains("PROGRESS:"));

        // Даже без счётчика длинный журнал режется по началу, а не по концу.
        let long = format!("{}\nError: license is required", "x".repeat(20_000));
        assert!(condense_log(&long, 100).contains("license is required"));
    }

    #[test]
    fn a_newer_vc_redist_is_not_a_failure() {
        // 1638 установщик возвращает, когда в системе уже стоит версия новее.
        // Считать это отказом значит показывать ошибку там, где всё в порядке.
        assert!(vc_redist_install_succeeded(0));
        assert!(vc_redist_install_succeeded(1638));
        // 3010 — поставилось, просит перезагрузку. Пакет уже на месте.
        assert!(vc_redist_install_succeeded(3010));
        // 1223 — человек отказался в запросе Windows, ставить нечего.
        assert!(!vc_redist_install_succeeded(1223));
        assert!(!vc_redist_install_succeeded(1603));
        assert!(!vc_redist_install_succeeded(-1));
    }

    #[test]
    fn download_progress_reads_both_engine_formats() {
        // ipatool-cpp on a piped stdout.
        assert_eq!(extract_percent("PROGRESS:0"), Some(0.0));
        assert_eq!(extract_percent("PROGRESS:42"), Some(42.0));
        assert_eq!(extract_percent("PROGRESS:100"), Some(100.0));
        // Go engines keep their percent bar.
        assert_eq!(
            extract_percent("downloading  42% |####      | (50/119 MB)"),
            Some(42.0)
        );
        assert_eq!(extract_percent("Загрузка 7,5%"), Some(7.5));
        assert_eq!(extract_percent("no progress here"), None);
        // The label the UI shows must follow the cpp line too.
        assert_eq!(
            parse_download_progress("PROGRESS:37").map(|(pct, _, _)| pct),
            Some(37.0)
        );
    }

    #[test]
    fn cpp_engine_counts_as_a_store_signer() {
        // v5 ships no sapsigner.exe next to it, but signs Store requests itself.
        // Without this, Signum falls back to its native Store client and has to
        // re-authenticate, which is what produced "App Store вернул страницу входа".
        assert!(ipatool_signs_store_requests(
            "apple-tools/windows-x64/v5/ipatool.exe"
        ));
        // A Go engine with no helper beside it genuinely cannot sign.
        assert!(!ipatool_signs_store_requests(
            "apple-tools/windows-x64/v3-legacy/ipatool.exe"
        ));
    }

    #[test]
    fn opaque_apple_login_status_stops_engine_fallback() {
        for status in [301, 204, 403, 404, 500] {
            assert!(is_apple_opaque_auth_response(&format!(
                "Login error: something went wrong (status={status})"
            )));
        }
        assert!(is_apple_opaque_auth_response(
            "authentication request failed after 3 attempts (HTTP 204, 404, 404): apple returned no usable authentication response"
        ));
        assert!(!is_apple_opaque_auth_response("incorrect password"));
        assert!(!is_apple_opaque_auth_response("2FA code is required"));
        // Код из «немых» должен относиться ко входу, а не встретиться где попало.
        assert!(!is_apple_opaque_auth_response("download failed: HTTP 404"));
        // Обычный отказ со статусом 200 — прежнее поведение: пробуем второй движок.
        assert!(!is_apple_opaque_auth_response(
            "Login error: something went wrong (status=200)"
        ));
    }

    #[test]
    fn opaque_apple_login_is_recognised_without_detailed_errors() {
        // Обычный режим: технический хвост отрезан, от вывода движка остаётся
        // только готовая фраза. Именно её видит login_catalog_with_fallback —
        // по ней он и должен остановить переход на второй движок.
        let _detailed = with_detailed_errors(false);
        for raw in [
            "Login error: something went wrong (status=403)",
            "Login error: something went wrong (status=500)",
            "authentication request failed after 3 attempts (HTTP 204, 404, 404): Apple returned no usable authentication response",
        ] {
            let shown = format_login_error(raw, "secret-password");
            assert!(shown.starts_with(APPLE_UNAVAILABLE_MARKER), "{shown}");
            assert!(is_apple_opaque_auth_response(&shown), "{shown}");
            assert!(!shown.contains("status="), "технический хвост попал в обычный режим");
            // Пароль не обвиняем и к коду 2FA не отсылаем.
            assert!(shown.contains("Пароль менять не нужно"), "{shown}");
            assert!(!is_fatal_account_error(&shown), "{shown}");
            assert!(!is_two_factor_error(&shown), "{shown}");
        }
        let plain = format_login_error("Login error: something went wrong (status=200)", "x");
        assert!(plain.starts_with("SIGNUM_LOGIN_OPAQUE:"), "{plain}");
        assert!(!is_apple_opaque_auth_response(&plain));
        let wrong = format_login_error("Login error: Your Apple ID or password is incorrect.", "x");
        assert!(!is_apple_opaque_auth_response(&wrong), "{wrong}");
    }

    #[test]
    fn saved_login_error_carries_no_internal_marker() {
        assert_eq!(
            without_login_marker(&format!("{APPLE_UNAVAILABLE_MARKER} Apple сейчас не принимает вход.")),
            "Apple сейчас не принимает вход."
        );
        assert_eq!(without_login_marker("SIGNUM_LOGIN_OPAQUE: Apple отклонил вход."), "Apple отклонил вход.");
        assert_eq!(without_login_marker("Неверный Apple ID или пароль."), "Неверный Apple ID или пароль.");
    }

    #[test]
    fn opaque_apple_status_reports_the_code_the_engine_gave_up_on() {
        assert_eq!(apple_opaque_auth_status("login error: something went wrong (status=301)"), Some(301));
        assert_eq!(
            apple_opaque_auth_status("authentication request failed after 3 attempts (http 500, 204, 404)"),
            Some(404)
        );
        assert_eq!(apple_opaque_auth_status("login error: rate limited by apple (http 429): <html>"), None);
        assert_eq!(apple_opaque_auth_status("list-versions error: http 500"), None);
    }

    /// Счётчик отмены KBSync общий на процесс — тесты с ним идут по одному.
    static KBSYNC_CANCEL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn kbsync_cancel_survives_overlapping_commands() {
        let _serial = KBSYNC_CANCEL_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(!kbsync_cancel_requested());
        let logout = KbsyncCancelGuard::new();
        assert!(kbsync_cancel_requested());
        // Вторая команда (вход во время выхода, двойной щелчок).
        let login = KbsyncCancelGuard::new();
        drop(logout);
        // Первая закончилась, вторая ещё работает — отмена обязана держаться.
        assert!(kbsync_cancel_requested(), "отмена снята раньше времени");
        drop(login);
        assert!(!kbsync_cancel_requested());
    }

    #[test]
    fn kbsync_cancel_guard_forgets_presence_check() {
        let _serial = KBSYNC_CANCEL_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *kbsync_presence_memo().lock().unwrap() = Some(KbsyncPresence {
            email: "user@example.com".into(),
            ready: true,
            checked_at: Instant::now(),
        });
        drop(KbsyncCancelGuard::new());
        assert!(kbsync_presence_memo().lock().unwrap().is_none());
    }

    #[test]
    fn kbsync_presence_is_asked_once_per_account() {
        let checked_at = Instant::now();
        let known = |ready: bool| KbsyncPresence {
            email: "user@example.com".into(),
            ready,
            checked_at,
        };
        let soon = checked_at + Duration::from_secs(5);
        let later = checked_at + KBSYNC_PRESENCE_RETRY + Duration::from_secs(1);
        assert!(kbsync_presence_needs_check(None, "user@example.com", soon));
        // Blob на месте — больше не спрашиваем, сколько бы ни прошло.
        assert!(!kbsync_presence_needs_check(Some(&known(true)), "user@example.com", later));
        // Другой аккаунт — спрашиваем заново.
        assert!(kbsync_presence_needs_check(Some(&known(true)), "other@example.com", soon));
        // Отказ помним недолго.
        assert!(!kbsync_presence_needs_check(Some(&known(false)), "user@example.com", soon));
        assert!(kbsync_presence_needs_check(Some(&known(false)), "user@example.com", later));
    }

    #[test]
    fn kbsync_stamp_is_touched_only_by_a_real_generation() {
        // Фон зовёт движок с --refresh: blob порождён заново при любом ответе.
        assert!(kbsync_result_is_fresh_generation(KbsyncWarm::RefreshIfStale, "generated"));
        assert!(kbsync_result_is_fresh_generation(KbsyncWarm::RefreshIfStale, "cache"));
        // Перед опросом версий --refresh нет: «cache» — blob просто прочитан.
        assert!(kbsync_result_is_fresh_generation(KbsyncWarm::OnlyIfMissing, "generated"));
        assert!(!kbsync_result_is_fresh_generation(KbsyncWarm::OnlyIfMissing, "cache"));
    }

    /// Убитый процесс оставил потомка, который держит канал вывода открытым.
    /// Раньше чтение ждало конца потомка — на KBSync это минуты.
    #[cfg(unix)]
    #[tokio::test]
    async fn killed_child_output_is_not_awaited_forever() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "echo started; sleep 30 & wait"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let output = Arc::new(StdMutex::new(Vec::<String>::new()));
        let mut out_task =
            tokio::spawn(consume_stream(child.stdout.take().unwrap(), output.clone(), None));
        let mut err_task =
            tokio::spawn(consume_stream(child.stderr.take().unwrap(), output.clone(), None));
        // Дождаться первой строки: оболочка запущена и уже породила sleep.
        for _ in 0..50 {
            if !output.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(output.lock().unwrap().as_slice(), ["started"]);
        let started = Instant::now();
        child.kill().await.unwrap();
        drain_killed_child_output(&mut out_task, &mut err_task).await;
        assert!(
            started.elapsed() < KILLED_CHILD_DRAIN + Duration::from_secs(3),
            "ждали осиротевшего потомка {:?}",
            started.elapsed()
        );
    }

    /// Отмена должна остановить уже запущенный процесс KBSync, а не только
    /// не дать стартовать новому. Вместо движка — оболочка с потомком, который
    /// живёт ещё полминуты и держит канал вывода: так ведёт себя kbsync,
    /// собранный отдельным файлом.
    ///
    /// Запускается вручную: `run_process_inner` перед запуском приводит в
    /// порядок настоящую папку сессии ipatool, а обычным тестам чужие файлы
    /// трогать незачем.  `cargo test -- --ignored kbsync_cancel_stops`
    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "трогает настоящую папку сессии ipatool; запускать вручную"]
    async fn kbsync_cancel_stops_a_running_process() {
        let _serial = KBSYNC_CANCEL_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Последний аргумент достаётся оболочке как $0 — по нему
        // run_process_inner и узнаёт запуск KBSync.
        let args: Vec<String> = ["-c", "sleep 30 & wait", "kbsync"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let started = Instant::now();
        let run = tokio::spawn(async move {
            run_process_inner("/bin/sh", &args, Duration::from_secs(60), None, true, false).await
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!run.is_finished(), "без отмены процесс должен работать");
        let logout = KbsyncCancelGuard::new();
        let result = run.await.unwrap();
        drop(logout);
        assert_eq!(result, Err(KBSYNC_CANCELLED.to_string()));
        assert!(
            started.elapsed() < Duration::from_secs(6),
            "отмена заняла {:?}",
            started.elapsed()
        );
        // Отмена снята — следующий запуск идёт до конца.
        let quick: Vec<String> = ["-c", "echo ok", "kbsync"].into_iter().map(str::to_string).collect();
        let output = run_process_inner("/bin/sh", &quick, Duration::from_secs(10), None, true, false)
            .await
            .unwrap();
        assert_eq!(output.trim(), "ok");
    }

    #[test]
    fn go_engine_is_staged_with_its_signer_and_reused() {
        let root = std::env::temp_dir().join(format!("signum-stage-{}", Uuid::new_v4()));
        // Как в Signum.app: движок в v4, подписчик в v2.
        let v4 = root.join("bundle").join("macos-arm64").join("v4");
        let v2 = root.join("bundle").join("macos-arm64").join("v2");
        std::fs::create_dir_all(&v4).unwrap();
        std::fs::create_dir_all(&v2).unwrap();
        std::fs::write(v4.join("ipatool"), b"engine-one").unwrap();
        std::fs::write(v4.join("libhelper.dylib"), b"library").unwrap();
        std::fs::write(v2.join("sapsigner"), b"signer").unwrap();
        let stage = root.join("Application Support").join("tools").join("staged").join("v4");

        let staged = stage_engine_with_signer(&v4.join("ipatool"), &v2.join("sapsigner"), &stage).unwrap();
        assert_eq!(staged, stage.join("ipatool"));
        assert_eq!(std::fs::read(&staged).unwrap(), b"engine-one");
        assert_eq!(std::fs::read(stage.join("sapsigner")).unwrap(), b"signer");
        assert_eq!(std::fs::read(stage.join("libhelper.dylib")).unwrap(), b"library");
        // Движок в новой папке — по-прежнему Go-движок, и подписчик у него рядом.
        let staged_text = staged.to_string_lossy().into_owned();
        assert!(!is_cpp_ipatool(&staged_text));
        assert_eq!(
            sap_helper_path_for(&staged_text).as_deref(),
            Some(stage.join("sapsigner").to_string_lossy().as_ref())
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in ["ipatool", "sapsigner"] {
                let mode = std::fs::metadata(stage.join(name)).unwrap().permissions().mode();
                assert_eq!(mode & 0o111, 0o111, "{name} должен запускаться");
            }
        }

        // Второй вызов ничего не переписывает: метка того же размера на месте.
        std::fs::write(&staged, b"engine-XXX").unwrap();
        stage_engine_with_signer(&v4.join("ipatool"), &v2.join("sapsigner"), &stage).unwrap();
        assert_eq!(std::fs::read(&staged).unwrap(), b"engine-XXX");

        // Приложение обновилось — движок другой, папка обновляется.
        std::fs::write(v4.join("ipatool"), b"engine-two-longer").unwrap();
        stage_engine_with_signer(&v4.join("ipatool"), &v2.join("sapsigner"), &stage).unwrap();
        assert_eq!(std::fs::read(&staged).unwrap(), b"engine-two-longer");

        // Пропавший из папки файл возвращается.
        std::fs::remove_file(stage.join("sapsigner")).unwrap();
        stage_engine_with_signer(&v4.join("ipatool"), &v2.join("sapsigner"), &stage).unwrap();
        assert_eq!(std::fs::read(stage.join("sapsigner")).unwrap(), b"signer");

        // Нет подписчика — честная ошибка, а не полупустая папка.
        assert!(stage_engine_with_signer(&v4.join("ipatool"), &v2.join("missing"), &stage).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ideviceinstaller_install_command_is_recognised_on_any_path_style() {
        let install = ["-u".to_string(), "00008030-000000000000002E".into(), "install".into(), "app.ipa".into()];
        assert!(is_ideviceinstaller_install_command(r"C:\Signum\support\ideviceinstaller.exe", &install));
        assert!(is_ideviceinstaller_install_command("/opt/signum/support/ideviceinstaller", &install));
        assert!(is_ideviceinstaller_install_command("ideviceinstaller", &["upgrade".into(), "app.ipa".into()]));
        assert!(!is_ideviceinstaller_install_command("ideviceinstaller.exe", &["list".into(), "--user".into()]));
        assert!(!is_ideviceinstaller_install_command(r"C:\Signum\v5\ipatool.exe", &install));
    }

    #[test]
    fn ideviceinstaller_upload_done_is_not_a_finished_install() {
        fn lines(text: &str) -> Vec<String> {
            text.lines().map(str::to_string).collect()
        }
        // Вывод из отчёта пользователя: IPA залит, команда установки ушла,
        // ответа телефона ещё нет. Это НЕ успех.
        let uploaded = "WARNING: could not locate iTunesMetadata.plist in archive!\n\
                        WARNING: could not locate Payload/App.app/SC_Info/App.sinf in archive!\n\
                        DONE.\nInstalling 'com.example.app'";
        assert!(!ideviceinstaller_install_completed(&lines(uploaded)));
        assert!(!ideviceinstaller_install_completed(&lines(
            "Copying 'app.ipa' to device... DONE.\nInstalling 'com.example.app'\nInstall: CreatingStagingDirectory (5%)\nInstall: InstallingApplication (60%)"
        )));
        // Отказ телефона приходит после «Installing» — успехом он не станет.
        assert!(!ideviceinstaller_install_completed(&lines(&format!(
            "{uploaded}\nERROR: Install failed. Got error \"APIInternalError\" with code 0xe8008001: verification failed"
        ))));
        assert!(!ideviceinstaller_install_completed(&lines(&format!(
            "{uploaded}\nInstall: Complete\nERROR: Install failed. Got error \"ApplicationVerificationFailed\"."
        ))));
        // Настоящее окончание: статус Complete от installd.
        assert!(ideviceinstaller_install_completed(&lines(&format!(
            "{uploaded}\nInstall: CreatingStagingDirectory (5%)\nInstall: GeneratingApplicationMap (90%)\nInstall: Complete"
        ))));
        assert!(ideviceinstaller_install_completed(&lines("Upgrading 'com.example.app'\nUpgrade: Complete")));
        assert!(ideviceinstaller_install_completed(&lines("Installing 'com.example.app'\nInstall - Complete")));
        assert!(!ideviceinstaller_install_completed(&[]));
    }

    #[test]
    fn upgrade_retry_follows_a_real_install_refusal_only() {
        let _detailed = with_detailed_errors(false);
        let refused = format_process_error(
            "ideviceinstaller.exe",
            128,
            "Installing 'com.example.app'\nERROR: Install failed. Got error \"ApplicationAlreadyInstalled\".",
        );
        assert!(install_failure_allows_upgrade_retry(&refused, false));
        // Отменённое задание, пропавший телефон, таймаут и незапустившийся
        // инструмент второй заход не исправит.
        assert!(!install_failure_allows_upgrade_retry(&refused, true));
        assert!(!install_failure_allows_upgrade_retry(&device_not_found_message(), false));
        let timeout = format!("{PROCESS_TIMEOUT_PREFIX} (360 с). Проверьте подключение и повторите попытку.");
        assert!(is_process_timeout_error(&timeout));
        assert!(!install_failure_allows_upgrade_retry(&timeout, false));
        assert!(!install_failure_allows_upgrade_retry(
            "Не удалось запустить ideviceinstaller.exe: отказано в доступе",
            false
        ));
    }

    #[test]
    fn timed_out_install_is_not_excused_by_sinf_warnings() {
        // Подробные ошибки включены: к таймауту приклеен вывод процесса.
        let _detailed = with_detailed_errors(true);
        let output = "WARNING: could not locate iTunesMetadata.plist in archive!\n\
                      WARNING: could not locate Payload/App.app/SC_Info/App.sinf in archive!\n\
                      Copying 'app.ipa' to device... DONE.\nInstalling 'com.example.app'";
        let timeout = with_technical_detail(
            &format!("{PROCESS_TIMEOUT_PREFIX} (360 с). Проверьте подключение и повторите попытку."),
            output,
        );
        assert!(timeout.contains(".sinf"), "в тексте должен быть вывод процесса");
        assert!(is_process_timeout_error(&timeout));
        assert!(!is_sinf_warning_only_install_output(&timeout));
        // Сами по себе предупреждения по-прежнему безобидны.
        assert!(is_sinf_warning_only_install_output(output));
        DETAILED_ERRORS_ENABLED.store(false, Ordering::Relaxed);
    }

    #[test]
    fn install_error_prefers_the_answer_that_names_a_cause() {
        let generic = "Не удалось выполнить операцию. Проверьте Apple ID, подключение iPhone и повторите попытку.".to_string();
        let specific = "iPhone отклонил приложение при проверке. Переподключите устройство и повторите установку.".to_string();
        let timeout = format!("{PROCESS_TIMEOUT_PREFIX} (360 с). Проверьте подключение и повторите попытку.");
        assert_eq!(pick_install_error(generic.clone(), specific.clone()), specific);
        assert_eq!(pick_install_error(specific.clone(), generic.clone()), specific);
        assert_eq!(pick_install_error(generic.clone(), timeout), generic);
        assert_eq!(
            pick_install_error(specific, device_not_found_message()),
            device_not_found_message()
        );
    }

    #[test]
    fn timed_out_install_counts_only_when_the_same_build_is_on_the_phone() {
        let on_phone = |version: Option<&str>| -> HashMap<String, InstalledApp> {
            HashMap::from([(
                "com.example.app".to_string(),
                InstalledApp {
                    bundle_id: "com.example.app".into(),
                    name: Some("Example".into()),
                    version: version.map(str::to_string),
                    app_store_id: None,
                    is_placeholder: false,
                },
            )])
        };
        assert!(installed_app_matches_ipa(&on_phone(Some("2.4.1")), "com.example.app", Some("2.4.1")));
        // Стояла прежняя версия — значит, новая не встала.
        assert!(!installed_app_matches_ipa(&on_phone(Some("2.3.0")), "com.example.app", Some("2.4.1")));
        assert!(!installed_app_matches_ipa(&on_phone(Some("2.4.1")), "com.other.app", Some("2.4.1")));
        // Версию не узнать — не гадаем.
        assert!(!installed_app_matches_ipa(&on_phone(None), "com.example.app", Some("2.4.1")));
        assert!(!installed_app_matches_ipa(&on_phone(Some("2.4.1")), "com.example.app", None));
        assert!(!installed_app_matches_ipa(&HashMap::new(), "com.example.app", Some("2.4.1")));

        let fresh = on_phone(Some("2.4.1"));
        let old = on_phone(Some("2.3.0"));
        let empty = HashMap::new();
        let landed = |before: Option<&HashMap<String, InstalledApp>>, after| {
            timed_out_install_landed(before, after, "com.example.app", Some("2.4.1"))
        };
        // Не было — появилось; стояла старая — обновилась.
        assert!(landed(Some(&empty), &fresh));
        assert!(landed(Some(&old), &fresh));
        // Та же версия стояла и до установки (перевыпуск подписи): по списку
        // не отличить новую копию от старой — успехом не считаем.
        assert!(!landed(Some(&fresh), &fresh));
        // Не встало.
        assert!(!landed(Some(&empty), &empty));
        assert!(!landed(Some(&old), &old));
        // Снимка до установки нет — не гадаем.
        assert!(!landed(None, &fresh));
    }

    #[test]
    fn windows_guards_name_what_to_turn_off() {
        let off = WindowsGuards { memory_integrity: false, smart_app_control: false };
        assert!(off.advice().is_none(), "когда мешать нечему, советовать нечего");

        let hvci = WindowsGuards { memory_integrity: true, smart_app_control: false };
        let text = hvci.advice().expect("включённая защита обязана дать совет");
        assert!(text.contains("Целостность памяти"));
        assert!(text.contains("перезагрузить"), "без перезагрузки настройка не вступит в силу");
        assert!(!text.contains("Smart App Control"), "не советуем трогать то, что и так выключено");

        let both = WindowsGuards { memory_integrity: true, smart_app_control: true };
        let text = both.advice().expect("две включённые защиты — два шага");
        assert!(text.contains("1. ") && text.contains("2. "));
    }

    #[test]
    fn a_bundle_id_clash_explains_itself() {
        // Флаг подробных ошибок — глобальный, и соседние тесты его переключают.
        // Один раз это уже уронило сборку прямо перед отправкой установщика,
        // поэтому здесь задаём его явно, а не полагаемся на порядок запуска.
        let _detailed = with_detailed_errors(false);
        let raw = "ERROR: Install failed. Got error \"BundleIDConflictWithOtherIdentifier\" with code 0x00";
        let message = format_process_error("ideviceinstaller.exe", 128, raw);
        assert!(message.contains("Удалите его с телефона"));
        assert!(!message.contains("BundleIDConflict"));
        // The plain "already installed" case keeps its own wording.
        assert!(
            format_process_error("ideviceinstaller.exe", 1, "ApplicationAlreadyInstalled")
                .contains("уже установлено")
        );
    }

    #[test]
    fn windows_major_is_read_from_ver_in_any_language() {
        assert_eq!(windows_major_from_ver("\r\nMicrosoft Windows [Version 6.3.9600]\r\n"), Some(6));
        assert_eq!(windows_major_from_ver("Microsoft Windows [Версия 10.0.19045.4529]"), Some(10));
        assert_eq!(windows_major_from_ver("garbage"), None);
    }

    #[test]
    fn an_untrusted_iphone_is_explained_not_hidden() {
        let _detailed = with_detailed_errors(false);
        // What device discovery now returns instead of an empty list.
        let raw = "ERROR: Could not connect to lockdownd: Pairing dialog response pending (-19)";
        let message = format_process_error("ideviceinfo.exe", 1, raw);
        assert!(message.contains("Доверять"));
        assert!(!is_device_missing_error(&message));
        let locked = "ERROR: Could not connect to lockdownd: Password protected (-17)";
        assert!(format_process_error("ideviceinfo.exe", 1, locked).contains("Разблокируйте"));
    }

    #[test]
    fn an_unreachable_apple_is_not_blamed_on_the_password() {
        let _detailed = with_detailed_errors(false);
        // Из логов 1.1.6: ipatool-cpp на Windows без связи с Apple.
        let raw = "Login error: failed to initialize SAP signer: WinHttpSendRequest failed: 12029";
        assert!(format_login_error(raw, "secret").contains("Нет связи с серверами Apple"));
        assert!(format_process_error("ipatool.exe", 1, raw).contains("Нет связи с серверами Apple"));
    }

    #[test]
    fn a_corrupt_download_says_so() {
        let _detailed = with_detailed_errors(false);
        // ipatool-cpp 1.1.1, appstore.cpp verify_and_report().
        let raw = "Error: download hash mismatch after re-download (expected md5 abc, got def)";
        assert!(format_process_error("ipatool.exe", 1, raw).contains("скачался повреждённым"));
    }

    #[test]
    fn a_libcurl_jar_is_recognised_as_the_cpp_engines() {
        let jar = b"# Netscape HTTP Cookie File
# https://curl.se/docs/http-cookies.html
";
        assert!(ipatool_cookie_bytes_are_libcurl(jar));
        assert!(ipatool_cookie_bytes_are_libcurl(b"# HTTP Cookie File
"));
        // A Go jar and an empty file are not cpp's.
        assert!(!ipatool_cookie_bytes_are_libcurl(
            b"[{\"Name\":\"mzf_dr\",\"Domain\":\"apple.com\"}]"
        ));
        assert!(!ipatool_cookie_bytes_are_libcurl(b""));
        assert!(!ipatool_cookie_bytes_are_libcurl(b"[]"));
        // Short reads must not panic on the 64-byte window.
        assert!(!ipatool_cookie_bytes_are_libcurl(b"#"));
    }

    #[test]
    fn cpp_session_files_are_told_apart_from_go_ones() {
        // Go engine: JWE under keyring/.
        assert!(!ipatool_account_bytes_are_cpp(
            b"eyJhbGciOiJkaXIiLCJlbmMiOiJBMjU2R0NNIn0..aaaa.bbbb"
        ));
        // `auth export` diagnostics sidecar.
        assert!(!ipatool_account_bytes_are_cpp(b"{\"email\":\"a@b.c\"}"));
        assert!(!ipatool_account_bytes_are_cpp(b""));
        // ipatool-cpp: binary blob whose first byte is the format version.
        assert!(ipatool_account_bytes_are_cpp(&[
            0x02, 0x10, 0x00, 0x00, 0x00, 0x3e, 0xda, 0x26
        ]));

        // Why the guard in ipatool_cookie_file_is_corrupt exists: the jar
        // libcurl writes for cpp looks corrupt to the JSON-shaped Go check, and
        // rewriting it to "[]" wiped a session that had just been created.
        let netscape = b"# Netscape HTTP Cookie File
.apple.com	TRUE	/	TRUE	0	mz_at0	x
";
        assert!(ipatool_cookie_bytes_are_corrupt(netscape));
        // A real Go jar and an empty one still classify as before.
        assert!(!ipatool_cookie_bytes_are_corrupt(b"[{\"Name\":\"mz_at0\"}]"));
        assert!(ipatool_cookie_bytes_are_corrupt(b"{}"));
    }

    #[test]
    fn v5_is_recognised_as_the_cpp_engine() {
        // Bundled layout and the name the tool updater writes.
        // Backslashes only split path components on Windows.
        #[cfg(windows)]
        assert!(is_cpp_ipatool(
            r"C:\Program Files\Signum\resources\apple-tools\windows-x64\v5\ipatool.exe"
        ));
        assert!(is_cpp_ipatool("/Applications/Signum.app/Contents/Resources/apple-tools/macos-arm64/v5/ipatool"));
        assert!(is_cpp_ipatool("/home/u/.cache/signum/tools/ipatool-v5-1.0.0"));
        assert!(is_cpp_ipatool("linux_x64/ipatool-v5"));
        // Every Go engine must keep --non-interactive, v3-legacy included.
        assert!(!is_cpp_ipatool("apple-tools/windows-x64/v4/ipatool.exe"));
        assert!(!is_cpp_ipatool("apple-tools/windows-x64/v3-legacy/ipatool.exe"));
        assert!(!is_cpp_ipatool("apple-tools/windows-x64/v2/ipatool.exe"));
        assert!(!is_cpp_ipatool("ipatool"));
        // Not an ipatool at all.
        assert!(!is_cpp_ipatool("apple-tools/windows-x64/v5/anisette.exe"));
    }

    #[test]
    fn v6_mac_is_local_unicast_and_does_not_repeat() {
        let mut taken = HashSet::new();
        for _ in 0..32 {
            let mac = mint_local_mac(&taken);
            assert_eq!(mac.len(), 12);
            assert!(mac_is_stored(&mac), "{mac}");
            assert!(taken.insert(mac));
        }
    }

    #[test]
    fn v6_serial_looks_like_a_modern_mac_and_does_not_repeat() {
        let mut taken = HashSet::new();
        for _ in 0..32 {
            let serial = mint_apple_serial(&taken);
            assert!(serial_is_stored(&serial), "{serial}");
            assert!(!serial.contains('I') && !serial.contains('O'));
            assert!(taken.insert(serial));
        }
    }

    #[test]
    fn v5_leads_the_engine_chain() {
        assert_eq!(IPATOOL_ENGINE_ORDER, &["v5", "v4"]);
        assert_eq!(normalize_ipatool_engine("V5"), "v5");
        let chain = || vec!["v5", "v4", "v3", "v2"].into_iter().map(String::from).collect::<Vec<_>>();
        assert_eq!(promote_login_engine(chain(), "auto", None), ["v5", "v4", "v3", "v2"]);
    }

    #[test]
    fn auto_probes_native_chain() {
        // Канонический порядок, промоушен владельца сессии, приоритет явного
        // выбора и невыдумывание отсутствующего движка.
        let engines = || vec!["v5".to_string(), "v4".to_string()];
        assert_eq!(promote_login_engine(engines(), "auto", None), ["v5", "v4"]);
        // Явный выбор важнее записанного владельца и не выбрасывает остальные.
        assert_eq!(promote_login_engine(engines(), "v4", Some("v5")), ["v4", "v5"]);
        assert_eq!(promote_login_engine(engines(), "v5", Some("v4")), ["v5", "v4"]);
        // Отсутствующий движок не выдумывается: порядок остаётся прежним.
        assert_eq!(
            promote_login_engine(vec!["v5".to_string()], "v4", None),
            ["v5"]
        );
        // Сохранённый «v6» — это теперь «авто»: впереди владелец сессии.
        assert_eq!(promote_login_engine(engines(), "v6", Some("v4")), ["v4", "v5"]);
        // Убранный движок больше не поднимается наверх, даже если он записан
        // владельцем сессии от прошлой версии.
        assert_eq!(promote_login_engine(engines(), "auto", Some("v2")), ["v5", "v4"]);
        assert_eq!(promote_login_engine(engines(), "auto", Some("v6")), ["v5", "v4"]);
    }

    #[test]
    fn proxy_url_is_built_for_both_curl_and_reqwest() {
        let base = || AppSettings {
            proxy_enabled: true,
            proxy_kind: "socks5".into(),
            proxy_host: "127.0.0.1".into(),
            proxy_port: 1080,
            proxy_user: String::new(),
            proxy_password: String::new(),
            ..Default::default()
        };
        // Схема с «h» на конце: имена узлов разрешает прокси, а не эта машина.
        // Ради обхода блокировки по имени прокси обычно и включают, поэтому
        // socks5:// ломал ровно тот случай, ради которого настройка и нужна.
        assert_eq!(proxy_url(&base()).as_deref(), Some("socks5h://127.0.0.1:1080"));

        // Выключенный прокси не должен просачиваться в окружение.
        let off = AppSettings { proxy_enabled: false, ..base() };
        assert_eq!(proxy_url(&off), None);

        // Без адреса или порта адрес не собирается: иначе в ALL_PROXY уехал бы
        // мусор вроде "socks5://:0" и все запросы легли бы разом.
        let no_host = AppSettings { proxy_host: "  ".into(), ..base() };
        assert_eq!(proxy_url(&no_host), None);
        let no_port = AppSettings { proxy_port: 0, ..base() };
        assert_eq!(proxy_url(&no_port), None);

        // Неизвестный вид приводится к socks5, а не подставляется как есть.
        let weird = AppSettings { proxy_kind: "magic".into(), ..base() };
        assert_eq!(proxy_url(&weird).as_deref(), Some("socks5h://127.0.0.1:1080"));

        // socks4 тоже с удалённым разрешением имён.
        let four = AppSettings { proxy_kind: "socks4".into(), ..base() };
        assert_eq!(proxy_url(&four).as_deref(), Some("socks4a://127.0.0.1:1080"));

        // Логин и пароль кодируются: символы вроде @ и : иначе разорвали бы URL.
        let auth = AppSettings {
            proxy_kind: "http".into(),
            proxy_user: "user@mail".into(),
            proxy_password: "p:s@ss".into(),
            ..base()
        };
        assert_eq!(
            proxy_url(&auth).as_deref(),
            Some("http://user%40mail:p%3As%40ss@127.0.0.1:1080")
        );
    }

    #[test]
    fn engine_names_normalize_to_known_values() {
        for (raw, want) in [
            ("v4", "v4"),
            ("V4", "v4"),
            (" v5 ", "v5"),
            ("v5", "v5"),
            ("auto", "auto"),
            ("", "auto"),
            // Убранные движки приводятся к «авто», иначе сохранённый у
            // пользователя выбор закрепил бы движок, которого больше нет.
            ("v2", "auto"),
            ("v3", "auto"),
            ("v6", "auto"),
            ("V6", "auto"),
            // Unknown / future value must degrade to auto, not pin a missing engine.
            ("v7", "auto"),
            ("nonsense", "auto"),
        ] {
            assert_eq!(normalize_ipatool_engine(raw), want, "raw: {raw:?}");
        }
    }

    /// Снятые движки должны быть именно те, которых нет в живом списке.
    /// Если когда-нибудь вернут v3, этот тест упадёт раньше, чем приложение
    /// начнёт снимать вход у тех, у кого он рабочий.
    #[test]
    fn retired_engines_are_absent_from_the_live_order() {
        for retired in ["v2", "v3"] {
            assert!(
                !IPATOOL_ENGINE_ORDER.contains(&retired),
                "{retired} снят, но всё ещё в IPATOOL_ENGINE_ORDER"
            );
        }
        for live in IPATOOL_ENGINE_ORDER {
            assert!(
                !matches!(*live, "v2" | "v3"),
                "{live} в живом списке, а вход с него снимается как чужой"
            );
        }
    }

    #[test]
    fn auto_login_follows_the_session_owner() {
        // Auto starts with whichever engine owns the session, if any.
        assert_eq!(engine_to_try_first("auto", None), None);
        assert_eq!(engine_to_try_first("auto", Some("v4")), Some("v4".to_string()));
        assert_eq!(engine_to_try_first("auto", Some("v5")), Some("v5".to_string()));
        // Владелец сессии от прошлой версии (v2, v6) ничего не закрепляет.
        assert_eq!(engine_to_try_first("auto", Some("v2")), None);
        assert_eq!(engine_to_try_first("auto", Some("v6")), None);
        // A stale/garbage owner does not change the auto preference.
        assert_eq!(engine_to_try_first("auto", Some("auto")), None);
        assert_eq!(engine_to_try_first("auto", Some("nonsense")), None);
        // An explicit choice always wins over the recorded owner.
        // Явный выбор важнее записанного владельца сессии.
        assert_eq!(
            engine_to_try_first("v4", Some("v5")),
            Some("v4".to_string())
        );
        assert_eq!(
            engine_to_try_first("v5", Some("v4")),
            Some("v5".to_string())
        );
        // Убранный движок (v2, v6) приводится к «авто» и идёт за владельцем.
        assert_eq!(engine_to_try_first("v2", Some("v5")), Some("v5".to_string()));
        assert_eq!(engine_to_try_first("v2", None), None);
        assert_eq!(engine_to_try_first("v6", Some("v5")), Some("v5".to_string()));
    }

    #[test]
    fn version_picker_keeps_latest_and_newest_ids_only() {
        let ids = vec![
            "100".into(),
            "500".into(),
            "200".into(),
            "900".into(),
            "300".into(),
        ];
        assert_eq!(
            take_recent_version_ids(ids, Some("200"), 3),
            vec!["200".to_string(), "900".to_string(), "500".to_string()]
        );
        assert_eq!(
            take_recent_version_ids(vec!["9".into(), "8".into(), "7".into()], None, 2),
            vec!["9".to_string(), "8".to_string()]
        );
    }

    #[test]
    fn list_versions_parser_only_accepts_successful_responses() {
        let output = concat!(
            "some log line\n",
            r#"{"success":true,"externalVersionIdentifiers":[123,"456"]}"#,
            "\n"
        );
        assert_eq!(
            parse_ipatool_list_versions(output),
            Some(vec![String::from("123"), String::from("456")])
        );
        assert_eq!(
            parse_ipatool_list_versions(r#"{"success":false,"externalVersionIdentifiers":[123]}"#),
            None
        );
    }

    #[test]
    fn list_versions_parser_reads_multiline_json_but_rejects_failed_payloads() {
        let multiline = r#"log before JSON
{
  "success": true,
  "externalVersionIdentifiers": [
    { "externalVersionId": 123456 },
    "789012"
  ]
}
"#;
        assert_eq!(
            parse_ipatool_list_versions(multiline),
            Some(vec![String::from("123456"), String::from("789012")])
        );
        let failed = r#"warning externalVersionIdentifiers=["111111"]
{"success":false,"externalVersionIdentifiers":[222222]}"#;
        assert_eq!(parse_ipatool_list_versions(failed), None);
        // A failed check must not become a cached "not on this Apple ID".
        assert_eq!(parse_ipatool_list_versions(r#"{"success":false,"error":"network request failed"}"#), None);
        assert_eq!(parse_ipatool_list_versions(r#"{"success":false,"error":"you must purchase this app first"}"#), Some(Vec::new()));
    }

    #[test]
    fn successful_exit_with_2fa_message_is_not_a_successful_login() {
        let output = "2FA code is required; run the command again and supply a code using the `--auth-code` flag";
        assert!(is_two_factor_error(output));
        assert!(format_login_error(output, "secret").starts_with("SIGNUM_2FA_REQUIRED:"));
    }

    #[test]
    fn macos_ipatool_two_factor_auth_code_required_is_detected() {
        let output = "Error: two-factor auth code required. Retry with --auth-code CODE";
        assert!(is_two_factor_error(output));
        assert!(format_login_error(output, "secret").starts_with("SIGNUM_2FA_REQUIRED:"));
    }

    #[test]
    fn mac_stdin_2fa_prompt_is_required() {
        assert!(is_two_factor_error("INF enter 2FA code:"));
        assert!(is_two_factor_error(
            "failed to read string: EOF"
        ));
        assert!(format_login_error("enter 2FA code:", "secret").starts_with("SIGNUM_2FA_REQUIRED:"));
        assert!(two_factor_required_login_error().starts_with("SIGNUM_2FA_REQUIRED:"));
    }

    #[test]
    fn ipatool_cpp_login_error_is_not_success() {
        assert!(login_output_indicates_failure(
            "Login error: something went wrong (status=403)"
        ));
    }

    #[test]
    fn generic_login_failure_is_not_treated_as_2fa_required() {
        let mapped = format_login_error("Login error: unexpected xyz failure 42", "secret");
        assert!(!is_two_factor_error(&mapped));
        assert!(!mapped.to_ascii_lowercase().contains("signum_2fa_required"));
    }

    #[test]
    fn bare_2fa_substring_is_not_enough_for_required() {
        assert!(!is_two_factor_error(
            "Не удалось войти. Проверьте email, пароль и интернет."
        ));
        // Help text alone must not force the 2FA form.
        assert!(!is_two_factor_error("use --auth-code later if needed"));
    }

    #[test]
    fn login_error_maps_locked_account() {
        let output =
            "Login error: GSA init error -20209: This Apple Account has been locked for security reasons. Visit iForgot";
        let mapped = format_login_error(output, "secret");
        assert!(mapped.to_ascii_lowercase().contains("iforgot") || mapped.contains("заблокировал"));
    }

    #[test]
    fn login_error_maps_disabled_account() {
        let mapped = format_login_error("Login error: account is disabled", "secret");
        assert!(mapped.to_ascii_lowercase().contains("отключ") || mapped.contains("disabled"));
    }

    #[test]
    fn login_error_maps_wrong_password() {
        let mapped = format_login_error("Login error: incorrect password", "mypass");
        assert!(mapped.contains("Неверный Apple ID или пароль"));
        assert!(!mapped.contains("mypass"));
    }

    #[test]
    fn account_lock_and_password_change_stop_engine_fallback() {
        assert!(is_fatal_account_error(
            "This Apple Account has been locked for security reasons"
        ));
        assert!(is_fatal_account_error("Apple says password must be changed"));
        assert!(is_fatal_account_error("Please reset your password at iforgot"));
        assert!(!is_fatal_account_error("temporary network timeout"));
        assert!(!is_fatal_account_error("anisette helper failed to start"));
    }

    #[test]
    fn login_error_unknown_is_friendly_without_stack() {
        let _detailed = with_detailed_errors(false);
        let mapped = format_login_error("Login error: unexpected xyz failure 42", "secret");
        assert!(!mapped.to_ascii_lowercase().contains("unexpected xyz"));
        assert!(mapped.contains("Не удалось войти"));
    }

    #[test]
    fn corrupt_cookies_panic_is_mapped_without_stack() {
        let _detailed = with_detailed_errors(false);
        let output = "panic: cannot load cookies: invalid character '#' looking for beginning of value goroutine 1 [running]";
        let mapped = format_session_verify_error(output);
        assert!(!mapped.contains("goroutine"));
        assert!(!mapped.contains("Технически"));
        assert!(mapped.contains("повреждён") || mapped.contains("Войти снова"));
        let login = format_login_error(output, "secret");
        assert!(!login.contains("goroutine"));
        assert!(is_corrupt_cookie_error(output));
    }

    #[test]
    fn cookie_object_and_json_account_errors_are_mapped_without_dump() {
        let _detailed = with_detailed_errors(true);
        let dump = r#"2026/08/28 22:07:36 warning: discarding cookies in invalid format (error: json: cannot unmarshal object into Go value of type []cookiejar.entry) {"level":"error","error":"failed to get account: failed to get item: illegal base64 data at input byte 0","success":false,"time":"2026-08-28T22:07:36+03:00"}"#;
        assert!(is_corrupt_cookie_error(dump));
        assert!(is_keychain_integrity_error(dump));
        let mapped = format_session_verify_error(dump);
        assert!(mapped.contains("Войти снова") || mapped.contains("повреждён"));
        assert!(!mapped.contains("cookiejar"));
        assert!(!mapped.contains("illegal base64"));
        assert!(!mapped.contains("Технически"));
        // Замок уже у нас — второй with_detailed_errors встал бы сам на себя.
        DETAILED_ERRORS_ENABLED.store(false, Ordering::Relaxed);
        assert!(ipatool_cookie_bytes_are_corrupt(b"{}\n"));
        assert!(ipatool_cookie_bytes_are_corrupt(b"# Netscape HTTP Cookie File\n"));
        assert!(!ipatool_cookie_bytes_are_corrupt(b"[]\n"));
        assert!(!ipatool_cookie_bytes_are_corrupt(b""));
        assert!(!ipatool_cookie_bytes_are_corrupt(br#"{"idmsa.apple.com":[{"Name":"myacinfo"}]}"#));
        assert!(ipatool_account_bytes_are_json_sidecar(b"{\"email\":\"a@b.c\"}\n"));
        assert!(!ipatool_account_bytes_are_json_sidecar(b"eyJhbGciOiJkaXIiLCJlbmMiOiJBMjU2R0NNIn0..abc"));
    }

    #[test]
    fn auth_info_parser_reads_json_after_cookie_warning() {
        let dump = r#"2026/08/28 22:07:36 warning: discarding cookies in invalid format (error: json: cannot unmarshal object into Go value of type []cookiejar.entry) {"level":"error","error":"failed to get account: failed to get item: illegal base64 data at input byte 0","success":false,"time":"2026-08-28T22:07:36+03:00"}"#;
        let parsed = parse_ipatool_auth_info_email(dump);
        assert!(parsed.unwrap_err().contains("illegal base64"));
    }

    #[test]
    fn store_variants_do_not_count_as_archived_alternatives() {
        let selected = serde_json::json!({
            "app_store_id": "100",
            "in_store": true
        });
        let versions = vec![
            selected.clone(),
            serde_json::json!({
                "app_store_id": "200",
                "in_store": true
            }),
        ];

        assert!(!catalog_has_alternative_versions(&versions, &selected));
    }

    #[test]
    fn archived_version_counts_as_an_install_alternative() {
        let selected = serde_json::json!({
            "app_store_id": "100",
            "in_store": true
        });
        let versions = vec![
            selected.clone(),
            serde_json::json!({
                "app_store_id": "300",
                "in_store": false
            }),
        ];

        assert!(catalog_has_alternative_versions(&versions, &selected));
    }

    #[test]
    fn in_store_is_the_only_app_store_availability_flag() {
        let removed = serde_json::json!({
            "in_store": false,
            "source": "app_store"
        });
        let available = serde_json::json!({
            "in_store": true,
            "source": "removed"
        });

        assert!(!version_is_store_offer(&removed));
        assert!(version_is_store_offer(&available));
    }

    #[test]
    fn managed_tool_download_path_only_accepts_the_selected_target_and_safe_names() {
        assert_eq!(
            validated_tool_download_path(
                "windows-x64",
                "anisette",
                "files/tools/windows-x64/anisette-3.0.0.exe"
            ),
            Some("files/tools/windows-x64/anisette-3.0.0.exe".into())
        );
        assert!(validated_tool_download_path(
            "windows-x64",
            "anisette",
            "files/tools/linux-x64/anisette-3.0.0.exe"
        )
        .is_none());
        assert!(validated_tool_download_path(
            "windows-x64",
            "anisette",
            "files/tools/windows-x64/../anisette.exe"
        )
        .is_none());
        assert!(validated_tool_download_path(
            "windows-x64",
            "unknown-tool",
            "files/tools/windows-x64/unknown.exe"
        )
        .is_none());
    }

    #[test]
    fn managed_tool_checksum_requires_exact_hex_digest() {
        assert!(valid_sha256_hex(&"a".repeat(64)));
        assert!(!valid_sha256_hex(&"a".repeat(63)));
        assert!(!valid_sha256_hex(&format!("{}z", "a".repeat(63))));
        assert!(valid_managed_tool_file_name("ipatool-v2-3.9.9.2.exe"));
        assert!(!valid_managed_tool_file_name("../ipatool.exe"));
        assert!(!valid_managed_tool_file_name("ipatool/evil.exe"));
    }
}

/// Linux desktops hand WebKitGTK a GPU path that often is not usable, and an
/// AppImage additionally shadows the host GLib. Both produce a window that
/// never paints, so settle them before anything touches GTK.
#[cfg(target_os = "linux")]
fn prepare_linux_runtime() {
    // Without this WebKit tries the DMA-BUF renderer, fails on
    // DRM_IOCTL_MODE_CREATE_DUMB / GBM (NVIDIA, VMs, locked-down seats) and
    // shows an empty grey window with no error of its own.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    // Inside an AppImage the bundled GLib is older than the host, so host GIO
    // modules (gvfs, dconf) fail to resolve their symbols and spam the console.
    // We use none of them; pointing GIO at an empty directory skips the lot.
    // Only inside the AppImage — a deb install must keep host integration.
    if std::env::var_os("APPIMAGE").is_some() && std::env::var_os("GIO_MODULE_DIR").is_none() {
        std::env::set_var("GIO_MODULE_DIR", "/nonexistent");
    }
}

#[cfg(not(target_os = "linux"))]
fn prepare_linux_runtime() {}

fn main() {
    // The detached support uploader also uses HTTPS, but exits before the
    // Tauri builder is created. Install rustls' provider before that early path.
    let _ = rustls::crypto::ring::default_provider().install_default();
    if diag_log::run_pending_upload_mode() {
        return;
    }
    prepare_linux_runtime();
    uninstall::capture_args();
    if uninstall::is_mode() && uninstall::is_silent() {
        uninstall::run_silent();
        return;
    }
    // isideload (iloader) error reporting for Apple GSA / Developer network errors.
    let _ = isideload::init();
    let queue = Arc::new(InstallQueue::default());
    let startup_queue = queue.clone();
    let state = AppState {
        account_cache_owner: StdMutex::new(None),
        version_page_cache: StdMutex::new(HashMap::new()),
        jobs: Arc::new(Mutex::new(HashMap::new())),
        queue,
        ipatool_lock: Arc::new(Mutex::new(())),
        cancel_flags: Arc::new(StdMutex::new(HashMap::new())),
        background_mode: Arc::new(AtomicBool::new(false)),
        account_version_cache: Arc::new(StdMutex::new(HashMap::new())),
        version_probe_cache: Arc::new(StdMutex::new(HashMap::new())),
        version_picker_cache: Arc::new(StdMutex::new(HashMap::new())),
        history_2fa_pending: Arc::new(StdMutex::new(None)),
        region_2fa_pending: Arc::new(StdMutex::new(None)),
        signing_2fa_reply: Arc::new(StdMutex::new(None)),
        login_2fa_engine: Arc::new(StdMutex::new(HashMap::new())),
        bank_otp: Arc::new(bank_otp::BankOtpStore::default()),
        tool_update_lock: Arc::new(Mutex::new(())),
    };
    let mut builder = tauri::Builder::default();
    // Первым среди плагинов — так требует сам плагин.
    //
    // Signum уходит в трей, окна не видно, и человек жмёт ярлык снова. Раньше
    // это запускало ещё один процесс, и у каждого был свой значок в трее —
    // у людей набиралось по семь штук. Теперь вторая копия отдаёт запуск
    // первой и закрывается. Деинсталлятор не трогаем: он работает тем же
    // файлом и не должен упираться в уже запущенное приложение.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if !uninstall::is_mode() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Автозапуск приходит с --background: окно показывать не надо,
            // иначе оно выскочит человеку в лицо при входе в систему.
            if argv.iter().any(|arg| arg == "--background") {
                return;
            }
            show_window(app);
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state)
        .setup(move |app| {
            if uninstall::is_mode() {
                uninstall::configure_window(app)?;
                return Ok(());
            }
            diag_log::on_startup(&app.handle());
            refresh_v6_identity(&app.handle(), None);
            if diag_log::is_active() {
                diag_log::note(
                    "env",
                    &format!("arch={} os={}", std::env::consts::ARCH, std::env::consts::OS),
                );
                ipatool_debug_log(&ipatool_session_diag());
            }
            setup_tray(&app.handle())?;
            if drop_retired_engine_session(&app.handle()) {
                LEGACY_LOGOUT_NOTICE.store(true, Ordering::SeqCst);
            }
            // Запущены автозапуском — работаем из трея, окно не показываем.
            let silent_start = std::env::args().any(|arg| arg == "--background");
            app.state::<AppState>().background_mode.store(silent_start, Ordering::Relaxed);
            if load_settings(&app.handle())
                .map(|s| s.background_enabled)
                .unwrap_or(false)
            {
                // Запись автозапуска могли снести чистильщиком или другой
                // сборкой — восстанавливаем по настройке, она главнее.
                let _ = set_autostart(true);
                start_background_refresh(&app.handle());
            }
            if let Some(window) = app.get_webview_window("main") {
                if silent_start {
                    let _ = window.hide();
                }
                #[cfg(target_os = "macos")]
                {
                    // On macOS with custom titlebar (decorations: false), use overlay style
                    // so traffic lights are visible and draggable area works well.
                    let _ = window.set_title_bar_style(tauri::TitleBarStyle::Overlay);
                }
                if !silent_start {
                    restore_window_state(&window);
                }
                let window_for_events = window.clone();
                window.on_window_event(move |event| match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        let app_handle = window_for_events.app_handle();
                        if has_active_ipa_work(&app_handle)
                            || load_settings(&app_handle).map(|s| s.background_enabled).unwrap_or(false)
                        {
                            api.prevent_close();
                            hide_window_to_tray(&app_handle);
                        } else {
                            persist_window_state(&window_for_events);
                            diag_log::on_exit(&app_handle);
                        }
                    }
                    tauri::WindowEvent::Destroyed => {
                        persist_window_state(&window_for_events);
                    }
                    _ => {}
                });
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(resume_pending_jobs_on_startup(
                handle.clone(),
                startup_queue,
            ));
            tauri::async_runtime::spawn(install_worker(handle.clone()));
            tauri::async_runtime::spawn(smart_auto_updater(handle.clone()));
            tauri::async_runtime::spawn(history_session_keepalive(handle.clone()));
            tauri::async_runtime::spawn(recover_temporary_region(handle.clone()));
            tauri::async_runtime::spawn(startup_managed_tool_update(handle.clone()));
            tauri::async_runtime::spawn(startup_vc_redist_check(handle.clone()));
            tauri::async_runtime::spawn(startup_ota_check(handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            get_ipatool_engine_status,
            get_app_version_info,
            save_settings,
            set_background_mode,
            take_legacy_logout_notice,
            take_session_expired_notice,
            purchase_app_only,
            purchase_apps_on_accounts,
            get_ipatool_random_passphrase,
            regenerate_ipatool_random_passphrase,
            fetch_catalog,
            fetch_catalog_feed,
            app_transfer::preview_app_transfer,
            app_transfer::enqueue_app_transfer,
            fetch_tb_activation,
            fetch_app_info,
            fetch_app_store_versions,
            store_search::search_store_apps,
            store_search::search_store_developers,
            store_search::developer_apps,
            resolve_catalog_install,
            probe_account_restore_catalog,
            probe_account_restore_for_app,
            resolve_app_store_versions_ios,
            resolve_app_store_versions_labels,
            plan_store_region_for_app,
            toggle_favorite_app,
            list_signed_apps,
            renew_signed_app,
            list_app_updates,
            enqueue_catalog_installs,
            find_app_store_version_for_device,
            fetch_purchase_history,
            list_store_regions,
            preview_store_region_address,
            list_store_region_addresses,
            get_cached_store_region,
            get_store_region,
            change_store_region,
            login_apple,
            logout_apple,
            switch_apple_account,
            remove_apple_account,
            get_auth_status,
            enqueue_sign_and_install,
            inspect_ipa,
            submit_signing_2fa,
            get_signing_status,
            list_custom_certs,
            import_custom_cert,
            delete_custom_cert,
            bank_alt_send_sms,
            bank_alt_confirm_and_install,
            bank_alt_peek_cache,
            bank_alt_resume_from_cache,
            bank_alt_clear_cache,
            bank_alt_stop_page,
            list_connected_devices,
            select_device,
            get_device_info,
            trust_device,
            list_installed_apps,
            enqueue_placeholder_restore,
            enqueue_catalog_install,
            enqueue_catalog_export,
            enqueue_open_exports_folder,
            get_exports_dir,
            open_exports_folder,
            enqueue_local_install,
            cancel_install_job,
            retry_install_job,
            list_install_jobs,
            get_install_queue_status,
            set_install_queue_paused,
            move_install_job,
            cancel_pending_install_jobs,
            retry_failed_install_jobs,
            clear_finished_install_jobs,
            list_action_history,
            reverse_backup,
            get_cache_size,
            list_cached_ipas,
            delete_cached_ipa,
            clear_cache,
            check_for_update,
            check_tool_updates,
            download_update,
            install_downloaded_update,
            check_icloud_installed,
            ensure_icloud_dependency,
            open_icloud_manual_install,
            open_media_feature_pack,
            open_optional_features,
        diag_log::get_diag_status,
            diag_log::log_ui_action,
        diag_log::start_diag_recording,
        diag_log::stop_diag_recording,
        diag_log::send_diag_log,
            diag_log::export_diag_log,
            diag_log::report_auto_error,
            uninstall::is_uninstall_mode,
            uninstall::uninstall_info,
            uninstall::start_uninstall
        ])
        .build(tauri::generate_context!())
        .expect("error while running Signum")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                diag_log::on_exit(app);
            }
        });
}

#[cfg(test)]
mod signature_expiry_tests {
    use super::*;

    #[test]
    fn profile_expiry_becomes_comparable_seconds() {
        assert_eq!(
            parse_iso_seconds("2026-09-11T12:00:00Z"),
            Some(1_789_128_000)
        );
        assert_eq!(parse_iso_seconds("1970-01-01T00:00:01Z"), Some(1));
        // A leap day is where hand-rolled date maths usually breaks.
        assert_eq!(
            parse_iso_seconds("2024-02-29T00:00:00Z"),
            Some(1_709_164_800)
        );
    }

    #[test]
    fn unreadable_dates_do_not_pretend_to_be_valid() {
        assert_eq!(parse_iso_seconds(""), None);
        assert_eq!(parse_iso_seconds("2026-09-11"), None);
        assert_eq!(parse_iso_seconds("not a date at all"), None);
        // Before the epoch there is nothing to renew.
        assert_eq!(parse_iso_seconds("1969-01-01T00:00:00Z"), None);
    }
}
