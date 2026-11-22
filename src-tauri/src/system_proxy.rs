//! Системный прокси для дочерних процессов.
//!
//! Свои запросы Signum делает через reqwest, а тот на Windows сам читает прокси
//! из настроек Интернета. ipatool-cpp построен на libcurl и смотрит только в
//! переменные окружения. Поэтому у человека с VPN в режиме «системный прокси»
//! (v2rayN, Clash, Hiddify и т. п.) каталог открывался, а вход в Apple ID шёл к
//! Apple напрямую и падал. IPA_Downloader эту разницу закрывает: перед запуском
//! ipatool он берёт системный прокси и кладёт его в HTTPS_PROXY. Делаем так же.
//!
//! Правила те же, что у IPA_Downloader:
//! - прокси берём, только если он включён и Apple не стоит в исключениях;
//! - SOCKS отдаём как socks5h, чтобы имена разрешал сам прокси;
//! - прокси используем, только если его порт отвечает: VPN-клиент мог закрыться,
//!   не вернув настройки, и тогда мёртвый адрес сломал бы все запросы.
//!
//! PAC-сценарии (AutoConfigURL) не разбираем.

use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Узел, для которого спрашиваем прокси: тот же, что берёт IPA_Downloader.
const PROBE_HOST: &str = "buy.itunes.apple.com";
/// VPN включают и выключают на ходу, поэтому ответ живёт недолго.
const CACHE_FOR: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(800);

/// Адрес системного прокси для libcurl или `None`, если его нет или он не отвечает.
pub(crate) fn system_proxy_url() -> Option<String> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Option<String>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    // Замок держим на всё время проверки: параллельные запуски ipatool иначе
    // стучались бы в реестр и в порт прокси одновременно.
    let mut guard = cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((at, value)) = guard.as_ref() {
        if at.elapsed() < CACHE_FOR {
            return value.clone();
        }
    }
    let previous = guard.as_ref().map(|(_, value)| value.clone());
    let detected = detect();
    let found = detected.clone().filter(|url| proxy_reachable(url));
    if previous.as_ref() != Some(&found) {
        let msg = match (&detected, &found) {
            (_, Some(url)) => format!("system proxy: using {}", redact(url)),
            (Some(url), None) => format!("system proxy: {} does not answer, going direct", redact(url)),
            (None, None) => "system proxy: none".to_string(),
        };
        crate::diag_log::note("app", &msg);
    }
    *guard = Some((Instant::now(), found.clone()));
    found
}

#[cfg(target_os = "windows")]
fn detect() -> Option<String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .ok()?;
    parse_windows_internet_settings(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "macos")]
fn detect() -> Option<String> {
    use std::process::{Command, Stdio};
    let output = Command::new("/usr/sbin/scutil")
        .arg("--proxy")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    parse_scutil_proxy(&String::from_utf8_lossy(&output.stdout))
}

/// На Linux системного хранилища прокси нет: настройки рабочего стола
/// выливаются в http_proxy/https_proxy, а их ipatool и так наследует.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn detect() -> Option<String> {
    None
}

/// Разбирает вывод `reg query ...\Internet Settings`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_windows_internet_settings(text: &str) -> Option<String> {
    let mut enabled = false;
    let mut server = None;
    let mut bypass = String::new();
    for line in text.lines() {
        let Some((name, value)) = reg_value(line) else {
            continue;
        };
        match name.to_ascii_lowercase().as_str() {
            "proxyenable" => enabled = parse_reg_dword(value) == Some(1),
            "proxyserver" => server = Some(value.to_string()),
            "proxyoverride" => bypass = value.to_string(),
            _ => {}
        }
    }
    if !enabled || bypassed(&bypass, PROBE_HOST) {
        return None;
    }
    parse_windows_proxy_server(&server?)
}

/// Строка `    Name    REG_TYPE    Value` → (Name, Value).
fn reg_value(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    let type_at = line.find(" REG_")?;
    let name = line[..type_at].trim();
    let rest = line[type_at..].trim_start();
    let value = rest
        .find(char::is_whitespace)
        .map(|end| rest[end..].trim())
        .unwrap_or("");
    (!name.is_empty()).then_some((name, value))
}

fn parse_reg_dword(value: &str) -> Option<u32> {
    let value = value.trim();
    match value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => value.parse().ok(),
    }
}

/// `ProxyServer` бывает двух видов: `host:port` на все протоколы или
/// `http=h:p;https=h:p;socks=h:p` по протоколам. Для HTTPS к Apple сначала
/// подходит запись https, потом http, и только потом socks — тот же порядок,
/// что у IPA_Downloader (сначала системный HTTP-прокси, затем SOCKS).
fn parse_windows_proxy_server(server: &str) -> Option<String> {
    let server = server.trim();
    if server.is_empty() {
        return None;
    }
    if !server.contains('=') {
        return normalize_proxy(server, "http");
    }
    let mut https = None;
    let mut http = None;
    let mut socks = None;
    for entry in server.split(';') {
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "https" => https = Some(value),
            "http" => http = Some(value),
            "socks" => socks = Some(value),
            _ => {}
        }
    }
    https
        .and_then(|value| normalize_proxy(value, "http"))
        .or_else(|| http.and_then(|value| normalize_proxy(value, "http")))
        .or_else(|| socks.and_then(|value| normalize_proxy(value, "socks5h")))
}

/// Разбирает вывод `scutil --proxy` на macOS.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_scutil_proxy(text: &str) -> Option<String> {
    let mut values = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(" : ") {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    let pick = |prefix: &str, scheme: &str| -> Option<String> {
        if values.get(&format!("{prefix}Enable")).map(String::as_str) != Some("1") {
            return None;
        }
        let host = values.get(&format!("{prefix}Proxy"))?;
        let port = values.get(&format!("{prefix}Port"))?;
        normalize_proxy(&format!("{host}:{port}"), scheme)
    };
    pick("HTTPS", "http")
        .or_else(|| pick("HTTP", "http"))
        .or_else(|| pick("SOCKS", "socks5h"))
}

/// `host:port` или `scheme://host:port` → адрес для libcurl.
fn normalize_proxy(raw: &str, default_scheme: &str) -> Option<String> {
    let raw = raw.trim().trim_end_matches('/');
    let (scheme, address) = match raw.split_once("://") {
        Some((scheme, address)) => {
            let scheme = match scheme.to_ascii_lowercase().as_str() {
                "socks" | "socks5" | "socks5h" => "socks5h",
                "socks4" | "socks4a" => "socks4a",
                "https" => "https",
                _ => "http",
            };
            (scheme, address)
        }
        None => (default_scheme, raw),
    };
    let (host, port) = address.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    if host.is_empty() || port == 0 || host.contains(char::is_whitespace) {
        return None;
    }
    Some(format!("{scheme}://{host}:{port}"))
}

/// `ProxyOverride`: шаблоны через `;` со звёздочками; `<local>` — узлы без точки.
fn bypassed(list: &str, host: &str) -> bool {
    list.split(';')
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .any(|pattern| {
            if pattern.eq_ignore_ascii_case("<local>") {
                !host.contains('.')
            } else {
                wildcard_match(&pattern.to_ascii_lowercase(), &host.to_ascii_lowercase())
            }
        })
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    let first = parts[0];
    if !rest.starts_with(first) {
        return false;
    }
    rest = &rest[first.len()..];
    let last = parts[parts.len() - 1];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

fn proxy_reachable(url: &str) -> bool {
    let Some((_, address)) = url.split_once("://") else {
        return false;
    };
    let address = address.rsplit_once('@').map_or(address, |(_, host)| host);
    let Ok(addrs) = address.to_socket_addrs() else {
        return false;
    };
    addrs
        .take(2)
        .any(|addr| TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok())
}

/// В журнал — без логина и пароля, если они вдруг есть в адресе.
fn redact(url: &str) -> String {
    match (url.split_once("://"), url.rsplit_once('@')) {
        (Some((scheme, _)), Some((_, host))) => format!("{scheme}://***@{host}"),
        _ => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REG_PLAIN: &str = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\r\n    CertificateRevocation    REG_DWORD    0x1\r\n    ProxyEnable    REG_DWORD    0x1\r\n    ProxyServer    REG_SZ    127.0.0.1:10809\r\n    ProxyOverride    REG_SZ    localhost;127.*;10.*;<local>\r\n";

    #[test]
    fn windows_plain_proxy_is_http() {
        assert_eq!(
            parse_windows_internet_settings(REG_PLAIN).as_deref(),
            Some("http://127.0.0.1:10809")
        );
    }

    #[test]
    fn windows_disabled_proxy_is_ignored() {
        let text = REG_PLAIN.replace("REG_DWORD    0x1\r\n    ProxyServer", "REG_DWORD    0x0\r\n    ProxyServer");
        assert_eq!(parse_windows_internet_settings(&text), None);
    }

    #[test]
    fn windows_apple_in_bypass_goes_direct() {
        let text = REG_PLAIN.replace("<local>", "*.apple.com");
        assert_eq!(parse_windows_internet_settings(&text), None);
    }

    #[test]
    fn windows_per_protocol_prefers_https_then_socks() {
        assert_eq!(
            parse_windows_proxy_server("http=1.2.3.4:80;https=1.2.3.4:443;socks=1.2.3.4:1080").as_deref(),
            Some("http://1.2.3.4:443")
        );
        assert_eq!(
            parse_windows_proxy_server("socks=127.0.0.1:7890").as_deref(),
            Some("socks5h://127.0.0.1:7890")
        );
        assert_eq!(
            parse_windows_proxy_server("socks5://127.0.0.1:7890").as_deref(),
            Some("socks5h://127.0.0.1:7890")
        );
        assert_eq!(parse_windows_proxy_server("ftp=1.2.3.4:21"), None);
        assert_eq!(parse_windows_proxy_server("127.0.0.1"), None);
    }

    #[test]
    fn macos_scutil_https_then_socks() {
        let text = "<dictionary> {\n  ExceptionsList : <array> {\n    0 : *.local\n  }\n  HTTPEnable : 0\n  HTTPSEnable : 1\n  HTTPSPort : 7890\n  HTTPSProxy : 127.0.0.1\n  SOCKSEnable : 1\n  SOCKSPort : 7891\n  SOCKSProxy : 127.0.0.1\n}\n";
        assert_eq!(parse_scutil_proxy(text).as_deref(), Some("http://127.0.0.1:7890"));
        let socks_only = text.replace("HTTPSEnable : 1", "HTTPSEnable : 0");
        assert_eq!(parse_scutil_proxy(&socks_only).as_deref(), Some("socks5h://127.0.0.1:7891"));
        assert_eq!(parse_scutil_proxy("<dictionary> {\n  HTTPEnable : 0\n}\n"), None);
    }

    #[test]
    fn wildcard_bypass() {
        assert!(bypassed("*.apple.com", "buy.itunes.apple.com"));
        assert!(bypassed("buy.*.apple.com", "buy.itunes.apple.com"));
        assert!(!bypassed("localhost;127.*;<local>", "buy.itunes.apple.com"));
        assert!(bypassed("<local>", "intranet"));
    }

    #[test]
    fn redact_hides_credentials() {
        assert_eq!(redact("socks5h://u:p@1.2.3.4:1080"), "socks5h://***@1.2.3.4:1080");
        assert_eq!(redact("http://1.2.3.4:80"), "http://1.2.3.4:80");
    }
}
