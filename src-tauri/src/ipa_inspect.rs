                                                                            
                                                                   

use serde::Serialize;
use std::{
    io::{Cursor, Read},
    path::Path,
};
use zip::ZipArchive;

const MH_MAGIC: u32 = 0xFEED_FACE;
const MH_CIGAM: u32 = 0xCEFA_EDFE;
const MH_MAGIC_64: u32 = 0xFEED_FACF;
const MH_CIGAM_64: u32 = 0xCFFA_EDFE;
const FAT_MAGIC: u32 = 0xCAFE_BABE;
const FAT_CIGAM: u32 = 0xBEBA_FECA;
const FAT_MAGIC_64: u32 = 0xCAFE_BABF;
const FAT_CIGAM_64: u32 = 0xBFBA_FECA;

const LC_CODE_SIGNATURE: u32 = 0x1D;
const LC_ENCRYPTION_INFO: u32 = 0x21;
const LC_ENCRYPTION_INFO_64: u32 = 0x2C;

const CSMAGIC_EMBEDDED_SIGNATURE: u32 = 0xFADE_0CC0;
const CSMAGIC_BLOBWRAPPER: u32 = 0xFADE_0B01;
const CSSLOT_CMS_SIGNATURE: u32 = 0x10000;

#[derive(Debug, Clone, Serialize, Default)]
pub struct IpaInspect {
    pub kind: String,
    pub title: String,
    pub explanation: String,
    pub needs_sign: bool,
    pub app_name: Option<String>,
    pub version: Option<String>,
    pub bundle_id: Option<String>,
    pub min_ios: Option<String>,
    pub architectures: Vec<String>,
    pub extensions: Vec<String>,
    pub profile_expires_at: Option<String>,
    pub warnings: Vec<String>,
}

struct Slice {
    cryptid: Option<u32>,
    signed: bool,
    signature_kind: Option<String>,
    architecture: String,
}

fn u32_at(data: &[u8], off: usize, be: bool) -> Option<u32> {
    let b = data.get(off..off + 4)?;
    Some(if be {
        u32::from_be_bytes(b.try_into().ok()?)
    } else {
        u32::from_le_bytes(b.try_into().ok()?)
    })
}

fn u64_at(data: &[u8], off: usize, be: bool) -> Option<u64> {
    let b = data.get(off..off + 8)?;
    Some(if be {
        u64::from_be_bytes(b.try_into().ok()?)
    } else {
        u64::from_le_bytes(b.try_into().ok()?)
    })
}

fn looks_macho(magic: u32) -> bool {
    matches!(magic, MH_MAGIC | MH_CIGAM | MH_MAGIC_64 | MH_CIGAM_64)
}

fn looks_fat(magic: u32) -> bool {
    matches!(magic, FAT_MAGIC | FAT_CIGAM | FAT_MAGIC_64 | FAT_CIGAM_64)
}

fn signer_kind(signer: &str) -> String {
    let low = signer.to_ascii_lowercase();
    if low.contains("apple iphone os application signing")
        || low.contains("apple iphone certification authority")
        || low.contains("apple itunes store")
    {
        return "app_store".into();
    }
    if low.contains("in-house") || low.contains("enterprise") {
        return "enterprise".into();
    }
    if low.contains("iphone developer:")
        || low.contains("apple development:")
        || low.contains("iphone os application development")
    {
        return "development".into();
    }
    if low.contains("iphone distribution:") || low.contains("apple distribution:") {
        return "distribution".into();
    }
    "present".into()
}

fn signer_from_cms(cms: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(cms);
    const MARKERS: &[&str] = &[
        "Apple iPhone OS Application Signing",
        "iPhone Distribution:",
        "iPhone Developer:",
        "Apple Development:",
        "Apple Distribution:",
        "iPhone OS Application Development",
    ];
    for marker in MARKERS {
        if let Some(idx) = text.find(marker) {
            let chunk: String = text[idx..]
                .chars()
                .take(80)
                .filter(|c| *c >= ' ' && *c < '\u{7f}')
                .collect();
            let label = chunk.trim_matches(|c: char| c == '.' || c.is_whitespace());
            if !label.is_empty() {
                return Some(label.to_string());
            }
        }
    }
    None
}

fn parse_superblob(blob: &[u8]) -> Option<String> {
    if u32_at(blob, 0, true)? != CSMAGIC_EMBEDDED_SIGNATURE {
        return None;
    }
    let count = u32_at(blob, 8, true)? as usize;
    let mut signer = None;
    for i in 0..count {
        let base = 12 + i * 8;
        let slot = u32_at(blob, base, true)?;
        let off = u32_at(blob, base + 4, true)? as usize;
        let magic = u32_at(blob, off, true)?;
        let length = u32_at(blob, off + 4, true)? as usize;
        if off + length > blob.len() {
            continue;
        }
        let piece = &blob[off..off + length];
        if slot == CSSLOT_CMS_SIGNATURE || magic == CSMAGIC_BLOBWRAPPER {
            signer = signer_from_cms(piece);
        }
    }
    signer
}

fn parse_macho_slice(data: &[u8]) -> Option<Slice> {
    if data.len() < 32 {
        return None;
    }
    let magic_be = u32_at(data, 0, true)?;
    let (magic, be) = if looks_macho(magic_be) {
        (magic_be, matches!(magic_be, MH_MAGIC | MH_MAGIC_64))
    } else {
        let magic_le = u32_at(data, 0, false)?;
        if !looks_macho(magic_le) {
            return None;
        }
        (magic_le, false)
    };
    let is64 = matches!(magic, MH_MAGIC_64 | MH_CIGAM_64);
    let ncmds = u32_at(data, 16, be)? as usize;
    let sizeofcmds = u32_at(data, 20, be)? as usize;
    let header = if is64 { 32 } else { 28 };
    let end = header + sizeofcmds;
    if end > data.len() { return None; }
    let mut off = header;
    let mut slice = Slice {
        cryptid: None,
        signed: false,
        signature_kind: None,
        architecture: match u32_at(data, 4, be).unwrap_or(0) {
            12 => "armv7", 0x0100_000c => "arm64", 7 => "i386", 0x0100_0007 => "x86_64", _ => "неизвестная",
        }.into(),
    };
    for _ in 0..ncmds {
        if off + 8 > end {
            return None;
        }
        let cmd = u32_at(data, off, be)?;
        let cmdsize = u32_at(data, off + 4, be)? as usize;
        if cmdsize < 8 || off + cmdsize > end {
            return None;
        }
        if matches!(cmd, LC_ENCRYPTION_INFO | LC_ENCRYPTION_INFO_64) && cmdsize >= 20 {
            slice.cryptid = u32_at(data, off + 16, be);
        } else if cmd == LC_CODE_SIGNATURE && cmdsize >= 16 {
            let dataoff = u32_at(data, off + 8, be)? as usize;
            let datasize = u32_at(data, off + 12, be)? as usize;
            slice.signed = true;
            if dataoff + datasize <= data.len() && datasize > 12 {
                if let Some(signer) = parse_superblob(&data[dataoff..dataoff + datasize]) {
                    slice.signature_kind = Some(signer_kind(&signer));
                } else {
                    slice.signature_kind = Some("present".into());
                }
            }
        }
        off += cmdsize;
    }
    Some(slice)
}

fn parse_macho(data: &[u8]) -> Vec<Slice> {
    let Some(magic) = u32_at(data, 0, true) else {
        return Vec::new();
    };
    if looks_macho(magic) || looks_macho(u32_at(data, 0, false).unwrap_or(0)) {
        return parse_macho_slice(data).into_iter().collect();
    }
    if !looks_fat(magic) {
        return Vec::new();
    }
    let be = matches!(magic, FAT_MAGIC | FAT_MAGIC_64);
    let is64 = matches!(magic, FAT_MAGIC_64 | FAT_CIGAM_64);
    let narch = u32_at(data, 4, be).unwrap_or(0).min(16) as usize;
    let rec = if is64 { 32 } else { 20 };
    let mut slices = Vec::new();
    for i in 0..narch {
        let base = 8 + i * rec;
        let (offset, size) = if is64 {
            (
                u64_at(data, base + 8, be).unwrap_or(0) as usize,
                u64_at(data, base + 16, be).unwrap_or(0) as usize,
            )
        } else {
            (
                u32_at(data, base + 8, be).unwrap_or(0) as usize,
                u32_at(data, base + 12, be).unwrap_or(0) as usize,
            )
        };
        if offset.checked_add(32).is_none_or(|end| end > data.len()) {
            continue;
        }
        let end = if size == 0 {
            data.len()
        } else {
            offset.saturating_add(size).min(data.len())
        };
        if let Some(s) = parse_macho_slice(&data[offset..end]) {
            slices.push(s);
        }
    }
    slices
}

fn dict_str(dict: &plist::Dictionary, key: &str) -> Option<String> {
    dict.get(key)
        .and_then(|v| v.as_string())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

pub fn inspect_ipa_file(path: &Path) -> IpaInspect {
    if !path.is_file() {
        return IpaInspect {
            kind: "invalid".into(),
            title: "Файл не найден".into(),
            explanation: "Выберите файл приложения ещё раз.".into(),
            needs_sign: false,
            app_name: None,
            version: None,
            ..Default::default()
        };
    }
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return IpaInspect {
                kind: "invalid".into(),
                title: "Не удалось открыть файл".into(),
                explanation: "Проверьте, что файл не занят другой программой.".into(),
                needs_sign: false,
                app_name: None,
                version: None,
                ..Default::default()
            };
        }
    };
    let mut archive = match ZipArchive::new(file) {
        Ok(z) => z,
        Err(_) => {
            return IpaInspect {
                kind: "invalid".into(),
                title: "Это не файл приложения".into(),
                explanation: "Нужен файл .ipa с компьютера.".into(),
                needs_sign: false,
                app_name: None,
                version: None,
                ..Default::default()
            };
        }
    };

    let mut plist_name: Option<String> = None;
    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name().replace('\\', "/");
            names.push(name.clone());
            if name.starts_with("Payload/")
                && name.ends_with(".app/Info.plist")
                && name.matches('/').count() == 2
            {
                plist_name = Some(name);
            }
        }
    }
    let Some(plist_name) = plist_name else {
        return IpaInspect {
            kind: "invalid".into(),
            title: "Повреждённый файл".into(),
            explanation: "Внутри нет приложения. Выберите другой файл.".into(),
            needs_sign: false,
            app_name: None,
            version: None,
            ..Default::default()
        };
    };

    let plist_bytes = {
        let mut entry = match archive.by_name(&plist_name) {
            Ok(e) => e,
            Err(_) => {
                return IpaInspect {
                    kind: "invalid".into(),
                    title: "Повреждённый файл".into(),
                    explanation: "Не удалось прочитать приложение.".into(),
                    needs_sign: false,
                    app_name: None,
                    version: None,
                    ..Default::default()
                };
            }
        };
        let mut buf = Vec::new();
        if entry.size() > 4 * 1024 * 1024 || entry.read_to_end(&mut buf).is_err() {
            return IpaInspect { kind: "invalid".into(), title: "Повреждённый файл".into(), explanation: "Не удалось прочитать Info.plist.".into(), ..Default::default() };
        }
        buf
    };
    let dict = plist::Value::from_reader(Cursor::new(plist_bytes))
        .ok()
        .and_then(|v| v.into_dictionary());
    let (app_name, version, exe) = if let Some(ref d) = dict {
        (
            dict_str(d, "CFBundleDisplayName").or_else(|| dict_str(d, "CFBundleName")),
            dict_str(d, "CFBundleShortVersionString").or_else(|| dict_str(d, "CFBundleVersion")),
            dict_str(d, "CFBundleExecutable"),
        )
    } else {
        (None, None, None)
    };

    let Some(exe) = exe else {
        return IpaInspect {
            kind: "invalid".into(),
            title: "Повреждённый файл".into(),
            explanation: "Не удалось понять, что внутри файла.".into(),
            needs_sign: false,
            app_name,
            version,
            ..Default::default()
        };
    };
    let app_dir = plist_name.trim_end_matches("Info.plist");
    let exe_name = format!("{app_dir}{exe}");
    let exe_name = if names.iter().any(|n| n == &exe_name) {
        exe_name
    } else {
        names
            .iter()
            .find(|n| n.starts_with(app_dir) && n.ends_with(&format!("/{exe}")))
            .cloned()
            .unwrap_or(exe_name)
    };

    let macho = {
        let mut entry = match archive.by_name(&exe_name) {
            Ok(e) => e,
            Err(_) => {
                return IpaInspect {
                    kind: "invalid".into(),
                    title: "Повреждённый файл".into(),
                    explanation: "Не удалось прочитать приложение.".into(),
                    needs_sign: false,
                    app_name,
                    version,
                    ..Default::default()
                };
            }
        };
        let mut buf = Vec::new();
        if entry.size() > 512 * 1024 * 1024 || entry.read_to_end(&mut buf).is_err() {
            return IpaInspect { kind: "invalid".into(), title: "Повреждённый файл".into(), explanation: "Не удалось прочитать исполняемый файл (предел проверки — 512 МБ).".into(), ..Default::default() };
        }
        buf
    };
    let slices = parse_macho(&macho);
    if slices.is_empty() {
        return IpaInspect {
            kind: "invalid".into(),
            title: "Повреждённый файл".into(),
            explanation: "Это не приложение для iPhone.".into(),
            needs_sign: false,
            app_name,
            version,
            ..Default::default()
        };
    }

    let encrypted = slices.iter().any(|s| s.cryptid == Some(1));
    let decrypted =
        slices.iter().any(|s| s.cryptid == Some(0)) || slices.iter().all(|s| s.cryptid.is_none());
                                                                          
                                                                             
                                  
    let personally_signed = slices
        .iter()
        .any(|s| matches!(s.signature_kind.as_deref(), Some("development")));

    let (kind, title, explanation, needs_sign) = if encrypted {
        (
            "encrypted",
            "Зашифрованный",
             "Этот файл из App Store. Дополнительная подпись не нужна; для запуска может потребоваться Apple ID, на котором приложение приобретено.",
            false,
        )
    } else if personally_signed {
        (
            "signed",
            "Подписанный",
            "Файл уже готов. Его можно сразу поставить на iPhone — заново подписывать не нужно.",
            false,
        )
    } else if decrypted {
        (
            "decrypted",
            "Расшифрованный",
            "Файл нужно подписать вашим Apple ID. После этого его можно поставить на iPhone или сохранить.",
            true,
        )
    } else {
        (
            "decrypted",
            "Расшифрованный",
            "Файл нужно подписать вашим Apple ID. После этого его можно поставить на iPhone или сохранить.",
            true,
        )
    };

    let bundle_id = dict.as_ref().and_then(|dict| dict_str(dict, "CFBundleIdentifier"));
    let min_ios = dict.as_ref().and_then(|dict| dict_str(dict, "MinimumOSVersion"));
    let mut architectures = slices.iter().map(|slice| slice.architecture.clone()).collect::<Vec<_>>();
    architectures.sort(); architectures.dedup();
    let extensions = names.iter().filter(|name| name.starts_with(app_dir) && name.ends_with(".appex/Info.plist"))
        .map(|name| name.trim_start_matches(app_dir).trim_end_matches("/Info.plist").to_string()).collect::<Vec<_>>();
    let profile_expires_at = archive.by_name(&format!("{app_dir}embedded.mobileprovision")).ok().and_then(|entry| {
        if entry.size() > 4 * 1024 * 1024 { return None; }
        let mut bytes = Vec::new();
        entry.take(4 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
        crate::apple_signing::provision_expiry_from_bytes(&bytes)
    });
    let mut warnings = Vec::new();
    if encrypted { warnings.push("Для зашифрованного IPA может потребоваться Apple ID, на котором приложение приобретено.".into()); }
    if !architectures.iter().any(|architecture| architecture == "arm64") { warnings.push("Нет архитектуры arm64: современные iPhone могут не запустить приложение.".into()); }
    if let Some(expiry) = profile_expires_at.as_deref() {
        if crate::parse_iso_seconds(expiry).is_some_and(|time| time <= crate::now()) { warnings.push("Срок профиля подписи истёк. Требуется повторная подпись.".into()); }
    }
    if personally_signed { warnings.push("Наличие подписи не гарантирует её действительность и разрешение для выбранного iPhone.".into()); }
    let expired = profile_expires_at.as_deref().and_then(crate::parse_iso_seconds).is_some_and(|time| time <= crate::now());
    IpaInspect {
        kind: kind.into(),
        title: title.into(),
        explanation: explanation.into(),
        needs_sign: needs_sign || (personally_signed && expired),
        app_name,
        version,
        bundle_id, min_ios, architectures, extensions, profile_expires_at, warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn report_reads_metadata_architecture_extensions_and_profile_and_rejects_bad_binary() {
        let path = std::env::temp_dir().join(format!("signum-inspect-{}.ipa", uuid::Uuid::new_v4()));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let info = br#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.example.app</string><key>CFBundleName</key><string>Example</string><key>CFBundleShortVersionString</key><string>2.0</string><key>CFBundleExecutable</key><string>Example</string><key>MinimumOSVersion</key><string>15.0</string></dict></plist>"#;
        zip.start_file("Payload/Example.app/Info.plist", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(info).unwrap();
        let mut macho = vec![0; 32];
        macho[..4].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
        macho[4..8].copy_from_slice(&0x0100_000cu32.to_le_bytes());
        zip.start_file("Payload/Example.app/Example", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&macho).unwrap();
        zip.start_file("Payload/Example.app/PlugIns/Share.appex/Info.plist", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(info).unwrap();
        zip.start_file("Payload/Example.app/embedded.mobileprovision", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"CMS<key>ExpirationDate</key><date>2020-01-01T00:00:00Z</date>").unwrap();
        zip.finish().unwrap();
        let report = inspect_ipa_file(&path);
        assert_eq!(report.bundle_id.as_deref(), Some("com.example.app"));
        assert_eq!(report.min_ios.as_deref(), Some("15.0"));
        assert_eq!(report.architectures, ["arm64"]);
        assert_eq!(report.extensions, ["PlugIns/Share.appex"]);
        assert_eq!(report.profile_expires_at.as_deref(), Some("2020-01-01T00:00:00Z"));
        assert!(report.warnings.iter().any(|warning| warning.contains("истёк")));
        macho[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert!(parse_macho_slice(&macho).is_none());
        std::fs::write(&path, b"bad archive").unwrap();
        assert_eq!(inspect_ipa_file(&path).kind, "invalid");
        std::fs::remove_file(path).unwrap();
    }
}
