                                                                                    
                                                      

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU16, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CredsPageSession {
    pub token: String,
    pub bank_name: String,
    pub login: String,
    pub password: String,
    pub job_id: String,
    pub phase: String,
    pub percent: f64,
    pub message: String,
    pub status: String,                                    
    pub created: Instant,
}

struct ServerInner {
    sessions: HashMap<String, CredsPageSession>,
    port: u16,
    running: bool,
}

static PORT: AtomicU16 = AtomicU16::new(0);
static STOP: AtomicBool = AtomicBool::new(false);

fn server_state() -> &'static Mutex<ServerInner> {
    static STATE: std::sync::OnceLock<Mutex<ServerInner>> = std::sync::OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(ServerInner {
            sessions: HashMap::new(),
            port: 0,
            running: false,
        })
    })
}

                                                                    
   
                                                                     
                                                                            
pub fn local_lan_ipv4() -> Option<String> {
    let mut candidates: Vec<std::net::Ipv4Addr> = Vec::new();

                                                                                     
    candidates.extend(list_ipv4_from_os());

                                                                              
    for dest in [
        "192.168.0.1:80",
        "192.168.1.1:80",
        "10.0.0.1:80",
        "172.16.0.1:80",
    ] {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect(dest).is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    if let std::net::IpAddr::V4(v4) = addr.ip() {
                        candidates.push(v4);
                    }
                }
            }
        }
    }

                                                                       
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                if let std::net::IpAddr::V4(v4) = addr.ip() {
                    candidates.push(v4);
                }
            }
        }
    }

    candidates
        .into_iter()
        .filter(is_usable_lan_v4)
        .max_by_key(|ip| score_lan_v4(ip))
        .map(|ip| ip.to_string())
}

fn is_usable_lan_v4(ip: &std::net::Ipv4Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast() {
        return false;
    }
    let o = ip.octets();
                       
    if o[0] == 169 && o[1] == 254 {
        return false;
    }
                                                                                 
    if o[0] == 198 && (o[1] == 18 || o[1] == 19) {
        return false;
    }
                               
    if o[0] == 198 && o[1] == 51 && o[2] == 100 {
        return false;
    }
    true
}

                                                    
fn score_lan_v4(ip: &std::net::Ipv4Addr) -> i32 {
    let o = ip.octets();
                                 
    if o[0] == 192 && o[1] == 168 {
        return 100;
    }
    if o[0] == 10 {
        return 90;
    }
    if o[0] == 172 && (16..=31).contains(&o[1]) {
        return 80;
    }
                                                             
    if o[0] == 100 && (64..=127).contains(&o[1]) {
        return 15;
    }
    5
}

fn list_ipv4_from_os() -> Vec<std::net::Ipv4Addr> {
    #[cfg(target_os = "windows")]
    {
        list_ipv4_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        list_ipv4_unix()
    }
}

#[cfg(target_os = "windows")]
fn list_ipv4_windows() -> Vec<std::net::Ipv4Addr> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

                                                            
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn silent_output(program: &str, args: &[&str]) -> Option<String> {
        let o = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        if !o.status.success() && o.stdout.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&o.stdout).into_owned())
    }

    let mut out = Vec::new();

                                                          
    if let Some(text) = silent_output("ipconfig", &[]) {
                                                                      
        for line in text.lines() {
            let lower = line.to_ascii_lowercase();
            if !(lower.contains("ipv4")
                || lower.contains("ip-адрес")
                || lower.contains("ip address"))
            {
                continue;
            }
            if let Some(pos) = line.rfind(':') {
                let cand = line[pos + 1..].trim();
                if let Ok(ip) = cand.parse::<std::net::Ipv4Addr>() {
                    out.push(ip);
                }
            }
        }
    }

                                                     
    if out.is_empty() {
        if let Some(text) = silent_output(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty IPAddress",
            ],
        ) {
            for line in text.lines() {
                if let Ok(ip) = line.trim().parse::<std::net::Ipv4Addr>() {
                    out.push(ip);
                }
            }
        }
    }
    out
}

#[cfg(not(target_os = "windows"))]
fn list_ipv4_unix() -> Vec<std::net::Ipv4Addr> {
    use std::process::Command;
    let mut out = Vec::new();
                                                   
    if let Ok(o) = Command::new("ip")
        .args(["-4", "-o", "addr", "show", "scope", "global"])
        .output()
    {
        if o.status.success() {
            let text = String::from_utf8_lossy(&o.stdout);
            for part in text.split_whitespace() {
                                            
                let base = part.split('/').next().unwrap_or("");
                if let Ok(ip) = base.parse::<std::net::Ipv4Addr>() {
                    out.push(ip);
                }
            }
        }
    }
    if out.is_empty() {
        if let Ok(o) = Command::new("ifconfig").output() {
            let text = String::from_utf8_lossy(&o.stdout);
            for line in text.lines() {
                for token in line.split_whitespace() {
                    let base = token.split('/').next().unwrap_or("");
                    if let Ok(ip) = base.parse::<std::net::Ipv4Addr>() {
                        out.push(ip);
                    }
                }
            }
        }
    }
    out
}

                                                               
pub async fn publish_creds_page(
    bank_name: &str,
    login: &str,
    password: &str,
    job_id: &str,
) -> Result<(String, String), String> {
    ensure_server_running().await?;
    let port = PORT.load(Ordering::Relaxed);
    if port == 0 {
        return Err("Мини-сервер не запустился".into());
    }
    let token = Uuid::new_v4().simple().to_string();
    let session = CredsPageSession {
        token: token.clone(),
        bank_name: bank_name.to_string(),
        login: login.to_string(),
        password: password.to_string(),
        job_id: job_id.to_string(),
        phase: "queue".into(),
        percent: 0.0,
        message: "Signum скачивает приложение и ставит на iPhone…".into(),
        status: "running".into(),
        created: Instant::now(),
    };
    {
        let mut g = server_state().lock().unwrap();
                                                        
        g.sessions
            .retain(|_, s| s.created.elapsed() < Duration::from_secs(30 * 60));
        g.sessions.insert(token.clone(), session);
    }
    let host = local_lan_ipv4().unwrap_or_else(|| "127.0.0.1".into());
    let url = format!("http://{host}:{port}/c/{token}");
    Ok((token, url))
}

                                                        
pub fn update_job_status(job_id: &str, phase: &str, percent: f64, message: &str, status: &str) {
    let mut g = server_state().lock().unwrap();
    for s in g.sessions.values_mut() {
        if s.job_id == job_id {
            s.phase = phase.to_string();
            s.percent = percent;
            s.message = message.to_string();
            s.status = status.to_string();
        }
    }
}

                                                                         
pub fn stop_all() {
    STOP.store(true, Ordering::Relaxed);
    let mut g = server_state().lock().unwrap();
    g.sessions.clear();
    g.running = false;
    PORT.store(0, Ordering::Relaxed);
}

                                              
pub fn has_active_sessions() -> bool {
    let mut g = server_state().lock().unwrap();
    g.sessions
        .retain(|_, s| s.created.elapsed() < Duration::from_secs(30 * 60));
    !g.sessions.is_empty()
}

async fn ensure_server_running() -> Result<(), String> {
    {
        let g = server_state().lock().unwrap();
        if g.running && PORT.load(Ordering::Relaxed) != 0 {
            return Ok(());
        }
    }
    STOP.store(false, Ordering::Relaxed);
                                                                       
    let listener = match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 18765))).await {
        Ok(l) => l,
        Err(_) => TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
            .await
            .map_err(|e| format!("Не удалось открыть порт для QR-страницы: {e}"))?,
    };
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    PORT.store(port, Ordering::Relaxed);
    {
        let mut g = server_state().lock().unwrap();
        g.port = port;
        g.running = true;
    }

    tauri::async_runtime::spawn(async move {
        loop {
            if STOP.load(Ordering::Relaxed) {
                break;
            }
            let accept = tokio::time::timeout(Duration::from_millis(800), listener.accept()).await;
            let Ok(Ok((mut socket, _))) = accept else {
                continue;
            };
            tauri::async_runtime::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = match socket.read(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                let line = req.lines().next().unwrap_or("");
                let path = line.split_whitespace().nth(1).unwrap_or("/");
                let (status, content_type, body) = handle_path(path);
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n\r\n",
                    body.as_bytes().len()
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.write_all(body.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
        let mut g = server_state().lock().unwrap();
        g.running = false;
        PORT.store(0, Ordering::Relaxed);
    });

                                
    tokio::time::sleep(Duration::from_millis(50)).await;
    Ok(())
}

fn handle_path(path: &str) -> (&'static str, &'static str, String) {
                 
    if let Some(token) = path.strip_prefix("/c/") {
        let token = token.split('?').next().unwrap_or(token).trim();
        if let Some(s) = get_session(token) {
            return ("200 OK", "text/html", render_html(&s));
        }
        return (
            "404 Not Found",
            "text/html",
            "<!doctype html><meta charset=utf-8><body style=\"font-family:system-ui;padding:24px;background:#0b0b12;color:#eee\">Сессия истекла. Откройте QR снова из Signum.</body>".into(),
        );
    }
                   
    if let Some(token) = path.strip_prefix("/api/") {
        let token = token.split('?').next().unwrap_or(token).trim();
        if let Some(s) = get_session(token) {
            let json = serde_json::json!({
                "bank_name": s.bank_name,
                "login": s.login,
                "password": s.password,
                "phase": s.phase,
                "percent": s.percent,
                "message": s.message,
                "status": s.status,
            });
            return ("200 OK", "application/json", json.to_string());
        }
        return (
            "404 Not Found",
            "application/json",
            r#"{"error":"expired"}"#.into(),
        );
    }
    (
        "200 OK",
        "text/html",
        "<!doctype html><meta charset=utf-8><body style=\"font-family:system-ui;padding:24px;background:#0b0b12;color:#eee\">Signum · отсканируйте QR из приложения</body>".into(),
    )
}

fn get_session(token: &str) -> Option<CredsPageSession> {
    let mut g = server_state().lock().unwrap();
    g.sessions
        .retain(|_, s| s.created.elapsed() < Duration::from_secs(30 * 60));
    g.sessions.get(token).cloned()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_html(s: &CredsPageSession) -> String {
    let bank = html_escape(&s.bank_name);
    let bank_js = serde_json::to_string(&s.bank_name).unwrap_or_else(|_| "\"\"".into());
    let login = html_escape(&s.login);
    let password = html_escape(&s.password);
    let token = html_escape(&s.token);
    format!(
        r##"<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"/>
<meta name="apple-mobile-web-app-capable" content="yes"/>
<title>Signum · {bank}</title>
<style>
  :root {{ color-scheme: dark; }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    background: #0b0b12; color: #f1f5f9; min-height: 100vh;
    padding: 20px 16px 40px;
  }}
  .card {{
    max-width: 420px; margin: 0 auto;
    background: linear-gradient(180deg,#161622,#12121a);
    border: 1px solid rgba(255,255,255,.08);
    border-radius: 20px; padding: 20px;
  }}
  h1 {{ font-size: 1.35rem; margin: 0 0 4px; font-weight: 700; }}
  .sub {{ color: #94a3b8; font-size: .9rem; margin-bottom: 16px; }}
  .guide {{
    background: linear-gradient(180deg, rgba(251,191,36,.14), rgba(251,191,36,.05));
    border: 1px solid rgba(251,191,36,.32);
    border-radius: 16px; padding: 16px; margin-bottom: 16px;
  }}
  .guide-top {{ display:flex; align-items:flex-start; justify-content:space-between; gap:10px; margin-bottom: 12px; }}
  .guide h2 {{ font-size: 1.05rem; margin: 0 0 6px; color: #fffbeb; }}
  .guide .lead {{ font-size: .88rem; line-height: 1.45; color: #fde68a; margin: 0; }}
  .guide-count {{
    flex-shrink:0; border-radius:999px; padding: 4px 10px;
    background: rgba(251,191,36,.18); color:#fef3c7;
    font-size: .75rem; font-weight: 700;
  }}
  .carousel {{ touch-action: pan-y; user-select: none; }}
  .step-slide {{ display:none; }}
  .step-slide.active {{ display:block; }}
  .step-head {{ display:flex; gap:10px; align-items:flex-start; margin-bottom: 10px; }}
  .num {{
    flex-shrink:0; width:28px; height:28px; border-radius:999px;
    background: rgba(251,191,36,.22); color:#fef3c7;
    display:flex; align-items:center; justify-content:center;
    font-size: .85rem; font-weight: 700;
  }}
  .step b {{ display:block; font-size: .95rem; color:#f8fafc; margin-bottom: 2px; }}
  .step p {{ margin:0; font-size: .88rem; line-height: 1.45; color:#cbd5e1; }}
  .step-img {{
    display:block; width:100%; max-width: 360px; margin: 0 auto;
    border-radius: 14px; border: 1px solid rgba(255,255,255,.1);
    background: #0a0a10; object-fit: cover;
  }}
  .step-cap {{
    margin-top: 8px; text-align:center;
    font-size: .75rem; color: #94a3b8;
  }}
  .carousel-nav {{
    display:flex; align-items:center; gap:8px; margin-top: 14px;
  }}
  .nav-btn {{
    border:0; border-radius: 12px; height:44px; min-width:44px;
    font-weight: 600; font-size: .9rem; cursor:pointer;
    display:flex; align-items:center; justify-content:center; gap:4px;
    color:#fff; background: rgba(255,255,255,.08);
    border: 1px solid rgba(255,255,255,.1);
  }}
  .nav-btn:disabled {{ opacity: .35; cursor: default; }}
  .nav-btn.next {{
    flex:1; background: linear-gradient(135deg,#8b5cf6,#6d28d9);
    border-color: transparent;
  }}
  .dots {{ display:flex; flex:1; align-items:center; justify-content:center; gap:6px; }}
  .dot {{
    width:8px; height:8px; border-radius:999px; border:0; padding:0;
    background: rgba(255,255,255,.22); cursor:pointer;
  }}
  .dot.on {{ width:20px; background: #fcd34d; }}
  .note {{
    margin-top: 14px; padding: 12px 14px; border-radius: 12px;
    background: rgba(0,0,0,.28); border: 1px solid rgba(255,255,255,.1);
    font-size: .85rem; line-height: 1.45; color: #cbd5e1;
  }}
  .ios-box {{
    border-radius: 12px; padding: 12px 14px; margin-bottom: 14px;
    font-size: .88rem; line-height: 1.45;
  }}
  .ios-box.ok {{
    background: rgba(16,185,129,.12); border: 1px solid rgba(16,185,129,.3); color: #d1fae5;
  }}
  .ios-box.bad {{
    background: rgba(244,63,94,.12); border: 1px solid rgba(244,63,94,.35); color: #ffe4e6;
  }}
  .ios-box.info {{
    background: rgba(255,255,255,.04); border: 1px solid rgba(255,255,255,.1); color: #cbd5e1;
  }}
  .tip {{
    margin-top: 10px; padding: 10px 12px; border-radius: 12px;
    background: rgba(251,191,36,.12); border: 1px solid rgba(251,191,36,.28);
    font-size: .85rem; line-height: 1.45; color: #fde68a;
  }}
  .status {{
    background: rgba(139,92,246,.12); border: 1px solid rgba(139,92,246,.3);
    border-radius: 14px; padding: 14px; margin-bottom: 16px; font-size: .9rem;
  }}
  .bar {{ height: 6px; background: rgba(255,255,255,.08); border-radius: 99px; overflow: hidden; margin-top: 10px; }}
  .bar > i {{ display:block; height:100%; width:0%; background: linear-gradient(90deg,#8b5cf6,#6d28d9); transition: width .3s; }}
  .field {{ margin-bottom: 12px; }}
  .label {{ font-size: .75rem; text-transform: uppercase; letter-spacing: .06em; color: #64748b; margin-bottom: 6px; }}
  .row {{
    display:flex; gap:8px; align-items:stretch;
    background: rgba(0,0,0,.35); border: 1px solid rgba(255,255,255,.08);
    border-radius: 12px; padding: 12px;
  }}
  .val {{ flex:1; word-break: break-all; font-weight: 600; font-size: 1rem; padding-top: 6px; }}
  button.copy {{
    border:0; border-radius: 10px; padding: 12px 16px; font-weight: 600; font-size: .9rem;
    background: linear-gradient(135deg,#8b5cf6,#6d28d9); color:#fff; cursor:pointer;
    white-space: nowrap;
  }}
  button.copy.ok {{ background: #059669; }}
  .hint {{ color:#64748b; font-size:.8rem; margin-top: 14px; line-height:1.45; text-align:center; }}
</style>
</head>
<body>
  <div class="card">
    <h1>Вход для установки</h1>
    <div class="sub">{bank} · Signum</div>
    <div class="ios-box info" id="iosBox">Проверяем версию iOS…</div>

    <div class="status" id="st">
      <div id="msg">Signum скачивает приложение и ставит на iPhone…</div>
      <div class="bar"><i id="bar"></i></div>
    </div>

    <div class="field">
      <div class="label">Логин (e-mail)</div>
      <div class="row">
        <div class="val" id="login">{login}</div>
        <button class="copy" type="button" data-copy="login">Копировать</button>
      </div>
    </div>
    <div class="field">
      <div class="label">Пароль</div>
      <div class="row">
        <div class="val" id="password">{password}</div>
        <button class="copy" type="button" data-copy="password">Копировать</button>
      </div>
    </div>

    <div class="guide">
      <div class="guide-top">
        <div>
          <h2>Пошаговая инструкция</h2>
          <p class="lead">Листайте шаги — как войти через «Контент и покупки»</p>
        </div>
        <div class="guide-count" id="guideCount">1 / 6</div>
      </div>

      <div class="carousel" id="guideCarousel">
        <div class="step-slide active" data-step="0">
          <div class="step-head">
            <span class="num">1</span>
            <div>
              <b>Скопируйте e-mail и пароль</b>
              <p>Скопируйте данные кнопками выше — они нужны для входа. Действуют около 30 минут.</p>
            </div>
          </div>
          <img class="step-img" alt="Скопируйте e-mail и пароль"
            src="https://manual.sovcombank.ru/_next/static/media/step3.cc5e3bcd.webp"
            referrerpolicy="no-referrer"/>
        </div>

        <div class="step-slide" data-step="1">
          <div class="step-head">
            <span class="num">2</span>
            <div>
              <b>Откройте «Настройки» и перейдите в профиль</b>
              <p>Нажмите на своё имя вверху и выберите «Контент и покупки».</p>
            </div>
          </div>
          <img class="step-img" alt="Настройки → профиль"
            src="https://manual.sovcombank.ru/_next/static/media/step1.0f206336.webp"
            referrerpolicy="no-referrer"/>
          <p class="step-cap">Не выходите из своего аккаунта Apple в настройках телефона</p>
        </div>

        <div class="step-slide" data-step="2">
          <div class="step-head">
            <span class="num">3</span>
            <div>
              <b>Выйдите из раздела «Контент и покупки»</b>
              <p>Выберите раздел → «Выйти» → подтвердите выход.</p>
            </div>
          </div>
          <img class="step-img" alt="Выйти из Контент и покупки"
            src="https://manual.sovcombank.ru/_next/static/media/step2.ae5cd83a.webp"
            referrerpolicy="no-referrer"/>
          <p class="step-cap">Позже при входе в свой Apple ID пароль часто вводить не нужно</p>
        </div>

        <div class="step-slide" data-step="3">
          <div class="step-head">
            <span class="num">4</span>
            <div>
              <b>Настройте раздел «Контент и покупки»</b>
              <p>Снова откройте раздел. Если появится окно — «Не Имя Фамилия?» → «Продолжить» и войдите скопированными e-mail и паролем.</p>
            </div>
          </div>
          <img class="step-img" alt="Настройте Контент и покупки"
            src="https://manual.sovcombank.ru/_next/static/media/step3.54efa08e.webp"
            referrerpolicy="no-referrer"/>
        </div>

        <div class="step-slide" data-step="4">
          <div class="step-head">
            <span class="num">5</span>
            <div>
              <b>После входа: «Другие возможности»</b>
              <p>Выберите «Другие возможности», затем нажмите «Не улучшать».</p>
            </div>
          </div>
          <img class="step-img" alt="Другие возможности"
            src="https://manual.sovcombank.ru/_next/static/media/step4.08e872de.webp"
            referrerpolicy="no-referrer"/>
        </div>

        <div class="step-slide" data-step="5">
          <div class="step-head">
            <span class="num">6</span>
            <div>
              <b>Установите любое бесплатное приложение</b>
              <p>Откройте App Store, найдите любое бесплатное приложение, нажмите «Загрузить» и подтвердите. После начала загрузки активация завершена — это приложение можно удалить.</p>
            </div>
          </div>
          <img class="step-img" alt="Установите любое бесплатное приложение"
            src="https://manual.sovcombank.ru/_next/static/media/accessIsOpen.e754a3c3.png"
            referrerpolicy="no-referrer"/>
          <div class="tip"><b>Если приложение вылетает</b> — попробуйте снова: загрузите <b>другое</b> бесплатное приложение из App Store (потом его можно удалить).</div>
        </div>
      </div>

      <div class="carousel-nav">
        <button type="button" class="nav-btn" id="guidePrev" aria-label="Назад" disabled>‹</button>
        <div class="dots" id="guideDots"></div>
        <button type="button" class="nav-btn next" id="guideNext">Далее ›</button>
      </div>

      <div class="note">Не выходите из Apple ID полностью в «Настройках». Нужен только вход через «Контент и покупки». После активации можно снова выбрать свой аккаунт.</div>
    </div>

    <p class="hint">Страница доступна в вашей Wi‑Fi сети · данные действуют около 30 минут</p>
  </div>
<script>
const token = "{token}";
const bankName = {bank_js};
function toast(btn) {{
  const t = btn.textContent;
  btn.textContent = "Скопировано";
  btn.classList.add("ok");
  setTimeout(() => {{ btn.textContent = t; btn.classList.remove("ok"); }}, 1400);
}}
document.querySelectorAll("button.copy").forEach(btn => {{
  btn.addEventListener("click", async () => {{
    const id = btn.getAttribute("data-copy");
    const text = document.getElementById(id).textContent.trim();
    try {{
      await navigator.clipboard.writeText(text);
      toast(btn);
    }} catch (e) {{
      const ta = document.createElement("textarea");
      ta.value = text; document.body.appendChild(ta); ta.select();
      try {{ document.execCommand("copy"); toast(btn); }} catch(_) {{}}
      document.body.removeChild(ta);
    }}
  }});
}});

(function() {{
  const box = document.getElementById("iosBox");
  if (!box) return;
  const low = (bankName || "").toLowerCase();
  const isRocket = low.includes("рокет") || low.includes("rocket");
  const minIos = isRocket ? "16.0" : "17.0";
  const ua = navigator.userAgent || "";
  let deviceIos = null;
  let m = ua.match(/OS (\d+)[_.](\d+)(?:[_.](\d+))?/i);
  if (m) deviceIos = m[1] + "." + m[2] + (m[3] ? "." + m[3] : "");
  if (!deviceIos) {{
    box.className = "ios-box info";
    box.textContent = "Нужна iOS " + minIos + "+ для " + bankName;
    return;
  }}
  function ok(dev, min) {{
    const p = (v) => v.split(/\D+/).filter(Boolean).map(Number);
    const a = p(dev), b = p(min);
    const n = Math.max(a.length, b.length);
    for (let i = 0; i < n; i++) {{
      if ((a[i] || 0) < (b[i] || 0)) return false;
      if ((a[i] || 0) > (b[i] || 0)) return true;
    }}
    return true;
  }}
  if (ok(deviceIos, minIos)) {{
    box.className = "ios-box ok";
    box.textContent = "iOS " + deviceIos + " — подходит для " + bankName + " (нужна " + minIos + "+)";
  }} else {{
    box.className = "ios-box bad";
    let msg = bankName + " не запустится на iOS " + deviceIos + ". Нужна iOS " + minIos + "+.";
    if (!isRocket && ok(deviceIos, "16.0")) {{
      msg += " Можно поставить Рокетбанк — он с iOS 16.0.";
    }}
    box.textContent = msg;
  }}
}})();

(function() {{
  const slides = Array.from(document.querySelectorAll(".step-slide"));
  const total = slides.length;
  const dotsEl = document.getElementById("guideDots");
  const prevBtn = document.getElementById("guidePrev");
  const nextBtn = document.getElementById("guideNext");
  const countEl = document.getElementById("guideCount");
  const car = document.getElementById("guideCarousel");
  let idx = 0;
  let touchX = null;

  for (let i = 0; i < total; i++) {{
    const d = document.createElement("button");
    d.type = "button";
    d.className = "dot" + (i === 0 ? " on" : "");
    d.setAttribute("aria-label", "Шаг " + (i + 1));
    d.addEventListener("click", () => show(i));
    dotsEl.appendChild(d);
  }}
  const dots = Array.from(dotsEl.children);

  function show(n) {{
    idx = Math.max(0, Math.min(total - 1, n));
    slides.forEach((s, i) => s.classList.toggle("active", i === idx));
    dots.forEach((d, i) => d.classList.toggle("on", i === idx));
    countEl.textContent = (idx + 1) + " / " + total;
    prevBtn.disabled = idx === 0;
    nextBtn.textContent = idx === total - 1 ? "Сначала" : "Далее ›";
  }}

  prevBtn.addEventListener("click", () => show(idx - 1));
  nextBtn.addEventListener("click", () => {{
    if (idx === total - 1) show(0);
    else show(idx + 1);
  }});

  car.addEventListener("touchstart", (e) => {{
    touchX = e.changedTouches[0] ? e.changedTouches[0].clientX : null;
  }}, {{ passive: true }});
  car.addEventListener("touchend", (e) => {{
    if (touchX == null) return;
    const x = e.changedTouches[0] ? e.changedTouches[0].clientX : touchX;
    const dx = x - touchX;
    touchX = null;
    if (Math.abs(dx) < 40) return;
    show(idx + (dx < 0 ? 1 : -1));
  }}, {{ passive: true }});

  show(0);
}})();

async function poll() {{
  try {{
    const r = await fetch("/api/" + token + "?t=" + Date.now());
    if (!r.ok) return;
    const j = await r.json();
    if (j.message) document.getElementById("msg").textContent = j.message;
    const p = Math.max(0, Math.min(100, Number(j.percent) || 0));
    document.getElementById("bar").style.width = p + "%";
    if (j.status === "done") {{
      document.getElementById("msg").textContent = "Готово: приложение установлено на iPhone";
      document.getElementById("bar").style.width = "100%";
    }}
    if (j.status === "failed") {{
      document.getElementById("msg").textContent = j.message || "Ошибка установки — смотрите Signum на компьютере";
    }}
  }} catch (e) {{}}
}}
poll();
setInterval(poll, 2000);
</script>
</body>
</html>
"##
    )
}
