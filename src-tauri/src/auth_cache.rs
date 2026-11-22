//! Кэш ответов `ipatool auth info`.
//!
//! `auth info` только расшифровывает локальный файл сессии и в сеть не ходит,
//! но Signum звал его перед каждым действием: открыл страницу приложения,
//! запросил версии, нажал «Установить», обновил страницу «Аккаунт». В отчётах
//! 1.2.2 это больше тысячи запусков ipatool по 200–450 мс каждый, часто по два
//! одновременно.
//!
//! Ответ — чистая функция от (движок, ключ сессии, байты файла сессии), поэтому
//! ключом кэша служит хэш именно этого набора: стоит входу, выходу, смене
//! аккаунта или сбросу хранилища переписать файл — ключ меняется, и следующий
//! вызов снова идёт в ipatool. Кэшируются только успешные ответы; ошибки всегда
//! перепроверяются, чтобы не мешать восстановлению сессии. TTL — страховка на
//! случай, если движок однажды начнёт проверять что-то кроме файла.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const AUTH_INFO_TTL: Duration = Duration::from_secs(10 * 60);

/// Ключ кэша. `None`, когда файла сессии нет вовсе: тогда сессия может жить
/// вне файла (связка ключей macOS у Go-движков), и кэшировать нечего.
pub fn fingerprint(program: &str, secret: &str, session_files: &[Option<Vec<u8>>]) -> Option<u64> {
    if session_files
        .iter()
        .all(|file| file.as_ref().map_or(true, |raw| raw.is_empty()))
    {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    program.hash(&mut hasher);
    secret.hash(&mut hasher);
    for file in session_files {
        file.hash(&mut hasher);
    }
    Some(hasher.finish())
}

#[derive(Debug)]
pub struct TtlCache {
    ttl: Duration,
    entries: HashMap<u64, (String, Instant)>,
}

impl TtlCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, key: u64, now: Instant) -> Option<String> {
        self.entries
            .get(&key)
            .filter(|(_, stored)| now.saturating_duration_since(*stored) < self.ttl)
            .map(|(value, _)| value.clone())
    }

    pub fn put(&mut self, key: u64, value: String, now: Instant) {
        let ttl = self.ttl;
        self.entries
            .retain(|_, (_, stored)| now.saturating_duration_since(*stored) < ttl);
        self.entries.insert(key, (value, now));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

fn cache() -> &'static Mutex<TtlCache> {
    static CACHE: OnceLock<Mutex<TtlCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(TtlCache::new(AUTH_INFO_TTL)))
}

/// Одновременные проверки ждут друг друга, и вторая берёт ответ первой из
/// кэша, а не запускает ещё один ipatool.
pub fn flight() -> &'static tokio::sync::Mutex<()> {
    static FLIGHT: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    FLIGHT.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub fn lookup(key: u64) -> Option<String> {
    cache().lock().ok()?.get(key, Instant::now())
}

pub fn store(key: u64, email: &str) {
    if let Ok(mut cache) = cache().lock() {
        cache.put(key, email.to_string(), Instant::now());
    }
}

pub fn invalidate_all() {
    if let Ok(mut cache) = cache().lock() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_session_file_means_no_cache_key() {
        assert_eq!(fingerprint("ipatool", "k", &[None, None]), None);
        assert_eq!(fingerprint("ipatool", "k", &[Some(Vec::new()), None]), None);
        assert!(fingerprint("ipatool", "k", &[Some(b"x".to_vec()), None]).is_some());
    }

    #[test]
    fn key_changes_with_session_bytes_engine_and_secret() {
        let base = fingerprint("v5/ipatool", "k", &[Some(b"a".to_vec()), None]);
        assert_eq!(base, fingerprint("v5/ipatool", "k", &[Some(b"a".to_vec()), None]));
        assert_ne!(base, fingerprint("v5/ipatool", "k", &[Some(b"b".to_vec()), None]));
        assert_ne!(base, fingerprint("v4/ipatool", "k", &[Some(b"a".to_vec()), None]));
        assert_ne!(base, fingerprint("v5/ipatool", "other", &[Some(b"a".to_vec()), None]));
        // The same bytes in the other slot (keyring/account) is a different store.
        assert_ne!(base, fingerprint("v5/ipatool", "k", &[None, Some(b"a".to_vec())]));
    }

    #[test]
    fn entries_expire_after_ttl() {
        let mut cache = TtlCache::new(Duration::from_secs(60));
        let t0 = Instant::now();
        cache.put(1, "a@b.c".into(), t0);
        assert_eq!(cache.get(1, t0 + Duration::from_secs(59)).as_deref(), Some("a@b.c"));
        assert_eq!(cache.get(1, t0 + Duration::from_secs(60)), None);
        assert_eq!(cache.get(2, t0), None);
    }

    #[test]
    fn put_prunes_expired_and_clear_empties() {
        let mut cache = TtlCache::new(Duration::from_secs(10));
        let t0 = Instant::now();
        cache.put(1, "old".into(), t0);
        cache.put(2, "new".into(), t0 + Duration::from_secs(20));
        assert_eq!(cache.entries.len(), 1);
        cache.clear();
        assert_eq!(cache.get(2, t0 + Duration::from_secs(20)), None);
    }
}
