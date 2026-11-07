//! Signum uninstaller (`uninstall.exe` inside the installed app folder).
//!
//! Runs *inside* the install directory, so the last step hands the folder
//! removal to a detached cmd that waits for this process to exit.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const REG_UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum";

fn run_quiet(program: &str, args: &[&str]) {
    if let Ok(mut cmd) = Command::new(program).args(args).spawn().map_err(|e| e) {
        let _ = cmd.wait();
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

fn appdata() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

fn localappdata() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

/// Everything Signum leaves on the PC besides the install folder itself.
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(roaming) = appdata() {
        dirs.push(roaming.join("pw.appbank.signum"));
        dirs.push(roaming.join("pw.appbank.signum.setup"));
    }
    if let Some(local) = localappdata() {
        dirs.push(local.join("pw.appbank.signum"));
        dirs.push(local.join("pw.appbank.signum.setup"));
        // v4 engine caches (Unicorn runtime + Apple SAP assets).
        dirs.push(local.join("ipatool"));
    }
    if let Some(home) = home_dir() {
        dirs.push(home.join(".ipatool"));
    }
    dirs
}

/// Default download/export destination. Kept by default: the IPAs there are
/// user files, not our state. Passing `--wipe-downloads` removes it too.
fn downloads_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join("Downloads").join("Signum"))
}

fn delete_dir(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    }
}

fn delete_file(path: &Path) {
    if path.is_file() {
        let _ = fs::remove_file(path);
    }
}

/// Remove Windows Credential Manager entries written under the Signum service
/// name. `cmdkey /delete` takes one target at a time; parse `/list` output and
/// match any target containing the marker regardless of UI language.
fn wipe_signum_credentials() {
    let mut listing = Command::new("cmdkey");
    listing.arg("/list");
    let Ok(output) = listing.output() else {
        return;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if !line.contains("Signum") {
            continue;
        }
        // Typical line: "  Target: LegacyGeneric:target=Signum.apple_auth_email"
        let Some(target_pos) = line.rfind("target=") else {
            continue;
        };
        let target = line[target_pos + "target=".len()..].trim();
        if target.is_empty() || !target.contains("Signum") {
            continue;
        }
        run_quiet("cmdkey", &["/delete", target]);
    }
    let raw = line_at_any_rate(&text);
    for target in raw {
        run_quiet("cmdkey", &["/delete", &target]);
    }
}

/// Fallback catalog of known credential targets in case /list output could not
/// be parsed (older keyring crate formats included).
fn line_at_any_rate(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for username in [
        "apple_auth_email",
        "ipatool_keychain_passphrase",
        "history_secrets_master_v1",
    ] {
        for sep in [".", "/"] {
            out.push(format!("Signum{sep}{username}"));
        }
    }
    // History secrets are per-account: Signum.history:<email>. /list parsing is
    // the primary path; when it fails we cannot enumerate them all here.
    let _ = text;
    out
}

fn maybe_confirm(silent: bool, install_dir: &Path, wipe_downloads: bool) -> bool {
    if silent {
        return true;
    }
    println!("Signum будет удалён с этого компьютера.");
    println!("Папка установки: {}", install_dir.display());
    println!("Будут удалены: настройки, вход в Apple ID, кэши, ярлыки, журналы.");
    if wipe_downloads {
        if let Some(dir) = downloads_dir() {
            println!("Также удалим папку загрузок: {}", dir.display());
        }
    } else if let Some(dir) = downloads_dir() {
        println!("Папка загрузок ({}) останется — там ваши файлы.", dir.display());
    }
    print!("Продолжить? [y/N]: ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes" | "д" | "да")
}

/// The exe cannot delete its own directory while running. Detach a cmd that
/// waits a moment and removes the whole install folder after we exit.
fn schedule_self_cleanup(install_dir: &Path) {
    let quoted = install_dir.display().to_string();
    let script = format!(
        "ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{quoted}\""
    );
    let mut cmd = Command::new("cmd");
    cmd.arg("/c").arg(script);
    let _ = cmd.spawn();
}

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    // No console output when launched silently from "Apps & Features".
    let silent = raw_args
        .iter()
        .skip(1)
        .any(|a| a.eq_ignore_ascii_case("--silent") || a.eq_ignore_ascii_case("-s"));
    let wipe_downloads = raw_args
        .iter()
        .skip(1)
        .any(|a| a.eq_ignore_ascii_case("--wipe-downloads"));

    let exe = std::env::current_exe().unwrap_or_default();
    let install_dir = exe
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    if !maybe_confirm(silent, &install_dir, wipe_downloads) {
        return;
    }

    // 1. Stop app processes so files unlock.
    run_quiet("taskkill", &["/F", "/IM", "signum.exe"]);
    run_quiet("taskkill", &["/F", "/IM", "anisette.exe"]);
    run_quiet("taskkill", &["/F", "/IM", "ipatool.exe"]);

    // 2. Shortcuts (Start Menu + Desktop).
    if let Some(roaming) = appdata() {
        delete_file(
            &roaming
                .join(r"Microsoft\Windows\Start Menu\Programs\Signum.lnk"),
        );
    }
    if let Some(home) = home_dir() {
        delete_file(&home.join(r"Desktop\Signum.lnk"));
    }

    // 3. "Apps & Features" entry.
    run_quiet("reg", &["delete", REG_UNINSTALL_KEY, "/f"]);

    // 4. Saved credentials (Apple ID e-mail, keychain passphrase, history keys).
    wipe_signum_credentials();

    // 5. App data everywhere (settings, accounts, sessions, caches, tools).
    for dir in data_dirs() {
        delete_dir(&dir);
    }
    if wipe_downloads {
        if let Some(dir) = downloads_dir() {
            delete_dir(&dir);
        }
    }

    // 6. Install folder goes last, after this process lets go of it.
    if !silent {
        println!("Завершаем удаление папки установки…");
    }
    schedule_self_cleanup(&install_dir);
}
