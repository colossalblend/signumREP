use std::path::Path;

/// Fails the build when the version or build number in `main.rs` disagrees with
/// the manifests the update server and the installers read.
///
/// Signum 1.2.3 shipped `APP_BUILD = "40"` while the release manifest announced
/// build 43. Because the server treated "same version + older build" as an
/// update, every 1.2.3 client re-downloaded and reinstalled itself for ever.
/// A compile-time check is the cheapest way to make that impossible ever again.
fn main() {
    tauri_build::build();

    let src = std::fs::read_to_string("src/main.rs").expect("read src/main.rs");
    let app_version = const_str(&src, "APP_VERSION").expect("APP_VERSION in src/main.rs");
    let app_build = const_str(&src, "APP_BUILD")
        .or_else(|| const_str(&src, "pub(crate) const APP_BUILD"))
        .expect("APP_BUILD in src/main.rs");

    assert_digits("APP_BUILD", &app_build);
    assert_digits("APP_VERSION", &app_version);

    let conf_path = Path::new("tauri.conf.json");
    let conf = std::fs::read_to_string(conf_path).expect("read tauri.conf.json");
    let conf_version = json_string(&conf, "version").expect("version in tauri.conf.json");
    if conf_version != app_version {
        panic!(
            "version mismatch: src/main.rs APP_VERSION is {app_version:?} but tauri.conf.json says {conf_version:?}. \
             Keep them equal or the updater, the installer and the UI will disagree."
        );
    }

    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=src/main.rs");
    println!("cargo:rustc-env=SIGNUM_APP_VERSION={app_version}");
    println!("cargo:rustc-env=SIGNUM_APP_BUILD={app_build}");
}

fn const_str(src: &str, name: &str) -> Option<String> {
    let at = src.find(&format!("const {name}: &str"))?;
    let rest = &src[at..];
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')? + start;
    Some(rest[start..end].to_string())
}

fn assert_digits(name: &str, value: &str) {
    if !value.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        || !value.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        panic!("{name} must be a dotted numeric string, got {value:?}");
    }
}

/// Minimal `"key": number/string` reader; the manifest is tiny and hand-written,
/// so pulling in a JSON crate just for this would be overkill.
fn json_string(json: &str, key: &str) -> Option<String> {
    let at = json.find(&format!("\"{key}\""))?;
    let rest = &json[at..];
    let colon = rest.find(':')?;
    let value = rest[colon + 1..].trim_start();
    if let Some(stripped) = value.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(stripped[..end].to_string());
    }
    let end = value.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(value.len());
    Some(value[..end].to_string())
}
