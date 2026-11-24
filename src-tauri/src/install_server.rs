// Wi-Fi сервер установки: раздаём IPA по локальной сети, iPhone ставит
// приложение сам через itms-services. Альтернатива USB-кабелю на случай,
// когда кабеля нет под рукой: Signum поднимает HTTP-сервер, рисует QR,
// человек открывает ссылку на iPhone и жмёт «Установить».

use serde::Serialize;
use std::io::Read;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

const READ_CHUNK: usize = 64 * 1024;
/// Сколько ждать, пока iPhone скачает приложение (30 минут за глаза).
const WAIT_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallServerState {
    pub url: String,
    pub title: String,
    pub bundle_id: String,
    pub version: String,
    pub size: u64,
    pub downloaded: u64,
    /// iPhone скачал приложение целиком (дальше ставит сам).
    pub complete: bool,
    /// Сервер остановлен — модалку в интерфейсе можно закрыть.
    pub stopped: bool,
}

struct Server {
    state: InstallServerState,
    downloaded: Arc<AtomicU64>,
    complete: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

fn servers() -> &'static Mutex<Option<Server>> {
    static SERVER: OnceLock<Mutex<Option<Server>>> = OnceLock::new();
    SERVER.get_or_init(|| Mutex::new(None))
}

fn snapshot(server: &Server, stopped: bool) -> InstallServerState {
    InstallServerState {
        url: server.state.url.clone(),
        title: server.state.title.clone(),
        bundle_id: server.state.bundle_id.clone(),
        version: server.state.version.clone(),
        size: server.state.size,
        downloaded: server.downloaded.load(Ordering::Relaxed),
        complete: server.complete.load(Ordering::Relaxed),
        stopped,
    }
}

fn emit_state(app: &AppHandle) {
    let payload = servers().lock().unwrap().as_ref().map(|s| snapshot(s, false));
    if let Some(payload) = payload {
        let _ = app.emit("install-server-state", payload);
    }
}

/// Локальный IP в текущей Wi-Fi сети без внешних зависимостей:
/// пакеты наружу не уходят, ядро лишь подбирает маршрут до шлюза.
fn local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

/// (title, bundle id, version) из Payload/*.app/Info.plist.
fn ipa_metadata(path: &Path) -> (String, String, String) {
    let fallback = (
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Приложение".into()),
        String::new(),
        "1.0".into(),
    );
    let Ok(file) = std::fs::File::open(path) else {
        return fallback;
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return fallback;
    };
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else { continue };
        let name = entry.name();
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() != 3 || parts[0] != "Payload" || parts[2] != "Info.plist" {
            continue;
        }
        let mut data = Vec::new();
        if entry.read_to_end(&mut data).is_err() {
            continue;
        }
        let Ok(value) = plist::Value::from_reader(std::io::Cursor::new(data)) else {
            continue;
        };
        let get = |key: &str| {
            value
                .as_dictionary()
                .and_then(|d| d.get(key))
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
        };
        let title = get("CFBundleDisplayName")
            .or_else(|| get("CFBundleName"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback.0.clone());
        let bundle_id = get("CFBundleIdentifier").unwrap_or_default();
        if title.is_empty() && bundle_id.is_empty() {
            continue;
        }
        let version = get("CFBundleShortVersionString")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "1.0".into());
        return (title, bundle_id, version);
    }
    fallback
}

/// Поднимает сервер для указанного IPA. Уже работающий сервер останавливает.
fn start_server(
    app: &AppHandle,
    ipa: &Path,
    title: Option<String>,
    bundle_id: Option<String>,
    version: Option<String>,
) -> Result<InstallServerState, String> {
    stop_server_inner(app);
    let size = std::fs::metadata(ipa)
        .map_err(|_| "Файл приложения не найден".to_string())?
        .len();
    if size < 1024 {
        return Err("Файл приложения не найден".into());
    }
    let (meta_title, meta_bundle, meta_version) = ipa_metadata(ipa);
    let title = title
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(meta_title);
    let bundle = bundle_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(meta_bundle);
    let version = version
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(meta_version);

    let ip = local_ip().ok_or("Не удалось определить локальный IP-адрес")?;
    let listener = std::net::TcpListener::bind(("0.0.0.0", 0))
        .map_err(|e| format!("Не удалось открыть порт: {e}"))?;
    let port = listener.local_addr().map_err(|e| format!("Не удалось определить порт: {e}"))?.port();
    // Блокирующий слушатель уходит в tokio — так проще, чем тащить async bind
    // сквозь select, а соединений тут единицы.
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("Сервер: {e}"))?;
    let listener = tokio::net::TcpListener::from_std(listener)
        .map_err(|e| format!("Сервер: {e}"))?;
    let url = format!("http://{ip}:{port}");

    let state = InstallServerState {
        url: url.clone(),
        title: title.clone(),
        bundle_id: bundle.clone(),
        version: version.clone(),
        size,
        downloaded: 0,
        complete: false,
        stopped: false,
    };
    let downloaded = Arc::new(AtomicU64::new(0));
    let complete = Arc::new(AtomicBool::new(false));
    let shutdown = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(tokio::sync::Notify::new());

    {
        let mut guard = servers().lock().unwrap();
        *guard = Some(Server {
            state,
            downloaded: Arc::clone(&downloaded),
            complete: Arc::clone(&complete),
            shutdown: Arc::clone(&shutdown),
            notify: Arc::clone(&notify),
            tasks: Mutex::new(Vec::new()),
        });
    }

    let app_handle = app.clone();
    let ipa_owned = ipa.to_path_buf();
    let base_url = url.clone();
    let accept = tokio::spawn(async move {
        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            let accepted = tokio::select! {
                _ = notify.notified() => break,
                accepted = listener.accept() => accepted,
            };
            let Ok((stream, _)) = accepted else { break };
            let conn_app = app_handle.clone();
            let conn_ipa = ipa_owned.clone();
            let conn_base = base_url.clone();
            let conn_downloaded = Arc::clone(&downloaded);
            let conn_complete = Arc::clone(&complete);
            let conn_shutdown = Arc::clone(&shutdown);
            let handle = tokio::spawn(async move {
                handle_connection(
                    conn_app,
                    stream,
                    conn_ipa,
                    conn_base,
                    conn_downloaded,
                    conn_complete,
                    conn_shutdown,
                )
                .await;
            });
            let mut guard = servers().lock().unwrap();
            match guard.as_mut() {
                Some(server) => server.tasks.lock().unwrap().push(handle),
                None => break,
            }
        }
    });
    if let Some(server) = servers().lock().unwrap().as_mut() {
        server.tasks.lock().unwrap().push(accept);
    }

    emit_state(app);
    Ok(InstallServerState {
        url,
        title,
        bundle_id: bundle,
        version,
        size,
        downloaded: 0,
        complete: false,
        stopped: false,
    })
}

#[tauri::command]
pub async fn start_install_server(
    app: AppHandle,
    ipa_path: String,
    title: Option<String>,
    bundle_id: Option<String>,
    version: Option<String>,
) -> Result<InstallServerState, String> {
    let ipa = PathBuf::from(&ipa_path);
    // Блокирующая работа с файлами мала (open + чтение Info.plist), поэтому
    // спокойно делаем её в async-команде.
    start_server(&app, &ipa, title, bundle_id, version)
}

#[tauri::command]
pub fn stop_install_server(app: AppHandle) -> Result<(), String> {
    stop_server_inner(&app);
    Ok(())
}

#[tauri::command]
pub fn get_install_server_state() -> Option<InstallServerState> {
    servers().lock().unwrap().as_ref().map(|s| snapshot(s, false))
}

fn stop_server_inner(app: &AppHandle) {
    let payload = {
        let mut guard = servers().lock().unwrap();
        let Some(server) = guard.as_ref() else {
            return;
        };
        let payload = snapshot(server, true);
        server.shutdown.store(true, Ordering::Relaxed);
        server.notify.notify_waiters();
        {
            let mut tasks = server.tasks.lock().unwrap();
            for task in tasks.drain(..) {
                task.abort();
            }
        }
        *guard = None;
        payload
    };
    let _ = app.emit("install-server-state", payload);
}

/// Ветка install_ipa для метода «server»: поднимаем сервер и ждём, пока
/// iPhone выкачает приложение. Прогресс — по фактически отданным байтам.
pub async fn install_via_server(
    app: &AppHandle,
    job_id: &str,
    ipa: &Path,
    progress_from: f64,
    progress_to: f64,
) -> Result<(), String> {
    let state = start_server(app, ipa, None, None, None)?;
    let size = state.size;
    crate::update_job(
        app,
        job_id,
        "install",
        progress_from,
        "Wi-Fi сервер: откройте ссылку на iPhone и нажмите «Установить»",
        crate::JobStatus::Running,
        None,
    )
    .await;
    let started = tokio::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if crate::is_job_cancelled(&app.state::<crate::AppState>(), job_id) {
            stop_server_inner(app);
            return Err("Установка отменена".into());
        }
        let (downloaded, complete) = {
            let guard = servers().lock().unwrap();
            match guard.as_ref() {
                Some(server) => (
                    server.downloaded.load(Ordering::Relaxed),
                    server.complete.load(Ordering::Relaxed),
                ),
                // Сервер остановили руками из интерфейса — считаем отменой.
                None => return Err("Wi-Fi сервер остановлен".into()),
            }
        };
        if complete {
            return Ok(());
        }
        let share = if size > 0 {
            (downloaded as f64 / size as f64).min(1.0)
        } else {
            0.0
        };
        crate::update_job(
            app,
            job_id,
            "install",
            progress_from + (progress_to - progress_from) * share,
            "Wi-Fi сервер: iPhone скачивает приложение…",
            crate::JobStatus::Running,
            None,
        )
        .await;
        if started.elapsed() >= WAIT_TIMEOUT {
            stop_server_inner(app);
            return Err(
                "iPhone не скачал приложение за 30 минут. Убедитесь, что оба устройства в одной Wi-Fi сети, и попробуйте снова."
                    .into(),
            );
        }
    }
}

/// Одно соединение: страница, манифест или сама IPA (с поддержкой Range).
#[allow(clippy::too_many_arguments)]
async fn handle_connection(
    app: AppHandle,
    mut stream: TcpStream,
    ipa: PathBuf,
    base_url: String,
    downloaded: Arc<AtomicU64>,
    complete: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) {
    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut tmp = [0u8; 8 * 1024];
    let header_len = loop {
        let n = stream.read(&mut tmp).await.unwrap_or(0);
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = header_end(&buf) {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            return;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_len]).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let path = parts
        .next()
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let range = head
        .lines()
        .find_map(|l| {
            let lower = l.to_ascii_lowercase();
            lower
                .strip_prefix("range:")
                .map(|v| v.trim().to_string())
        })
        .unwrap_or_default();

    match path.as_str() {
        "/" | "/index.html" => {
            let page = page_html(&base_url);
            respond(&mut stream, 200, "text/html; charset=utf-8", page.as_bytes(), method == "HEAD").await;
        }
        "/manifest.plist" => {
            let manifest = build_manifest(&base_url);
            respond(&mut stream, 200, "application/xml", &manifest, method == "HEAD").await;
        }
        "/app.ipa" => {
            serve_ipa(
                &mut stream,
                &method,
                &ipa,
                &range,
                &app,
                downloaded,
                complete,
                shutdown,
            )
            .await;
        }
        _ => {
            respond(&mut stream, 404, "text/plain; charset=utf-8", b"not found", false).await;
        }
    }
}

fn header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

async fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8], head_only: bool) {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    if !head_only {
        let _ = stream.write_all(body).await;
    }
}

fn build_manifest(base_url: &str) -> Vec<u8> {
    let (bundle, version, title) = {
        let guard = servers().lock().unwrap();
        match guard.as_ref() {
            Some(s) => (
                if s.state.bundle_id.is_empty() {
                    "com.example.app".to_string()
                } else {
                    s.state.bundle_id.clone()
                },
                s.state.version.clone(),
                s.state.title.clone(),
            ),
            None => ("com.example.app".to_string(), "1.0".to_string(), "App".to_string()),
        }
    };
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    // Манифест маленький и с фиксированным набором ключей — проще собрать
    // XML руками, чем тянуть сериализатор.
    let xml = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">",
            "<plist version=\"1.0\"><dict><key>items</key><array><dict>",
            "<key>assets</key><array><dict>",
            "<key>kind</key><string>software-package</string>",
            "<key>url</key><string>{}</string>",
            "</dict></array>",
            "<key>metadata</key><dict>",
            "<key>bundle-identifier</key><string>{}</string>",
            "<key>bundle-version</key><string>{}</string>",
            "<key>kind</key><string>software</string>",
            "<key>title</key><string>{}</string>",
            "</dict></dict></array></dict></plist>"
        ),
        esc(&format!("{base_url}/app.ipa")),
        esc(&bundle),
        esc(&version),
        esc(&title),
    );
    xml.into_bytes()
}

#[allow(clippy::too_many_arguments)]
async fn serve_ipa(
    stream: &mut TcpStream,
    method: &str,
    ipa: &Path,
    range: &str,
    app: &AppHandle,
    downloaded: Arc<AtomicU64>,
    complete: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) {
    let Ok(mut file) = tokio::fs::File::open(ipa).await else {
        return;
    };
    let Ok(meta) = file.metadata().await else {
        return;
    };
    let size = meta.len();
    if size == 0 {
        return;
    }
    let (start, end) = parse_range(range, size);
    let length = end - start + 1;
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return;
    }
    let head = if start == 0 && end + 1 == size {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {size}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
        )
    } else {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Type: application/octet-stream\r\nContent-Length: {length}\r\nContent-Range: bytes {start}-{end}/{size}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
        )
    };
    if stream.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    if method == "HEAD" {
        return;
    }

    let mut remaining = length as usize;
    let mut sent: u64 = 0;
    let mut last_emit = tokio::time::Instant::now()
        .checked_sub(Duration::from_secs(10))
        .unwrap_or_else(tokio::time::Instant::now);
    let mut buf = vec![0u8; READ_CHUNK];
    while remaining > 0 {
        if shutdown.load(Ordering::Relaxed) {
            return;
        }
        let want = remaining.min(READ_CHUNK);
        let read = file.read(&mut buf[..want]).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        if stream.write_all(&buf[..read]).await.is_err() {
            break;
        }
        remaining -= read;
        sent += read as u64;
        downloaded.fetch_add(read as u64, Ordering::Relaxed);
        let now = tokio::time::Instant::now();
        if now.duration_since(last_emit) >= Duration::from_millis(400) {
            last_emit = now;
            emit_state(app);
        }
    }
    if sent >= length && length > 0 && !complete.swap(true, Ordering::Relaxed) {
        emit_state(app);
    }
}

fn parse_range(header: &str, size: u64) -> (u64, u64) {
    // "bytes=start-end" | "bytes=start-". Суффиксных форм («bytes=-500») iOS
    // при установке не присылает.
    let header = header.trim().strip_prefix("bytes=").unwrap_or("").trim();
    if header.is_empty() {
        return (0, size - 1);
    }
    let mut bounds = header.splitn(2, '-');
    let start = bounds
        .next()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(0)
        .min(size - 1);
    let end = bounds
        .next()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(size - 1)
        .min(size - 1);
    (start, end)
}

fn page_html(base_url: &str) -> String {
    let manifest = format!("{base_url}/manifest.plist");
    format!(
        r#"<!DOCTYPE html>
<html lang="ru">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Signum — установка</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{
    font-family: -apple-system, "SF Pro Display", "Segoe UI", Roboto, sans-serif;
    background: linear-gradient(160deg, #0b1220 0%, #10192b 100%);
    color: #eef2f8; min-height: 100vh;
    display: flex; align-items: center; justify-content: center; padding: 24px;
  }}
  .card {{
    max-width: 420px; width: 100%; background: #141d30;
    border: 1px solid rgba(255,255,255,.08); border-radius: 22px; padding: 30px 26px;
    text-align: center;
  }}
  .title {{ font-size: 21px; font-weight: 700; }}
  .hint {{ color: #94a3bd; font-size: 13px; line-height: 1.5; margin-top: 8px; }}
  .install {{
    display: block; margin-top: 22px; padding: 15px 20px; border-radius: 14px;
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    color: #fff; text-decoration: none; font-weight: 700; font-size: 16px;
  }}
  .note {{ color: #6b7a93; font-size: 11px; margin-top: 18px; line-height: 1.5; }}
</style>
</head>
<body>
<div class="card">
  <div class="title">Установка через Signum</div>
  <div class="hint">Нажмите кнопку ниже — приложение скачается и установится на этот iPhone.</div>
  <a class="install" href="itms-services://?action=install-manifest&url={manifest}">Установить</a>
  <div class="note">Не закрывайте Signum на другом устройстве, пока идёт установка.<br>Оба устройства должны быть в одной Wi-Fi сети.</div>
</div>
</body>
</html>"#
    )
}
