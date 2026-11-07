use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
use tauri::{App, AppHandle, Manager};

const APP_NAME: &str = "Signum";
const BUNDLE_ID: &str = "pw.appbank.signum";

static MODE: AtomicBool = AtomicBool::new(false);
static SILENT: AtomicBool = AtomicBool::new(false);
static DELETE_DATA: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallInfo {
    pub version: String,
    pub target: String,
}

pub fn capture_args() {
    let args: Vec<String> = env::args().skip(1).collect();
    let uninstall = args.iter().any(|a| {
        let lower = a.to_ascii_lowercase();
        lower == "--uninstall" || lower == "/uninstall" || lower == "-uninstall"
    });
    let silent = args.iter().any(|a| {
        let lower = a.to_ascii_lowercase();
        lower == "--silent" || lower == "/s" || lower == "-s"
    });
    let delete_data = args.iter().any(|a| {
        let lower = a.to_ascii_lowercase();
        lower == "--delete-data" || lower == "/delete-data"
    });
    MODE.store(uninstall, Ordering::SeqCst);
    SILENT.store(silent, Ordering::SeqCst);
    DELETE_DATA.store(delete_data, Ordering::SeqCst);
}

pub fn is_mode() -> bool {
    MODE.load(Ordering::SeqCst)
}

pub fn is_silent() -> bool {
    SILENT.load(Ordering::SeqCst)
}

pub fn delete_data_flag() -> bool {
    DELETE_DATA.load(Ordering::SeqCst)
}

pub fn configure_window(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        let size = tauri::Size::Logical(tauri::LogicalSize::new(420.0, 540.0));
        let _ = window.set_title("Удаление Signum");
        let _ = window.set_size(size);
        let _ = window.set_min_size(Some(size));
        let _ = window.set_max_size(Some(size));
        let _ = window.set_resizable(false);
        let _ = window.set_maximizable(false);
        let _ = window.center();
    }
    Ok(())
}

fn env_dir(key: &str) -> Option<PathBuf> {
    env::var_os(key).map(PathBuf::from)
}

fn home_dir() -> Option<PathBuf> {
    env_dir("USERPROFILE")
        .or_else(|| env_dir("HOME"))
        .or_else(|| env::var_os("HOMEDRIVE").and_then(|drive| {
            env::var_os("HOMEPATH").map(|path| PathBuf::from(drive).join(path))
        }))
}

fn local_app_data() -> Option<PathBuf> {
    env_dir("LOCALAPPDATA")
}

fn roaming_app_data() -> Option<PathBuf> {
    env_dir("APPDATA")
}

pub fn install_dir() -> PathBuf {
    if let Ok(over) = env::var("SIGNUM_INSTALL_TARGET") {
        if !over.trim().is_empty() {
            return PathBuf::from(over);
        }
    }
    if let Some(local) = local_app_data() {
        return local.join(APP_NAME);
    }
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn current_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn paths_eq_ci(a: &Path, b: &Path) -> bool {
    let aa = a.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
    let bb = b.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
    aa.trim_end_matches('\\') == bb.trim_end_matches('\\')
}

/// Never delete the cargo target folder if someone runs `--uninstall` from a dev build.
fn is_safe_install_dir(dir: &Path) -> bool {
    let text = dir.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
    if text.contains("\\src-tauri\\") || text.contains("\\target\\debug") || text.contains("\\target\\release")
    {
        return false;
    }
    if let Some(local) = local_app_data() {
        if paths_eq_ci(dir, &local.join(APP_NAME)) {
            return true;
        }
    }
    if let Ok(over) = env::var("SIGNUM_INSTALL_TARGET") {
        if !over.trim().is_empty() && paths_eq_ci(dir, Path::new(&over)) {
            return true;
        }
    }
    false
}

fn remove_path(path: &Path) {
    if !path.exists() {
        return;
    }
    for attempt in 0..4 {
        let result = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        if result.is_ok() || !path.exists() {
            return;
        }
        if attempt < 3 {
            thread::sleep(Duration::from_millis(250 + attempt as u64 * 150));
        }
    }
}

fn kill_helpers() {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        let pid = std::process::id();
        let mut self_cmd = Command::new("taskkill");
        self_cmd.args([
            "/F",
            "/IM",
            "signum.exe",
            "/FI",
            &format!("PID ne {pid}"),
        ]);
        self_cmd.creation_flags(0x0800_0000);
        let _ = self_cmd.status();
        for name in ["ipatool.exe", "sapsigner.exe", "anisette.exe"] {
            let mut cmd = Command::new("taskkill");
            cmd.args(["/F", "/IM", name, "/T"]);
            cmd.creation_flags(0x0800_0000);
            let _ = cmd.status();
        }
        thread::sleep(Duration::from_millis(400));
    }
}

fn wipe_user_data() {
    if let Some(local) = local_app_data() {
        let root = local.join(APP_NAME);
        remove_path(&root.join("history-secrets"));
        remove_path(&root.join("store-session"));
        remove_path(&root.join("history-debug"));
        remove_path(&root.join("bank-alt-creds.json"));
        remove_path(&local.join(BUNDLE_ID));
    }
    if let Some(roaming) = roaming_app_data() {
        remove_path(&roaming.join(BUNDLE_ID));
        remove_path(&roaming.join(APP_NAME));
    }
    if let Some(home) = home_dir() {
        remove_path(&home.join(".ipatool"));
    }
}

fn remove_shortcuts() {
    if let Some(roaming) = roaming_app_data() {
        let programs = roaming
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs");
        remove_path(&programs.join("Signum.lnk"));
        remove_path(&programs.join("Signum"));
    }
    if let Some(home) = home_dir() {
        remove_path(&home.join("Desktop").join("Signum.lnk"));
    }
    if let Some(public) = env_dir("PUBLIC") {
        remove_path(&public.join("Desktop").join("Signum.lnk"));
    }
}

fn remove_uninstall_registry() {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        let keys = [
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum",
            r"HKCU\Software\Signum\Signum",
            r"HKCU\Software\Signum",
        ];
        for key in keys {
            let mut cmd = Command::new("reg");
            cmd.args(["delete", key, "/f"]);
            cmd.creation_flags(0x0800_0000);
            let _ = cmd.status();
        }
        let mut run = Command::new("reg");
        run.args([
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            APP_NAME,
            "/f",
        ]);
        run.creation_flags(0x0800_0000);
        let _ = run.status();
    }
}

fn spawn_remove_install_dir(delete_data: bool) {
    let dir = current_dir().unwrap_or_else(install_dir);
    if !is_safe_install_dir(&dir) {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        let path = dir.display().to_string().replace('\'', "''");
        let wipe_root = if delete_data {
            format!(
                "Remove-Item -LiteralPath '{path}' -Recurse -Force -ErrorAction SilentlyContinue"
            )
        } else {
            format!(
                "Remove-Item -LiteralPath '{path}\\signum.exe' -Force -ErrorAction SilentlyContinue; \
                 Remove-Item -LiteralPath '{path}\\resources' -Recurse -Force -ErrorAction SilentlyContinue; \
                 Remove-Item -LiteralPath '{path}\\uninstall.exe' -Force -ErrorAction SilentlyContinue; \
                 Remove-Item -LiteralPath '{path}\\uninstall.cmd' -Force -ErrorAction SilentlyContinue; \
                 Remove-Item -LiteralPath '{path}' -Force -ErrorAction SilentlyContinue"
            )
        };
        let script = format!(
            "Start-Sleep -Seconds 2; \
             Stop-Process -Name signum -Force -ErrorAction SilentlyContinue; \
             Start-Sleep -Seconds 1; \
             {wipe_root}; \
             Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue"
        );
        let script_path = env::temp_dir().join("signum-finish-uninstall.ps1");
        let _ = fs::write(&script_path, script);
        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
            &script_path.to_string_lossy(),
        ]);
        cmd.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
        let _ = cmd.spawn();
    }
}

/// Remove shortcuts, registry, and (optionally) login leftovers. Program files
/// in the install dir are deleted after this process exits.
pub fn perform(delete_data: bool) -> Result<(), String> {
    kill_helpers();
    remove_shortcuts();
    remove_uninstall_registry();
    if delete_data {
        wipe_user_data();
    }
    spawn_remove_install_dir(delete_data);
    Ok(())
}

pub fn run_silent() {
    let _ = perform(delete_data_flag());
}

#[tauri::command]
pub fn is_uninstall_mode() -> bool {
    is_mode()
}

#[tauri::command]
pub fn uninstall_info() -> UninstallInfo {
    UninstallInfo {
        version: crate::APP_VERSION.to_string(),
        target: install_dir().display().to_string(),
    }
}

#[tauri::command]
pub fn start_uninstall(app: AppHandle, delete_data: bool) -> Result<(), String> {
    perform(delete_data)?;
    app.exit(0);
    Ok(())
}
