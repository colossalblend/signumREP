#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::{
    fs,
    io::{copy, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, State};

const APP_NAME: &str = "Signum";
/// Берём из Cargo.toml, а не повторяем строкой: одна забытая правка здесь — и
/// в «Установке и удалении программ» годами висит версия, которой нет.
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const DETACHED: u32 = 0x0000_0008 | 0x0000_0200;

struct Installed(Mutex<Option<PathBuf>>);

#[derive(Clone, Serialize)]
struct SetupInfo {
    version: String,
    target: String,
}

#[derive(Clone, Serialize)]
struct Progress {
    percent: f64,
    message: String,
}

fn target_dir() -> PathBuf {
    if let Ok(over) = std::env::var("SIGNUM_INSTALL_TARGET") {
        if !over.trim().is_empty() {
            return PathBuf::from(over);
        }
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
}

fn emit_progress(app: Option<&AppHandle>, percent: f64, message: &str) {
    if let Some(app) = app {
        let _ = app.emit(
            "setup-progress",
            Progress {
                percent,
                message: message.to_string(),
            },
        );
    }
}

/// Имя пакета с файлами приложения.
const PAYLOAD_NAME: &str = "SignumPayload.zip";

/// Открывает пакет с файлами приложения.
///
/// Пакет дописан в хвост самого exe. Формат zip читается с конца, поэтому
/// такой файл остаётся валидным архивом, а установщик — одним файлом.
///
/// Раньше пакет вшивался через `include_bytes!`. Пока весил под сотню
/// мегабайт, это работало; с запечатанными артефактами и выросшим движком он
/// перевалил за триста, и rustc стал падать на сборке со
/// STATUS_STACK_BUFFER_OVERRUN. Здесь же компилятор пакета не видит вовсе.
fn open_payload() -> Result<zip::ZipArchive<fs::File>, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Не удалось определить путь установщика: {e}"))?;
    if let Ok(file) = fs::File::open(&exe) {
        if let Ok(archive) = zip::ZipArchive::new(file) {
            return Ok(archive);
        }
    }
    // Запасной путь — отдельный файл рядом. Нужен при отладке, когда стоит
    // голый stub без дописанного пакета.
    let dir = exe
        .parent()
        .ok_or_else(|| "Не удалось определить папку установщика".to_string())?;
    for candidate in [dir.join(PAYLOAD_NAME), dir.join("resources").join(PAYLOAD_NAME)] {
        if let Ok(file) = fs::File::open(&candidate) {
            if let Ok(archive) = zip::ZipArchive::new(file) {
                return Ok(archive);
            }
        }
    }
    Err("Установщик повреждён: внутри нет файлов приложения. Скачайте его заново.".to_string())
}

fn stop_running_app() {
    let mut cmd = Command::new("taskkill");
    cmd.args(["/F", "/IM", "signum.exe"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.status();
    std::thread::sleep(std::time::Duration::from_millis(350));
}

fn create_shortcut(folder: &str, exe: &Path, work: &Path) -> Result<(), String> {
    let exe_s = exe.display().to_string().replace('\'', "''");
    let work_s = work.display().to_string().replace('\'', "''");
    let script = format!(
        "$shell=New-Object -ComObject WScript.Shell; \
         $shortcut=$shell.CreateShortcut([Environment]::GetFolderPath('{folder}') + '\\Signum.lnk'); \
         $shortcut.TargetPath='{exe_s}'; \
         $shortcut.WorkingDirectory='{work_s}'; \
         $shortcut.IconLocation='{exe_s}'; \
         $shortcut.Save()"
    );
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.status()
        .map_err(|e| format!("Не удалось создать ярлык: {e}"))?;
    Ok(())
}

fn write_uninstaller(target: &Path, exe: &Path) {
    // Рядом с приложением лежит uninstall.exe (из payload). Если поймали старый
    // payload без него, оставляем cmd-обёртку как запасной вариант.
    let uninstaller = target.join("uninstall.exe");
    let uninstall_path: PathBuf;
    if uninstaller.is_file() {
        uninstall_path = uninstaller;
    } else {
        let script = target.join("uninstall.cmd");
        let body = format!(
            "@echo off\r\n\
             if exist \"{exe}\" (\r\n\
               start \"\" \"{exe}\" --uninstall\r\n\
               exit /b 0\r\n\
             )\r\n\
             taskkill /F /IM signum.exe >nul 2>&1\r\n\
             del /f /q \"%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Signum.lnk\" >nul 2>&1\r\n\
             del /f /q \"%USERPROFILE%\\Desktop\\Signum.lnk\" >nul 2>&1\r\n\
             reg delete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Signum\" /f >nul 2>&1\r\n\
             rmdir /s /q \"%USERPROFILE%\\.ipatool\" >nul 2>&1\r\n\
             rmdir /s /q \"%APPDATA%\\pw.appbank.signum\" >nul 2>&1\r\n\
             rmdir /s /q \"%LOCALAPPDATA%\\pw.appbank.signum\" >nul 2>&1\r\n\
             rmdir /s /q \"%APPDATA%\\pw.appbank.signum.setup\" >nul 2>&1\r\n\
             rmdir /s /q \"%LOCALAPPDATA%\\pw.appbank.signum.setup\" >nul 2>&1\r\n\
             rmdir /s /q \"%LOCALAPPDATA%\\ipatool\" >nul 2>&1\r\n\
             rmdir /s /q \"{target}\\history-secrets\" >nul 2>&1\r\n\
             rmdir /s /q \"{target}\\store-session\" >nul 2>&1\r\n\
             rmdir /s /q \"{target}\\history-debug\" >nul 2>&1\r\n\
             rmdir /s /q \"{target}\\resources\" >nul 2>&1\r\n\
             del /f /q \"{exe}\" >nul 2>&1\r\n\
             del /f /q \"{script}\" >nul 2>&1\r\n\
             rmdir /s /q \"{target}\" >nul 2>&1\r\n",
            target = target.display(),
            exe = exe.display(),
            script = script.display(),
        );
        let _ = fs::write(&script, body);
        uninstall_path = script;
    }

    let uninstall_cmd = format!("\"{path}\"", path = uninstall_path.display());
    let quiet_cmd = format!("\"{}\" --silent", uninstall_path.display());

    let mut cmd = Command::new("reg");
    cmd.args([
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum",
        "/v",
        "DisplayName",
        "/t",
        "REG_SZ",
        "/d",
        APP_NAME,
        "/f",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.status();

    let mut cmd = Command::new("reg");
    cmd.args([
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum",
        "/v",
        "DisplayVersion",
        "/t",
        "REG_SZ",
        "/d",
        APP_VERSION,
        "/f",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.status();

    let mut cmd = Command::new("reg");
    cmd.args([
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum",
        "/v",
        "InstallLocation",
        "/t",
        "REG_SZ",
        "/d",
        &target.display().to_string(),
        "/f",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.status();

    let mut cmd = Command::new("reg");
    cmd.args([
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum",
        "/v",
        "UninstallString",
        "/t",
        "REG_SZ",
        "/d",
        &uninstall_cmd,
        "/f",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.status();

    let mut cmd = Command::new("reg");
    cmd.args([
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum",
        "/v",
        "QuietUninstallString",
        "/t",
        "REG_SZ",
        "/d",
        &quiet_cmd,
        "/f",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.status();

    let mut cmd = Command::new("reg");
    cmd.args([
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum",
        "/v",
        "DisplayIcon",
        "/t",
        "REG_SZ",
        "/d",
        &exe.display().to_string(),
        "/f",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.status();
}

fn extract_payload(app: Option<&AppHandle>, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| format!("Не удалось создать папку: {e}"))?;
    let mut archive = open_payload()?;
    let total = (0..archive.len())
        .filter(|i| archive.by_index(*i).map(|e| !e.is_dir()).unwrap_or(false))
        .count()
        .max(1);
    let mut done = 0usize;
    for i in 0..archive.len() {
        let (rel, bytes, is_dir) = {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Не удалось прочитать пакет: {e}"))?;
            let name = entry.name().replace('\\', "/");
            if name.contains("..") {
                continue;
            }
            if entry.is_dir() || name.ends_with('/') {
                (name, Vec::new(), true)
            } else {
                let mut buf = Vec::new();
                copy(&mut entry, &mut buf).map_err(|e| format!("Не удалось прочитать файл: {e}"))?;
                (name, buf, false)
            }
        };
        if is_dir {
            continue;
        }
        let dest = target.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Не удалось создать папку: {e}"))?;
        }
        let mut out = fs::File::create(&dest).map_err(|e| {
            format!("Не удалось записать файл: {e}")
        })?;
        out.write_all(&bytes)
            .map_err(|e| format!("Не удалось записать файл: {e}"))?;
        done += 1;
        let pct = 22.0 + (done as f64 / total as f64) * 62.0;
        emit_progress(app, pct, &format!("Копируем файлы… {done} из {total}"));
    }
    Ok(())
}

fn install(app: Option<&AppHandle>, desktop_shortcut: bool) -> Result<PathBuf, String> {
    let target = target_dir();
    emit_progress(app, 4.0, "Останавливаем предыдущую версию…");
    stop_running_app();
    emit_progress(app, 18.0, "Распаковываем файлы…");
    extract_payload(app, &target)?;
    let exe = target.join("signum.exe");
    if !exe.is_file() {
        return Err("Не удалось установить Signum.".into());
    }
    emit_progress(app, 88.0, "Ярлыки…");
    let _ = create_shortcut("Programs", &exe, &target);
    if desktop_shortcut {
        let _ = create_shortcut("Desktop", &exe, &target);
    }
    write_uninstaller(&target, &exe);
    emit_progress(app, 100.0, "Готово");
    Ok(exe)
}

#[tauri::command]
fn setup_info() -> SetupInfo {
    SetupInfo {
        version: APP_VERSION.into(),
        target: target_dir().display().to_string(),
    }
}

#[tauri::command]
fn start_install(
    app: AppHandle,
    state: State<Installed>,
    desktop_shortcut: bool,
) -> Result<(), String> {
    let exe = install(Some(&app), desktop_shortcut)?;
    *state.0.lock().map_err(|_| "внутренняя ошибка")? = Some(exe);
    Ok(())
}

#[tauri::command]
fn launch_app(state: State<Installed>) -> Result<(), String> {
    let guard = state.0.lock().map_err(|_| "внутренняя ошибка")?;
    let exe = guard.clone().ok_or_else(|| "Сначала установите Signum.".to_string())?;
    drop(guard);
    let mut cmd = Command::new(&exe);
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(DETACHED | CREATE_NO_WINDOW);
    }
    cmd.spawn()
        .map_err(|e| format!("Не удалось запустить Signum: {e}"))?;
    Ok(())
}

#[tauri::command]
fn close_setup(app: AppHandle) {
    let _ = app.exit(0);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let silent = args.iter().any(|a| {
        let lower = a.to_ascii_lowercase();
        lower == "--silent" || lower == "/s"
    });
    if silent {
        let desktop = !args.iter().any(|a| a == "--no-desktop");
        match install(None, desktop) {
            Ok(_) => std::process::exit(0),
            Err(_) => std::process::exit(1),
        }
    }
    tauri::Builder::default()
        .manage(Installed(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            setup_info,
            start_install,
            launch_app,
            close_setup
        ])
        .run(tauri::generate_context!())
        .expect("setup");
}
