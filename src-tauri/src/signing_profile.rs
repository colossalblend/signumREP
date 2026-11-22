//! Разбор .mobileprovision и проверки «подходит ли профиль».
//!
//! Чистая логика без сети и файлов, чтобы её можно было проверить тестами:
//! срок действия профиля и сертификата, список устройств, Bundle ID,
//! сертификаты, на которые выписан профиль.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Что нам нужно из профиля.
#[derive(Debug, Clone, Default)]
pub struct ProfileInfo {
    pub name: String,
    pub team_id: String,
    /// `application-identifier` без префикса команды: `com.foo.app`, `com.foo.*` или `*`.
    pub bundle_pattern: String,
    pub expires_unix: Option<u64>,
    pub devices: Vec<String>,
    /// Enterprise / In-House: профиль подходит любому устройству.
    pub all_devices: bool,
    /// DER сертификатов, на которые выписан профиль.
    pub developer_certs: Vec<Vec<u8>>,
}

/// Вынуть XML-plist из подписанного контейнера профиля и разобрать.
pub fn parse_profile(raw: &[u8]) -> Result<ProfileInfo, String> {
    let start = find(raw, b"<?xml")
        .or_else(|| find(raw, b"<plist"))
        .ok_or_else(|| "в файле нет описания профиля".to_string())?;
    let end = rfind(raw, b"</plist>")
        .map(|at| at + b"</plist>".len())
        .filter(|end| *end > start)
        .ok_or_else(|| "описание профиля обрезано".to_string())?;
    let value = plist::Value::from_reader_xml(&raw[start..end])
        .map_err(|e| format!("описание профиля повреждено: {e}"))?;
    let dict = value
        .as_dictionary()
        .ok_or_else(|| "описание профиля повреждено".to_string())?;

    let name = dict
        .get("Name")
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string();
    let team_id = dict
        .get("TeamIdentifier")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_string())
        .or_else(|| {
            dict.get("ApplicationIdentifierPrefix")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_string())
        })
        .unwrap_or("")
        .to_string();
    let entitlements = dict.get("Entitlements").and_then(|v| v.as_dictionary());
    let app_identifier = entitlements
        .and_then(|e| {
            e.get("application-identifier")
                .or_else(|| e.get("com.apple.application-identifier"))
        })
        .and_then(|v| v.as_string())
        .unwrap_or("*");
    let bundle_pattern = strip_team_prefix(app_identifier, &team_id);
    let expires_unix = dict
        .get("ExpirationDate")
        .and_then(|v| v.as_date())
        .and_then(|d| SystemTime::from(d).duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let devices = dict
        .get("ProvisionedDevices")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_string())
                .map(|s| s.trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let all_devices = dict
        .get("ProvisionsAllDevices")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);
    let developer_certs = dict
        .get("DeveloperCertificates")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_data())
                .map(<[u8]>::to_vec)
                .collect()
        })
        .unwrap_or_default();

    Ok(ProfileInfo {
        name,
        team_id,
        bundle_pattern,
        expires_unix,
        devices,
        all_devices,
        developer_certs,
    })
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

/// `ABCDE12345.com.foo.*` -> `com.foo.*`.
fn strip_team_prefix(app_identifier: &str, team_id: &str) -> String {
    let value = app_identifier.trim();
    if !team_id.is_empty() {
        if let Some(rest) = value.strip_prefix(team_id).and_then(|r| r.strip_prefix('.')) {
            return rest.to_string();
        }
    }
    // Префикс бывает и не равен TeamIdentifier (старые профили) — отрезаем
    // первый сегмент, если он похож на 10-символьный идентификатор.
    match value.split_once('.') {
        Some((prefix, rest))
            if prefix.len() == 10 && prefix.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            rest.to_string()
        }
        _ => value.to_string(),
    }
}

impl ProfileInfo {
    #[cfg(test)]
    pub fn is_wildcard(&self) -> bool {
        self.bundle_pattern.ends_with('*')
    }

    /// Подходит ли Bundle ID под `application-identifier` профиля.
    pub fn allows_bundle(&self, bundle_id: &str) -> bool {
        bundle_matches(&self.bundle_pattern, bundle_id)
    }

    /// Bundle ID, под которым приложение пройдёт проверку профиля.
    ///
    /// Явный профиль подходит ровно одному идентификатору — берём его.
    /// Шаблон `com.foo.*` — дописываем исходный идентификатор после префикса.
    pub fn bundle_id_for(&self, bundle_id: &str) -> String {
        if self.allows_bundle(bundle_id) {
            return bundle_id.to_string();
        }
        if let Some(prefix) = self.bundle_pattern.strip_suffix('*') {
            return format!("{prefix}{bundle_id}");
        }
        self.bundle_pattern.clone()
    }

    /// Можно ли ставить на это устройство.
    pub fn allows_device(&self, udid: &str) -> bool {
        self.all_devices
            || self
                .devices
                .iter()
                .any(|d| d.eq_ignore_ascii_case(udid.trim()))
    }

    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_unix.is_some_and(|at| at <= now)
    }

    /// Выписан ли профиль на этот сертификат. Если в профиле сертификатов нет,
    /// проверить нечего — не мешаем подписи.
    pub fn includes_certificate(&self, cert_der: &[u8]) -> bool {
        self.developer_certs.is_empty() || self.developer_certs.iter().any(|c| c == cert_der)
    }
}

pub fn bundle_matches(pattern: &str, bundle_id: &str) -> bool {
    let pattern = pattern.trim();
    if pattern == "*" {
        return true;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => bundle_id.starts_with(prefix),
        None => pattern == bundle_id,
    }
}

/// Срок действия сертификата (notAfter) в секундах Unix.
pub fn certificate_not_after(der: &[u8]) -> Option<u64> {
    use x509_cert::der::Decode;
    let cert = x509_cert::Certificate::from_der(der).ok()?;
    let not_after = cert.tbs_certificate.validity.not_after.to_system_time();
    not_after
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Подпись перестаёт работать в момент, когда истекает что-то одно: профиль
/// или сертификат.
pub fn earliest(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Секунды Unix -> `2026-09-11T12:00:00Z` (так же пишет дату сам профиль).
pub fn unix_to_iso(secs: u64) -> String {
    plist::Date::from(UNIX_EPOCH + Duration::from_secs(secs)).to_xml_format()
}

/// Срок для `DD.MM.YYYY` в сообщениях об ошибке.
pub fn unix_to_ru_date(secs: u64) -> String {
    let iso = unix_to_iso(secs);
    let date = iso.split('T').next().unwrap_or("");
    let mut parts = date.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(y), Some(m), Some(d)) => format!("{d}.{m}.{y}"),
        _ => date.to_string(),
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_bytes(app_id: &str, extra: &str) -> Vec<u8> {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Name</key><string>Test</string>
<key>TeamIdentifier</key><array><string>ABCDE12345</string></array>
<key>ExpirationDate</key><date>2030-01-02T03:04:05Z</date>
<key>Entitlements</key><dict><key>application-identifier</key><string>{app_id}</string></dict>
{extra}
</dict></plist>"#
        );
        // Имитируем CMS-обёртку вокруг plist.
        let mut raw = vec![0x30, 0x82, 0x01, 0x00, 0x06, 0x09];
        raw.extend_from_slice(xml.as_bytes());
        raw.extend_from_slice(&[0xa0, 0x00, 0x31]);
        raw
    }

    #[test]
    fn parses_wrapped_profile() {
        let raw = profile_bytes(
            "ABCDE12345.com.foo.app",
            "<key>ProvisionedDevices</key><array><string>00008110-001A</string></array>\
             <key>DeveloperCertificates</key><array><data>AQID</data></array>",
        );
        let info = parse_profile(&raw).unwrap();
        assert_eq!(info.team_id, "ABCDE12345");
        assert_eq!(info.bundle_pattern, "com.foo.app");
        assert_eq!(info.expires_unix, Some(1_893_553_445));
        assert_eq!(unix_to_iso(info.expires_unix.unwrap()), "2030-01-02T03:04:05Z");
        assert_eq!(unix_to_ru_date(info.expires_unix.unwrap()), "02.01.2030");
        assert!(info.allows_device("00008110-001a"));
        assert!(!info.allows_device("00008110-FFFF"));
        assert!(info.includes_certificate(&[1, 2, 3]));
        assert!(!info.includes_certificate(&[9]));
        assert!(!info.is_expired(1_700_000_000));
        assert!(info.is_expired(1_893_553_445));
    }

    #[test]
    fn explicit_profile_forces_its_bundle_id() {
        let info = parse_profile(&profile_bytes("ABCDE12345.com.foo.app", "")).unwrap();
        assert!(!info.is_wildcard());
        assert!(info.allows_bundle("com.foo.app"));
        assert!(!info.allows_bundle("ru.bank.app"));
        assert_eq!(info.bundle_id_for("ru.bank.app"), "com.foo.app");
    }

    #[test]
    fn wildcard_profiles() {
        let any = parse_profile(&profile_bytes("ABCDE12345.*", "")).unwrap();
        assert!(any.is_wildcard());
        assert_eq!(any.bundle_id_for("ru.bank.app"), "ru.bank.app");

        let prefixed = parse_profile(&profile_bytes("ABCDE12345.com.foo.*", "")).unwrap();
        assert!(prefixed.allows_bundle("com.foo.x"));
        assert_eq!(prefixed.bundle_id_for("ru.bank.app"), "com.foo.ru.bank.app");
    }

    #[test]
    fn enterprise_profile_allows_any_device() {
        let info = parse_profile(&profile_bytes(
            "ABCDE12345.*",
            "<key>ProvisionsAllDevices</key><true/>",
        ))
        .unwrap();
        assert!(info.allows_device("whatever"));
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(parse_profile(b"not a profile").is_err());
        assert!(parse_profile(b"<plist><dict>").is_err());
    }

    #[test]
    fn earliest_expiry_wins() {
        assert_eq!(earliest(Some(5), Some(3)), Some(3));
        assert_eq!(earliest(None, Some(3)), Some(3));
        assert_eq!(earliest(Some(5), None), Some(5));
        assert_eq!(earliest(None, None), None);
    }
}
