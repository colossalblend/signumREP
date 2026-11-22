pub mod app_groups;
pub mod app_ids;
pub mod certificates;
pub mod developer_session;
pub mod device_type;
pub mod devices;
pub mod teams;

// some non-alphanumeric characters cause Developer error 35: An invalid value was provided for the parameter 'appIdName'.
pub fn normalize_app_names(name: &str) -> String {
    sanitize_developer_name(name, "App")
}

/// Signum patch: Developer Portal names (`appIdName`, app group and device
/// names) only accept plain ASCII letters/digits/spaces. Display names of RU
/// apps and iPhone names are often Cyrillic or contain emoji; upstream turns
/// them into a bare "App". Keep words separated by single spaces and, when
/// nothing ASCII is left, fall back to the last reverse-DNS label of
/// `fallback` (e.g. `com.tinkoff.business` -> `Business`).
pub fn sanitize_developer_name(name: &str, fallback: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.chars().filter(|c| c.is_ascii_alphanumeric()).count() >= 2 {
        return cleaned.chars().take(50).collect::<String>().trim().to_string();
    }
    let from_fallback: String = fallback
        .rsplit('.')
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(40)
        .collect();
    if from_fallback.len() >= 2 {
        if from_fallback.chars().any(|c| c.is_ascii_uppercase()) {
            return from_fallback;
        }
        let mut chars = from_fallback.chars();
        let first = chars.next().map(|c| c.to_ascii_uppercase()).unwrap_or('A');
        format!("{first}{}", chars.as_str())
    } else {
        "SignedApp".into()
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_developer_name;

    #[test]
    fn keeps_ascii_words() {
        assert_eq!(sanitize_developer_name("My  Bank-App!", "x"), "My Bank App");
    }

    #[test]
    fn cyrillic_falls_back_to_identifier_label() {
        assert_eq!(
            sanitize_developer_name("Личный кабинет", "com.tinkoff.business"),
            "Business"
        );
        assert_eq!(sanitize_developer_name("Айфон Маши", "iPhone"), "iPhone");
        assert_eq!(sanitize_developer_name("🙂", "a"), "SignedApp");
    }
}
