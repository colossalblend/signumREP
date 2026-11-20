//! CI-заглушка: настоящего `pty_login.rs` нет в репозитории. Workflow копирует
//! этот файл в `src/`, только если там нет своего. `None` = «PTY-вход
//! недоступен» — main.rs тогда входит через обычный `run_ipatool_login`.
#![allow(dead_code, unused_variables)]

pub fn cancel_parked(_reason: &str) {}

pub async fn catalog_login_pty<E: std::fmt::Display, C, R>(
    _program: &str,
    _args: &[String],
    _password: &str,
    _email: &str,
    _engine: E,
    _code: C,
    _resending: R,
) -> Option<Result<(), String>> {
    None
}
