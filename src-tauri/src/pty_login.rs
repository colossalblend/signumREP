//! Вход в Apple ID через ConPTY: один процесс ipatool-cpp на оба шага 2FA.
//!
//! Двухпроцессная схема остаётся откатом. Здесь первый запуск `auth login`
//! идёт через псевдотерминал (ConPTY): движок доходит до своей подсказки
//! «Enter 2FA code» и живёт, Signum отдаёт в интерфейс «нужен код» как раньше,
//! а повторный вызов дописывает код в живую сессию. Код проверяется Apple в той
//! же сессии, в которой был отправлен пуш, — как при входе из IPA_Downloader,
//! который запускает ipatool в настоящем терминале. Второй парольный шаг Apple
//! не получает вовсе: меньше запросов, меньше риска лимита.
//!
//! Откаты, когда псевдотерминал недоступен (не Windows, ConPTY не создался,
//! живая сессия не дожила до кода) — классический второй запуск с --auth-code.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as StdMutex, OnceLock,
};
use std::time::{Duration, Instant};

/// Общий лимит жизни сессии входа, включая ожидание кода от человека.
const OVERALL_TIMEOUT: Duration = Duration::from_secs(600);
/// Как часто опрашиваем вывод движка и канал с кодом.
const POLL: Duration = Duration::from_millis(150);
/// Сколько ждём завершения процесса после kill.
const TERM_WAIT_MS: u32 = 5000;
/// Подсказки, на которых ipatool-cpp ждёт код из stdin (main.cpp, cmd_login).
const PROMPT_MARKERS: [&str; 2] = ["enter 2fa code", "enter verification code"];

// ── Низкоуровневые помощники (кросс-платформенные ради тестов) ──────────────

/// Кавычки в стиле Windows CreateProcess: аргумент целиком или с пробелами,
/// кавычками и обратными слэшами. Правила CommandLineToArgvW: серия из 2n
/// слэшей перед кавычкой разбирается как n слэшей и кавычка-разделитель,
/// из 2n+1 — как n слэшей и литеральная кавычка. Поэтому литеральную кавычку
/// кодируем как 2n+1 слэш + кавычка, а хвостовые слэши удваиваем.
fn quote_arg(value: &str) -> String {
    if !value.is_empty()
        && !value
            .chars()
            .any(|c| c == ' ' || c == '\t' || c == '"' || c == '\n' || c == '\r')
    {
        return value.to_string();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    let mut backslashes = 0usize;
    for c in value.chars() {
        match c {
            '\\' => {
                backslashes += 1;
                quoted.push('\\');
            }
            '"' => {
                // Серия уже записана в вывод (count штук); дописываем ещё
                // столько же и экранируем кавычку: 2n+1 слэш → литеральная.
                for _ in 0..backslashes {
                    quoted.push('\\');
                }
                backslashes = 0;
                quoted.push('\\');
                quoted.push('"');
            }
            _ => {
                backslashes = 0;
                quoted.push(c);
            }
        }
    }
    // Хвостовые слэши удваиваем: в выводе уже count, дописываем ещё count.
    for _ in 0..backslashes {
        quoted.push('\\');
    }
    quoted.push('"');
    quoted
}

/// Командная строка для CreateProcessW: программа плюс аргументы.
fn command_line(program: &str, args: &[String]) -> String {
    let mut line = quote_arg(program);
    for arg in args {
        line.push(' ');
        line.push_str(&quote_arg(arg));
    }
    line
}

fn utf16_null(value: &str) -> Vec<u16> {
    let mut out: Vec<u16> = value.encode_utf16().collect();
    out.push(0);
    out
}

/// Блок окружения для CreateProcessW (CREATE_UNICODE_ENVIRONMENT).
///
/// Начинаем с окружения Signum и накладываем правки из std::process::Command:
/// `env()` переопределяет, `env_remove()` стирает. Имена на Windows не зависят
/// от регистра, поэтому сверяемся без учёта регистра и сортируем так же.
fn environment_block<'a, I>(overrides: I) -> Option<Vec<u16>>
where
    I: Iterator<Item = (&'a OsStr, Option<&'a OsStr>)>,
{
    // Родительское окружение; повторы differing only in case схлопываем.
    let mut entries: Vec<(String, String)> = Vec::new();
    for (key, value) in std::env::vars() {
        if key.is_empty() || key.contains('=') || value.contains('\0') {
            continue;
        }
        if !entries
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(&key))
        {
            entries.push((key, value));
        }
    }
    let apply = |entries: &mut Vec<(String, String)>, key: String, value: Option<String>| {
        if key.is_empty() || key.contains('=') {
            return;
        }
        let at = entries
            .iter()
            .position(|(name, _)| name.eq_ignore_ascii_case(&key));
        match (value, at) {
            (Some(value), Some(at)) => entries[at].1 = value,
            (Some(value), None) => entries.push((key, value)),
            (None, Some(at)) => {
                entries.remove(at);
            }
            (None, None) => {}
        }
    };
    for (key, value) in overrides {
        let key = key.to_string_lossy().to_string();
        match value {
            Some(value) if !value.to_string_lossy().contains('\0') => {
                apply(&mut entries, key, Some(value.to_string_lossy().to_string()));
            }
            Some(_) => {}
            None => apply(&mut entries, key, None),
        }
    }
    entries.sort_by(|a, b| a.0.to_ascii_uppercase().cmp(&b.0.to_ascii_uppercase()));
    let mut block = String::new();
    for (key, value) in entries {
        block.push_str(&key);
        block.push('=');
        block.push_str(&value);
        block.push('\0');
    }
    if block.is_empty() {
        block.push('\0');
    }
    let mut out: Vec<u16> = block.encode_utf16().collect();
    out.push(0);
    Some(out)
}

/// По подсказке в выводе движка определяем, что он ждёт код из stdin.
fn looks_like_2fa_prompt(lowered: &str) -> bool {
    PROMPT_MARKERS.iter().any(|marker| lowered.contains(marker))
}

// ── Реализация: только Windows (ConPTY) ─────────────────────────────────────

#[cfg(target_os = "windows")]
mod imp {
    use super::*;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
    use windows_sys::Win32::System::Console::{ClosePseudoConsole, CreatePseudoConsole, COORD, HPCON};
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
        WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
        LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, STARTUPINFOEXW,
    };

    /// HANDLE в windows-sys — сырой указатель, не Send. В задачи уезжает как
    /// usize, а здесь возвращается обратно (`x as HANDLE`).
    const STILL_ACTIVE: u32 = 259;

    // ── Парковка живой сессии ────────────────────────────────────────────────

    struct ParkedLogin {
        email: String,
        engine: String,
        code_tx: tokio::sync::mpsc::Sender<String>,
        cancel: Arc<AtomicBool>,
        task: tokio::task::JoinHandle<Result<(), String>>,
    }

    fn parked_slot() -> &'static StdMutex<Option<ParkedLogin>> {
        static SLOT: OnceLock<StdMutex<Option<ParkedLogin>>> = OnceLock::new();
        SLOT.get_or_init(|| StdMutex::new(None))
    }

    /// Живая сессия входа больше не нужна: завершаем процесс.
    /// Вызывается перед новым входом и при сбросе локального стора.
    pub(super) fn cancel_parked(reason: &str) {
        let parked = parked_slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(parked) = parked {
            parked.cancel.store(true, Ordering::Relaxed);
            crate::ipatool_debug_log(&format!("pty-login cancel parked session: {reason}"));
        }
    }

    /// Живая сессия именно этого входа; мёртвую или чужую убираем.
    fn take_live_parked(email: &str, engine: &str) -> Option<ParkedLogin> {
        let mut slot = parked_slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let parked = slot.take()?;
        if parked.task.is_finished() {
            return None;
        }
        if parked.email != email || parked.engine != engine {
            parked.cancel.store(true, Ordering::Relaxed);
            return None;
        }
        Some(parked)
    }

    fn put_parked(parked: ParkedLogin) {
        let mut slot = parked_slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(old) = slot.replace(parked) {
            old.cancel.store(true, Ordering::Relaxed);
        }
    }

    struct ParkInfo {
        code_tx: tokio::sync::mpsc::Sender<String>,
        cancel: Arc<AtomicBool>,
    }

    // ── Точка входа ──────────────────────────────────────────────────────────

    /// Вход через ConPTY. `None` — pty не применим, вызывающая сторона делает
    /// классический двухпроцессный запуск.
    pub(super) async fn catalog_login_pty(
        program: &str,
        args: &[String],
        password: &str,
        email: &str,
        engine: &str,
        code: Option<&str>,
        resending: bool,
    ) -> Option<Result<(), String>> {
        if !crate::is_cpp_ipatool(program) {
            return None;
        }
        if let Some(code) = code {
            // Повторный вызов с кодом: код дописываем в живую сессию, если она
            // дожила. Иначе — классический второй запуск с --auth-code.
            let Some(parked) = take_live_parked(email, engine) else {
                return None;
            };
            return Some(continue_parked(parked, code).await);
        }
        if resending {
            // Повторная отправка кода — новый полный вход, пуш уйдёт заново.
            cancel_parked("resend code");
        } else {
            cancel_parked("fresh login");
        }
        fresh_login_pty(program, args, password, email, engine).await
    }

    async fn continue_parked(
        parked: ParkedLogin,
        code: &str,
    ) -> Result<(), String> {
        crate::ipatool_debug_log("pty-login continue: writing 2FA code into live session");
        if parked.code_tx.send(code.to_string()).await.is_err() {
            return Err("Сессия входа завершилась раньше кода. Войдите заново.".into());
        }
        match parked.task.await {
            Ok(result) => result,
            Err(join) => Err(format!("Сессия входа прервана: {join}")),
        }
    }

    async fn fresh_login_pty(
        program: &str,
        args: &[String],
        password: &str,
        email: &str,
        engine: &str,
    ) -> Option<Result<(), String>> {
        // ipatool-cpp не принимает --non-interactive (как и в run_process_inner).
        let args: Vec<String> = args
            .iter()
            .filter(|arg| arg.as_str() != "--non-interactive")
            .cloned()
            .collect();
        let program_path = crate::windows_plain_path(Path::new(program));
        let mut command = std::process::Command::new(&program_path);
        command.args(&args);
        crate::apply_ipatool_child_env(&mut command, program, &program_path);

        let session = match unsafe { spawn_conpty(&command) } {
            Ok(session) => session,
            Err(error) => {
                crate::ipatool_debug_log(&format!("pty-login unavailable: {error}"));
                return None;
            }
        };
        crate::ipatool_debug_log(&format!(
            "pty-login spawn ok args=[{}] {}",
            crate::redact_cmd_args(&args),
            crate::ipatool_session_diag()
        ));

        let cancel = Arc::new(AtomicBool::new(false));
        let (park_tx, park_rx) = tokio::sync::oneshot::channel::<ParkInfo>();
        let mut inner = tokio::spawn(run_flow(
            session,
            cancel.clone(),
            park_tx,
            password.to_string(),
        ));

        tokio::select! {
            park = park_rx => match park {
                Ok(info) => {
                    put_parked(ParkedLogin {
                        email: email.to_string(),
                        engine: engine.to_string(),
                        code_tx: info.code_tx,
                        cancel: info.cancel,
                        task: inner,
                    });
                    crate::ipatool_debug_log("pty-login parked at 2FA prompt");
                    // Тот же текст, что вернул бы движок без TTY: UI показывает
                    // поле ввода кода, сессия тем временем жива.
                    Some(Err(crate::format_login_error(
                        "2FA code is required: the engine waits for the code in a live ConPTY session",
                        password,
                    )))
                }
                Err(_) => match inner.await {
                    Ok(result) => Some(result),
                    Err(join) => Some(Err(format!("Вход прерван: {join}"))),
                },
            },
            result = &mut inner => match result {
                Ok(result) => Some(result),
                Err(join) => Some(Err(format!("Вход прерван: {join}"))),
            },
        }
    }

    // ── Жизненный цикл одной сессии ──────────────────────────────────────────

    async fn run_flow(
        mut session: ConPtySession,
        cancel: Arc<AtomicBool>,
        park_tx: tokio::sync::oneshot::Sender<ParkInfo>,
        password: String,
    ) -> Result<(), String> {
        let started = Instant::now();
        let mut parked: Option<tokio::sync::mpsc::Receiver<String>> = None;
        let mut code_sent: Option<String> = None;
        let mut park_tx = Some(park_tx);

        loop {
            if cancel.load(Ordering::Relaxed) {
                session.kill();
                crate::ipatool_debug_log("pty-login cancelled");
                return Err("Вход отменён.".into());
            }
            if started.elapsed() >= OVERALL_TIMEOUT {
                session.kill();
                let text = masked_output(&session, code_sent.as_deref());
                crate::ipatool_debug_log(&format!(
                    "pty-login timeout elapsed_ms={} output={}",
                    started.elapsed().as_millis(),
                    crate::technical_excerpt(&text, 800)
                ));
                return Err(if parked.is_some() {
                    "Код подтверждения не введён вовремя. Войдите заново.".into()
                } else {
                    crate::format_login_error(&text, &password)
                });
            }

            let lowered = lowered_output(&session);
            if let Some(code_rx) = parked.as_mut() {
                if code_sent.is_none() {
                    if let Ok(code) = code_rx.try_recv() {
                        let payload = format!("{code}\r");
                        if let Err(error) = session.write(payload.as_bytes()).await {
                            session.kill();
                            return Err(format!("Не удалось передать код в сессию входа: {error}"));
                        }
                        code_sent = Some(code);
                        crate::ipatool_debug_log("pty-login 2FA code written to live session");
                    }
                }
            } else if looks_like_2fa_prompt(&lowered) {
                let (code_tx, code_rx) = tokio::sync::mpsc::channel(2);
                if let Some(tx) = park_tx.take() {
                    let _ = tx.send(ParkInfo {
                        code_tx,
                        cancel: cancel.clone(),
                    });
                }
                parked = Some(code_rx);
            }

            if session.eof.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(POLL).await;
        }

        let exit_code = session.wait_exit().await;
        let text = masked_output(&session, code_sent.as_deref());
        crate::ipatool_debug_log(&format!(
            "pty-login exit code={exit_code:?} elapsed_ms={} output={}",
            started.elapsed().as_millis(),
            crate::technical_excerpt(&text, 1200)
        ));
        let clean = crate::strip_ansi(&text);
        if exit_code == Some(0)
            && !crate::login_output_indicates_failure(&clean)
            && !crate::is_two_factor_error(&clean)
        {
            return Ok(());
        }
        Err(crate::format_login_error(&text, &password))
    }

    fn raw_output(session: &ConPtySession) -> String {
        let buffer = session
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        String::from_utf8_lossy(&buffer).into_owned()
    }

    fn lowered_output(session: &ConPtySession) -> String {
        crate::strip_ansi(&raw_output(session)).to_ascii_lowercase()
    }

    /// Вывод для журнала: пароль форматируется как обычно, введённый код
    /// маскируем — ConPTY его эхом возвращает в тот же поток.
    fn masked_output(session: &ConPtySession, code: Option<&str>) -> String {
        let text = raw_output(session);
        match code {
            Some(code) if !code.is_empty() => text.replace(code, "******"),
            _ => text,
        }
    }

    // ── ConPTY ───────────────────────────────────────────────────────────────

    struct ConPtySession {
        hpc: HPCON,
        input_write: HANDLE,
        output_read: HANDLE,
        process: HANDLE,
        buffer: Arc<StdMutex<Vec<u8>>>,
        eof: Arc<AtomicBool>,
    }

    // Сессию держит одна задача: хендлы внутри — сырые указатели, но доступ
    // идёт только из владельца (или через Arc<StdMutex> для буфера чтения).
    unsafe impl Send for ConPtySession {}

    impl ConPtySession {
        async fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
            // Хендлы — сырые указатели, поэтому в задачи уезжают как usize.
            let handle = self.input_write as usize;
            let payload = bytes.to_vec();
            tokio::task::spawn_blocking(move || unsafe {
                let mut written: u32 = 0;
                let ok = WriteFile(
                    handle as HANDLE,
                    payload.as_ptr(),
                    payload.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                );
                if ok == 0 {
                    Err(format!("WriteFile: {}", std::io::Error::last_os_error()))
                } else {
                    Ok(())
                }
            })
            .await
            .map_err(|join| format!("writer task: {join}"))?
        }

        async fn wait_exit(&mut self) -> Option<u32> {
            let handle = self.process as usize;
            tokio::task::spawn_blocking(move || unsafe {
                WaitForSingleObject(handle as HANDLE, TERM_WAIT_MS);
                let mut code: u32 = 0;
                if GetExitCodeProcess(handle as HANDLE, &mut code) != 0 && code != STILL_ACTIVE {
                    Some(code)
                } else {
                    None
                }
            })
            .await
            .unwrap_or(None)
        }

        fn kill(&mut self) {
            unsafe {
                TerminateProcess(self.process, 1);
            }
        }
    }

    impl Drop for ConPtySession {
        fn drop(&mut self) {
            // Если приложение закрывается, пока сессия жива, — добиваем
            // процесс: задача runtime уже остановлена, кода никто не введёт.
            self.kill();
            unsafe {
                // Закрытие псевдоконсоли завершает conhost, и чтение на другом
                // потоке разблокируется.
                ClosePseudoConsole(self.hpc);
                let deadline = Instant::now() + Duration::from_secs(1);
                while !self.eof.load(Ordering::Relaxed) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
                CloseHandle(self.input_write);
                CloseHandle(self.output_read);
                CloseHandle(self.process);
            }
        }
    }

    unsafe fn spawn_conpty(command: &std::process::Command) -> Result<ConPtySession, String> {
        // Список атрибутов под один PSEUDOCONSOLE.
        let mut attr_size: usize = 0;
        if InitializeProcThreadAttributeList(
            std::ptr::null_mut(),
            1,
            0,
            &mut attr_size,
        ) == 0
            && attr_size == 0
        {
            return Err(format!(
                "InitializeProcThreadAttributeList: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut attr_buf = vec![0u8; attr_size];
        let attr_list = attr_buf.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        if InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_size) == 0 {
            return Err(format!(
                "InitializeProcThreadAttributeList: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Две трубы: в псевдоконсоль (наш ввод) и из неё (вывод движка).
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 0,
        };
        let mut input_read: HANDLE = std::ptr::null_mut();
        let mut input_write: HANDLE = std::ptr::null_mut();
        let mut output_read: HANDLE = std::ptr::null_mut();
        let mut output_write: HANDLE = std::ptr::null_mut();
        if CreatePipe(&mut input_read, &mut input_write, &sa, 0) == 0 {
            return Err(format!(
                "CreatePipe(input): {}",
                std::io::Error::last_os_error()
            ));
        }
        if CreatePipe(&mut output_read, &mut output_write, &sa, 0) == 0 {
            CloseHandle(input_read);
            CloseHandle(input_write);
            return Err(format!(
                "CreatePipe(output): {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut hpc: HPCON = std::mem::zeroed();
        let size = COORD { X: 120, Y: 40 };
        let hr = CreatePseudoConsole(size, input_read, output_write, 0, &mut hpc);
        // Эти концы нужны только псевдоконсоли.
        CloseHandle(input_read);
        CloseHandle(output_write);
        if hr != 0 {
            DeleteProcThreadAttributeList(attr_list);
            return Err(format!(
                "CreatePseudoConsole: HRESULT {hr:#x} (нужен Windows 10 1809 или новее)"
            ));
        }

        if UpdateProcThreadAttribute(
            attr_list,
            0,
            windows_sys::Win32::System::Threading::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            &mut hpc as *mut HPCON as *const std::ffi::c_void,
            std::mem::size_of::<HPCON>(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == 0
        {
            let error = std::io::Error::last_os_error();
            ClosePseudoConsole(hpc);
            DeleteProcThreadAttributeList(attr_list);
            return Err(format!("UpdateProcThreadAttribute: {error}"));
        }

        let program = command.get_program().to_string_lossy().to_string();
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        let mut command_line_utf16 = utf16_null(&command_line(&program, &args));
        let environment = environment_block(command.get_envs());
        let cwd_utf16 = command
            .get_current_dir()
            .map(|dir| utf16_null(&dir.to_string_lossy()));

        let mut startup: STARTUPINFOEXW = std::mem::zeroed();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        // Belt and braces with CREATE_NO_WINDOW: hide the window if the console
        // host decides to create one anyway.
        const STARTF_USESHOWWINDOW: u32 = 0x0000_0001;
        const SW_HIDE: u16 = 0;
        startup.StartupInfo.dwFlags = STARTF_USESHOWWINDOW;
        startup.StartupInfo.wShowWindow = SW_HIDE;
        startup.lpAttributeList = attr_list;
        let mut process_info: PROCESS_INFORMATION = std::mem::zeroed();

        // CREATE_NO_WINDOW is essential here: without it conhost behind the
        // pseudoconsole could surface a real console window, and users saw the
        // engine's raw «Enter 2FA code:» prompt there instead of the app's own
        // 6-digit field. The prompt must stay in the hidden ConPTY pipe, where
        // run_flow() notices it and hands the code entry back to the UI.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let ok = CreateProcessW(
            std::ptr::null(),
            command_line_utf16.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            environment
                .as_ref()
                .map(|block| block.as_ptr() as *const std::ffi::c_void)
                .unwrap_or(std::ptr::null()),
            cwd_utf16
                .as_ref()
                .map(|block| block.as_ptr())
                .unwrap_or(std::ptr::null()),
            &startup.StartupInfo,
            &mut process_info,
        );
        DeleteProcThreadAttributeList(attr_list);
        if ok == 0 {
            let error = std::io::Error::last_os_error();
            ClosePseudoConsole(hpc);
            return Err(format!("CreateProcessW: {error}"));
        }
        CloseHandle(process_info.hThread);

        // Читаем вывод движка в фоне: как только conhost отдаст конец трубы
        // (после закрытия псевдоконсоли), поток выйдет сам.
        let buffer = Arc::new(StdMutex::new(Vec::new()));
        let eof = Arc::new(AtomicBool::new(false));
        // Хендлы — сырые указатели, не Send: поток чтения получает копию как usize.
        let reader_handle = output_read as usize;
        {
            let buffer = buffer.clone();
            let eof = eof.clone();
            std::thread::spawn(move || unsafe {
                let mut chunk = [0u8; 4096];
                loop {
                    let mut got: u32 = 0;
                    let ok = ReadFile(
                        reader_handle as HANDLE,
                        chunk.as_mut_ptr(),
                        chunk.len() as u32,
                        &mut got,
                        std::ptr::null_mut(),
                    );
                    if ok == 0 || got == 0 {
                        break;
                    }
                    buffer
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .extend_from_slice(&chunk[..got as usize]);
                }
                eof.store(true, Ordering::Relaxed);
            });
        }

        Ok(ConPtySession {
            hpc,
            input_write,
            output_read,
            process: process_info.hProcess,
            buffer,
            eof,
        })
    }
}

// ── Обёртки для остальных платформ: классический путь остаётся ──────────────

#[cfg(target_os = "windows")]
pub(crate) async fn catalog_login_pty(
    program: &str,
    args: &[String],
    password: &str,
    email: &str,
    engine: &str,
    code: Option<&str>,
    resending: bool,
) -> Option<Result<(), String>> {
    imp::catalog_login_pty(program, args, password, email, engine, code, resending).await
}

/// Живая сессия входа больше не нужна. На других платформах живых сессий нет.
pub(crate) fn cancel_parked(reason: &str) {
    #[cfg(target_os = "windows")]
    imp::cancel_parked(reason);
    #[cfg(not(target_os = "windows"))]
    let _ = reason;
}

#[cfg(not(target_os = "windows"))]
pub(crate) async fn catalog_login_pty(
    _program: &str,
    _args: &[String],
    _password: &str,
    _email: &str,
    _engine: &str,
    _code: Option<&str>,
    _resending: bool,
) -> Option<Result<(), String>> {
    None
}

// ── Тесты ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_args_stay_unquoted() {
        assert_eq!(quote_arg("auth"), "auth");
        assert_eq!(quote_arg("-e"), "-e");
        assert_eq!(quote_arg("user@example.com"), "user@example.com");
    }

    #[test]
    fn spaced_args_are_quoted() {
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg(""), "\"\"");
    }

    #[test]
    fn backslashes_before_quotes_follow_argv_rules() {
        // a,"b → кавычка литеральная, слэшей перед ней не было: \"
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
        // a,\"b → один слэш удваивается перед экранированной кавычкой: \\\"\"
        assert_eq!(quote_arg("a\\\"b"), "\"a\\\\\\\"b\"");
        // Слэш без кавычки рядом остаётся одиночным — аргумент даже не в кавычках.
        assert_eq!(quote_arg("a\\b"), "a\\b");
        // Слэш перед пробелом (не перед кавычкой) остаётся одиночным.
        assert_eq!(quote_arg("a\\ "), "\"a\\ \"");
        // Одиночный хвостовой слэш без пробелов и кавычек не требует кавычек:
        // он всё равно литеральный.
        assert_eq!(quote_arg("a\\"), "a\\");
        // А внутри кавычек хвостовой слэш удваивается перед закрывающей.
        assert_eq!(quote_arg("a b\\"), "\"a b\\\\\"");
    }

    #[test]
    fn command_line_joins_program_and_args() {
        let line = command_line(r"C:\Program Files\Signum\v5\ipatool.exe", &[
            "auth".into(),
            "login".into(),
            "-e".into(),
            "user@example.com".into(),
        ]);
        assert_eq!(
            line,
            "\"C:\\Program Files\\Signum\\v5\\ipatool.exe\" auth login -e user@example.com"
        );
    }

    #[test]
    fn environment_block_overrides_and_removes() {
        let mut command = std::process::Command::new("cmd");
        command.env("SIGNUM_PTY_TEST_SET", "1");
        let existing = std::env::var_os("PATH").is_some();
        command.env_remove("PATH");
        let block = environment_block(command.get_envs()).unwrap();
        let text = String::from_utf16(&block).unwrap();
        assert!(text.contains("SIGNUM_PTY_TEST_SET=1\0"));
        let entries: Vec<&str> = text.trim_end_matches('\0').split('\0').collect();
        if existing {
            assert!(!entries
                .iter()
                .any(|entry| entry
                    .split('=')
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("PATH"))));
        }
        // Блок обязан заканчиваться двойным NUL.
        assert!(text.ends_with('\0'));
    }

    #[test]
    fn environment_block_is_sorted_case_insensitive() {
        let mut command = std::process::Command::new("cmd");
        command.env("ZZZ_PTY", "1");
        command.env("aaa_PTY", "2");
        command.env("MMM_PTY", "3");
        let block = environment_block(command.get_envs()).unwrap();
        let text = String::from_utf16(&block).unwrap();
        let zzz = text.find("ZZZ_PTY=").unwrap();
        let mmm = text.find("MMM_PTY=").unwrap();
        let aaa = text.find("aaa_PTY=").unwrap();
        // Регистронезависимая сортировка: AAA < MMM < ZZZ, а имя остаётся как задали.
        assert!(aaa < mmm && mmm < zzz);
    }

    #[test]
    fn prompt_markers_are_recognized() {
        assert!(looks_like_2fa_prompt("...enter 2fa code: "));
        assert!(looks_like_2fa_prompt("please enter verification code"));
        assert!(!looks_like_2fa_prompt("login error: wrong password"));
        assert!(!looks_like_2fa_prompt("success\": true"));
    }
}
