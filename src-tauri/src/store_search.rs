                                                                                 

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::{sync::OnceLock, time::Duration};

const SEARCH_URL: &str = "https://appmagic.rocks/api/v2/search";
const SEARCH_LIMIT: u32 = 100;
const RESULT_LIMIT: usize = 100;
const ICON_LOOKUP_CONCURRENCY: usize = 8;
const ICON_LOOKUP_LIMIT: usize = 24;
const ICON_LOOKUP_TIMEOUT_SECS: u64 = 4;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStoreAppsInput {
    query: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSearchApp {
    name: String,
    publisher: String,
    icon_url: String,
    app_store_id: String,
                                                                     
    removed_from_store: bool,
    found_on_account: bool,
}

#[tauri::command]
pub async fn search_store_apps(app: tauri::AppHandle, input: SearchStoreAppsInput) -> Result<Vec<StoreSearchApp>, String> {
    let Some(query) = normalize_store_search_query(&input.query) else {
        return Ok(Vec::new());
    };
    let variants = search_query_variants(&query);
    let appmagic_requests = futures_util::future::join_all(
        variants
            .iter()
            .map(|variant| fetch_search_json(variant)),
    );
    // AppMagic быстрее для полнотекстового поиска, но его база может отставать
    // от App Store. iTunes — источник истины для ID и свежих приложений.
    let (responses, itunes) = tokio::join!(appmagic_requests, itunes_search_apps(&query));
    let mut apps = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut first_error = None;
    for response in responses {
        let value = match response {
            Ok(value) => value,
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
                continue;
            }
        };
        for app in parse_search_apps(&value, &query) {
            if seen.insert(app.app_store_id.clone()) {
                apps.push(app);
            }
        }
        if apps.len() >= RESULT_LIMIT {
            break;
        }
    }
    for app in itunes {
        if let Some(existing) = apps.iter_mut().find(|item: &&mut StoreSearchApp| item.app_store_id == app.app_store_id) {
            if existing.icon_url.is_empty() { existing.icon_url = app.icon_url; }
            if existing.publisher.is_empty() { existing.publisher = app.publisher; }
        } else if seen.insert(app.app_store_id.clone()) {
            apps.push(app);
        }
    }
    if needs_account_lookup(&query, &apps)
        && super::find_app_id_on_current_account(&app, &query).await?
    {
        apps.insert(0, StoreSearchApp {
            name: format!("Приложение {query}"),
            publisher: String::new(),
            icon_url: String::new(),
            app_store_id: query.clone(),
            removed_from_store: false,
            found_on_account: true,
        });
    }
    if apps.is_empty() {
        if let Some(error) = first_error {
            return Err(error);
        }
    }
    apps.truncate(RESULT_LIMIT);
    fill_ios_icons(&mut apps).await;
    Ok(apps)
}

fn needs_account_lookup(query: &str, apps: &[StoreSearchApp]) -> bool {
    is_adam_id(query) && !apps.iter().any(|app| app.app_store_id == query)
}

async fn itunes_search_apps(query: &str) -> Vec<StoreSearchApp> {
    let Ok(client) = http_client() else { return Vec::new(); };
    let responses = if query.chars().all(|ch| ch.is_ascii_digit()) {
        match client.get(ITUNES_LOOKUP).query(&[("id", query), ("entity", "software")]).send().await {
            Ok(response) if response.status().is_success() => response.json::<JsonValue>().await.ok()
                .map(|value| parse_itunes_apps(&value)).unwrap_or_default(),
            _ => Vec::new(),
        }
    } else {
        futures_util::future::join_all(["ru", "us"].into_iter().map(|country| async move {
            let Ok(response) = client.get(ITUNES_SEARCH).query(&[
                ("term", query), ("entity", "software"), ("country", country), ("limit", "50"),
            ]).send().await else { return Vec::new(); };
            if !response.status().is_success() { return Vec::new(); }
            response.json::<JsonValue>().await.ok().map(|value| parse_itunes_apps(&value)).unwrap_or_default()
        })).await.into_iter().flatten().collect()
    };
    let mut seen = std::collections::HashSet::new();
    responses.into_iter().filter(|app| seen.insert(app.app_store_id.clone())).collect()
}

fn http_client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    match CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .connect_timeout(Duration::from_secs(8))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| format!("HTTP client: {error}"))
    }) {
        Ok(client) => Ok(client),
        Err(error) => Err(error.clone()),
    }
}

async fn fetch_search_json(query: &str) -> Result<JsonValue, String> {
    let client = http_client()?;
    let response = client
        .get(SEARCH_URL)
        .query(&[("name", query), ("limit", &SEARCH_LIMIT.to_string())])
        .header("accept", "application/json, text/plain, */*")
        .header("accept-language", "ru,en;q=0.9")
        .header("origin", "https://appmagic.rocks")
        .header("referer", "https://appmagic.rocks/")
        .send()
        .await
        .map_err(|e| {
            crate::diag_log::http_err("GET", SEARCH_URL, &e.to_string());
            "Не удалось выполнить поиск. Проверьте интернет и повторите.".to_string()
        })?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        crate::diag_log::http("GET", SEARCH_URL, status, "", b"");
        return Err("Не удалось выполнить поиск. Повторите попытку.".into());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "Не удалось выполнить поиск. Повторите попытку.".to_string())?;
    crate::diag_log::http("GET", SEARCH_URL, status, "", &bytes);
    if bytes.starts_with(b"<") || bytes.starts_with(b"<!DOCTYPE") {
        return Err("Не удалось выполнить поиск. Повторите попытку.".into());
    }
    serde_json::from_slice::<JsonValue>(&bytes)
        .map_err(|_| "Не удалось обработать результаты поиска.".to_string())
}

fn parse_search_apps(value: &JsonValue, query: &str) -> Vec<StoreSearchApp> {
    let Some(apps) = value.get("applications").and_then(|item| item.as_array()) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for app in apps {
        let Some(item) = store_app_from_json(app) else {
            continue;
        };
        if !seen.insert(item.app_store_id.clone()) {
            continue;
        }
        out.push(item);
        if out.len() >= RESULT_LIMIT {
            break;
        }
    }
    if query.chars().all(|ch| ch.is_ascii_digit()) {
        out.sort_by_key(|item| if item.app_store_id == query { 0 } else { 1 });
    }
    out
}

fn store_app_from_json(app: &JsonValue) -> Option<StoreSearchApp> {
    let adam = ios_adam_id(app)?;
    let name = json_string(app, "name")
        .or_else(|| first_names_array(app))
        .filter(|value| !value.is_empty())?;
    let publisher = app
        .get("unitedPublisher")
        .and_then(|node| json_string(node, "name"))
        .or_else(|| json_string(app, "publisher_name"))
        .unwrap_or_default();
    let icon_url = prefer_ios_icon(app);
    Some(StoreSearchApp {
        name,
        publisher,
        icon_url,
        app_store_id: adam,
        removed_from_store: removed_from_app_store(app),
        found_on_account: false,
    })
}

                                                                                 
fn ios_adam_id(app: &JsonValue) -> Option<String> {
    if let Some(ids) = app.get("store_ids").and_then(|item| item.as_array()) {
        for prefix in ["2_", "3_"] {
            for item in ids {
                let Some(raw) = item.as_str() else {
                    continue;
                };
                if let Some(rest) = raw.strip_prefix(prefix) {
                    if is_adam_id(rest) {
                        return Some(rest.to_string());
                    }
                }
            }
        }
    }
    None
}

fn store_list_has(node: &JsonValue, key: &str, store: i64) -> bool {
    node.get(key)
        .and_then(|item| item.as_array())
        .map(|list| {
            list.iter()
                .any(|item| item.as_i64() == Some(store) || item.as_u64() == Some(store as u64))
        })
        .unwrap_or(false)
}

fn removed_from_app_store(app: &JsonValue) -> bool {
    if app
        .get("removed")
        .and_then(|item| item.as_bool())
        .unwrap_or(false)
    {
        return true;
    }
    let gone_ios =
        store_list_has(app, "removed_stores", 2) || store_list_has(app, "removed_stores", 3);
    let live_ios = store_list_has(app, "store", 2) || store_list_has(app, "store", 3);
    gone_ios && !live_ios
}

fn prefer_ios_icon(app: &JsonValue) -> String {
    let icon = json_string(app, "icon_url").unwrap_or_default();
    if icon.contains("mzstatic.com") {
                                                                          
                                                            
        return icon;
    }
    icon
}

                                                                              
                                                                                 
async fn ios_app_info_icon(adam: &str) -> Option<String> {
    let client = http_client().ok()?;
    for country in ["US", "RU"] {
        let Ok(response) = client
            .post("https://appmagic.rocks/api/v2/applications/app-info")
            .header("accept", "application/json, text/plain, */*")
            .header("content-type", "application/json")
            .header("origin", "https://appmagic.rocks")
            .header("referer", "https://appmagic.rocks/")
            .json(&serde_json::json!({
                "store": 2,
                "storeApplicationID": adam,
                "country": country,
            }))
            .send()
            .await
        else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(value) = response.json::<JsonValue>().await else {
            continue;
        };
        if let Some(icon) = value
            .get("data")
            .and_then(|data| data.get("icon_url"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            return Some(icon.to_string());
        }
    }
    None
}

async fn fill_ios_icons(apps: &mut [StoreSearchApp]) {
    use futures_util::{stream, StreamExt};

    let jobs: Vec<(usize, String)> = apps
        .iter()
        .enumerate()
        .filter(|(_, app)| !app.found_on_account && !app.icon_url.contains("mzstatic.com"))
        .take(ICON_LOOKUP_LIMIT)
        .map(|(index, app)| (index, app.app_store_id.clone()))
        .collect();
    if jobs.is_empty() {
        return;
    }
    let results = stream::iter(jobs.into_iter().map(|(index, adam)| async move {
        let artwork = tokio::time::timeout(
            Duration::from_secs(ICON_LOOKUP_TIMEOUT_SECS),
            ios_app_info_icon(&adam),
        )
        .await
        .ok()
        .flatten();
        (index, artwork)
    }))
    .buffer_unordered(ICON_LOOKUP_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;
    for (index, artwork) in results {
        if let Some(url) = artwork {
            apps[index].icon_url = url;
        }
    }
}

fn search_query_variants(query: &str) -> Vec<String> {
    let mut variants = vec![query.to_string()];
    if query.chars().any(is_cyrillic) {
        let latin: String = query
            .chars()
            .map(|ch| match ch.to_lowercase().next().unwrap_or(ch) {
                'а' => "a".to_string(),
                'б' => "b".to_string(),
                'в' => "v".to_string(),
                'г' => "g".to_string(),
                'д' => "d".to_string(),
                'е' | 'ё' => "e".to_string(),
                'ж' => "zh".to_string(),
                'з' => "z".to_string(),
                'и' => "i".to_string(),
                'й' => "y".to_string(),
                'к' => "k".to_string(),
                'л' => "l".to_string(),
                'м' => "m".to_string(),
                'н' => "n".to_string(),
                'о' => "o".to_string(),
                'п' => "p".to_string(),
                'р' => "r".to_string(),
                'с' => "s".to_string(),
                'т' => "t".to_string(),
                'у' => "u".to_string(),
                'ф' => "f".to_string(),
                'х' => "h".to_string(),
                'ц' => "ts".to_string(),
                'ч' => "ch".to_string(),
                'ш' => "sh".to_string(),
                'щ' => "sch".to_string(),
                'ъ' | 'ь' => String::new(),
                'ы' => "y".to_string(),
                'э' => "e".to_string(),
                'ю' => "yu".to_string(),
                'я' => "ya".to_string(),
                other => other.to_string(),
            })
            .collect();
        let compact: String = latin
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect();
        if !compact.is_empty() && compact != query {
            variants.push(compact);
        }
    }
    variants
}

fn is_cyrillic(ch: char) -> bool {
    matches!(ch, 'А'..='Я' | 'а'..='я' | 'Ё' | 'ё')
}

                                                                        
                                                                  
// ─── Поиск по разработчику ────────────────────────────────────────────────
//
// Здесь берём iTunes, а не AppMagic: у Apple это штатная возможность —
// `attribute=softwareDeveloper` ищет по имени разработчика, а `lookup` по
// `artistId` отдаёт все его приложения разом.
//
// Важное ограничение, о котором говорим человеку прямо в окне: iTunes знает
// только то, что сейчас лежит в магазине. Снятые с продажи приложения того же
// разработчика сюда не попадут — их ищут по названию или по идентификатору.

const ITUNES_SEARCH: &str = "https://itunes.apple.com/search";
const ITUNES_LOOKUP: &str = "https://itunes.apple.com/lookup";
const DEVELOPER_LIMIT: usize = 20;
const DEVELOPER_APPS_LIMIT: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeveloperAppsInput {
    artist_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreDeveloper {
    artist_id: String,
    name: String,
    /// Сколько приложений нашлось — показываем, чтобы было видно масштаб.
    app_count: usize,
    /// Несколько значков для предпросмотра прямо в строке результата.
    icons: Vec<String>,
}

#[tauri::command]
pub async fn search_store_developers(
    input: SearchStoreAppsInput,
) -> Result<Vec<StoreDeveloper>, String> {
    let Some(query) = normalize_store_search_query(&input.query) else {
        return Ok(Vec::new());
    };
    // Две витрины: в российской нет части приложений, в американской — другой.
    // Объединение даёт заметно более полный список разработчиков.
    let country_results = futures_util::future::join_all(
        ["ru", "us"]
            .into_iter()
            .map(|country| itunes_developers(&query, country)),
    )
    .await;
    let mut found: Vec<StoreDeveloper> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for developers in country_results {
        for developer in developers {
            if seen.insert(developer.artist_id.clone()) {
                found.push(developer);
            }
        }
    }
    // Сначала те, у кого приложений больше: обычно это и есть искомая студия.
    found.sort_by(|a, b| b.app_count.cmp(&a.app_count));
    found.truncate(DEVELOPER_LIMIT);
    Ok(found)
}

#[tauri::command]
pub async fn developer_apps(input: DeveloperAppsInput) -> Result<Vec<StoreSearchApp>, String> {
    let artist = input.artist_id.trim();
    if artist.is_empty() || !artist.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("Не указан разработчик.".into());
    }
    let country_results = futures_util::future::join_all(
        ["ru", "us"]
            .into_iter()
            .map(|country| itunes_artist_apps(artist, country)),
    )
    .await;
    let mut apps: Vec<StoreSearchApp> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for country_apps in country_results {
        for app in country_apps {
            if seen.insert(app.app_store_id.clone()) {
                apps.push(app);
            }
        }
    }
    if apps.is_empty() {
        return Err("У этого разработчика не нашлось приложений для iPhone.".into());
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(apps)
}

/// Разработчики, чьё имя похоже на запрос.
async fn itunes_developers(query: &str, country: &str) -> Vec<StoreDeveloper> {
    let Ok(client) = http_client() else {
        return Vec::new();
    };
    let Ok(response) = client
        .get(ITUNES_SEARCH)
        .query(&[
            ("term", query),
            ("entity", "software"),
            ("attribute", "softwareDeveloper"),
            ("country", country),
            ("limit", "60"),
        ])
        .send()
        .await
    else {
        return Vec::new();
    };
    if !response.status().is_success() {
        return Vec::new();
    }
    let Ok(value) = response.json::<JsonValue>().await else {
        return Vec::new();
    };
    group_by_developer(&value)
}

/// Apple отдаёт плоский список приложений — собираем их по разработчикам.
fn group_by_developer(value: &JsonValue) -> Vec<StoreDeveloper> {
    let Some(results) = value.get("results").and_then(|item| item.as_array()) else {
        return Vec::new();
    };
    let mut order: Vec<String> = Vec::new();
    let mut by_id: std::collections::HashMap<String, StoreDeveloper> =
        std::collections::HashMap::new();
    for item in results {
        let Some(artist_id) = numeric_field(item, "artistId") else {
            continue;
        };
        let Some(name) = json_string(item, "artistName") else {
            continue;
        };
        let entry = by_id.entry(artist_id.clone()).or_insert_with(|| {
            order.push(artist_id.clone());
            StoreDeveloper {
                artist_id: artist_id.clone(),
                name,
                app_count: 0,
                icons: Vec::new(),
            }
        });
        entry.app_count += 1;
        if entry.icons.len() < 3 {
            if let Some(icon) = itunes_icon(item) {
                entry.icons.push(icon);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect()
}

/// Все приложения разработчика в одной витрине.
async fn itunes_artist_apps(artist_id: &str, country: &str) -> Vec<StoreSearchApp> {
    let Ok(client) = http_client() else {
        return Vec::new();
    };
    let Ok(response) = client
        .get(ITUNES_LOOKUP)
        .query(&[
            ("id", artist_id),
            ("entity", "software"),
            ("country", country),
            ("limit", &DEVELOPER_APPS_LIMIT.to_string()),
        ])
        .send()
        .await
    else {
        return Vec::new();
    };
    if !response.status().is_success() {
        return Vec::new();
    }
    let Ok(value) = response.json::<JsonValue>().await else {
        return Vec::new();
    };
    parse_itunes_apps(&value)
}

fn parse_itunes_apps(value: &JsonValue) -> Vec<StoreSearchApp> {
    let Some(results) = value.get("results").and_then(|item| item.as_array()) else {
        return Vec::new();
    };
    results
        .iter()
        // Первым элементом идёт сам разработчик, а не приложение: у него нет
        // trackId. Заодно отсекаем приложения не для iPhone.
        .filter(|item| json_string(item, "kind").as_deref() == Some("software"))
        .filter_map(|item| {
            Some(StoreSearchApp {
                name: json_string(item, "trackName")?,
                publisher: json_string(item, "artistName").unwrap_or_default(),
                icon_url: itunes_icon(item).unwrap_or_default(),
                app_store_id: numeric_field(item, "trackId")?,
                // Раз Apple его отдала — он в магазине есть.
                removed_from_store: false,
                found_on_account: false,
            })
        })
        .collect()
}

fn itunes_icon(item: &JsonValue) -> Option<String> {
    for key in ["artworkUrl512", "artworkUrl100", "artworkUrl60"] {
        if let Some(url) = json_string(item, key) {
            return Some(url);
        }
    }
    None
}

/// Числовое поле, которое Apple отдаёт то числом, то строкой.
fn numeric_field(item: &JsonValue, key: &str) -> Option<String> {
    let node = item.get(key)?;
    if let Some(number) = node.as_i64() {
        return Some(number.to_string());
    }
    node.as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

fn first_names_array(app: &JsonValue) -> Option<String> {
    app.get("names_array")
        .and_then(|item| item.as_array())
        .and_then(|list| list.first())
        .and_then(|item| item.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn json_string(node: &JsonValue, key: &str) -> Option<String> {
    node.get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn is_adam_id(value: &str) -> bool {
    let digits = value.trim();
    (8..=12).contains(&digits.len()) && digits.chars().all(|ch| ch.is_ascii_digit())
}

                                                                                  
pub fn normalize_store_search_query(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(id) = extract_apple_id(trimmed) {
        return Some(id);
    }
    let compact: String = trimmed.chars().filter(|ch| !ch.is_whitespace()).collect();
    if is_adam_id(&compact) {
        return Some(compact);
    }
    if trimmed.chars().count() < 2 {
        return None;
    }
    Some(trimmed.to_string())
}

fn extract_apple_id(raw: &str) -> Option<String> {
    static URL_ID: OnceLock<Regex> = OnceLock::new();
    static QUERY_ID: OnceLock<Regex> = OnceLock::new();
    static TOKEN_ID: OnceLock<Regex> = OnceLock::new();
    let url_id = URL_ID.get_or_init(|| {
        Regex::new(r"(?i)(?:apps\.apple\.com|itunes\.apple\.com|itms-apps:)[^\s]*[/]id(\d{8,12})")
            .expect("url id regex")
    });
    if let Some(cap) = url_id.captures(raw) {
        return Some(cap[1].to_string());
    }
    let query_id =
        QUERY_ID.get_or_init(|| Regex::new(r"(?i)(?:\?|&)id=(\d{8,12})").expect("query id regex"));
    if let Some(cap) = query_id.captures(raw) {
        return Some(cap[1].to_string());
    }
    let token_id = TOKEN_ID.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^a-z0-9])id\s*(\d{8,12})(?:[^0-9]|$)").expect("token id regex")
    });
    token_id.captures(raw).map(|cap| cap[1].to_string())
}

#[cfg(test)]
mod tests {
    use super::{group_by_developer, normalize_store_search_query, parse_itunes_apps,
                parse_search_apps, search_query_variants};
    use serde_json::json;

    #[test]
    fn account_lookup_only_checks_missing_exact_app_ids() {
        let apps = parse_itunes_apps(&json!({"results": [{
            "kind": "software", "trackId": 389801252, "trackName": "Demo"
        }]}));
        assert!(!super::needs_account_lookup("389801252", &apps));
        assert!(super::needs_account_lookup("1514207975", &apps));
        assert!(super::needs_account_lookup("1514207975", &[]));
        assert!(!super::needs_account_lookup("Demo", &[]));
        assert!(!super::needs_account_lookup("151", &[]));
        assert!(!apps[0].found_on_account);
        let id = normalize_store_search_query("https://apps.apple.com/ru/app/id1514207975").unwrap();
        assert!(super::needs_account_lookup(&id, &[]));
    }

    #[test]
    fn developers_are_grouped_and_counted() {
        // Apple отдаёт плоский список приложений — разработчик повторяется в
        // каждой строке. Складываем их в одного и считаем приложения.
        let payload = json!({
            "results": [
                { "artistId": 284882218, "artistName": "Meta", "trackId": 389801252,
                  "kind": "software", "artworkUrl512": "https://is1.mzstatic.com/a.jpg" },
                { "artistId": 284882218, "artistName": "Meta", "trackId": 454638411,
                  "kind": "software", "artworkUrl512": "https://is1.mzstatic.com/b.jpg" },
                { "artistId": 1234567890, "artistName": "Другая студия", "trackId": 111111111,
                  "kind": "software" }
            ]
        });
        let devs = group_by_developer(&payload);
        assert_eq!(devs.len(), 2);
        assert_eq!(devs[0].artist_id, "284882218");
        assert_eq!(devs[0].app_count, 2);
        assert_eq!(devs[0].icons.len(), 2);
    }

    #[test]
    fn the_artist_row_is_not_taken_for_an_app() {
        // В ответе lookup первая строка — сам разработчик: у неё kind=artist и
        // нет trackId. Без этой проверки он попадал бы в список приложений.
        let payload = json!({
            "results": [
                { "artistId": 284882218, "artistName": "Meta", "kind": "artist" },
                { "artistId": 284882218, "artistName": "Meta", "trackId": 389801252,
                  "trackName": "Instagram", "kind": "software",
                  "artworkUrl100": "https://is1.mzstatic.com/i.jpg" }
            ]
        });
        let apps = parse_itunes_apps(&payload);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].app_store_id, "389801252");
        assert_eq!(apps[0].name, "Instagram");
        assert!(!apps[0].removed_from_store);
    }

    #[test]
    fn apple_numbers_arrive_as_numbers_or_strings() {
        let payload = json!({
            "results": [
                { "artistId": "284882218", "artistName": "Meta", "trackId": "389801252",
                  "trackName": "Instagram", "kind": "software" }
            ]
        });
        assert_eq!(parse_itunes_apps(&payload)[0].app_store_id, "389801252");
    }

    #[test]
    fn parse_name() {
        assert_eq!(
            normalize_store_search_query("  rutube  ").as_deref(),
            Some("rutube")
        );
    }

    #[test]
    fn parse_digits() {
        assert_eq!(
            normalize_store_search_query("389801252").as_deref(),
            Some("389801252")
        );
    }

    #[test]
    fn parse_id_prefix() {
        assert_eq!(
            normalize_store_search_query("id6763012218").as_deref(),
            Some("6763012218")
        );
        assert_eq!(
            normalize_store_search_query("id 389801252").as_deref(),
            Some("389801252")
        );
    }

    #[test]
    fn parse_app_store_url() {
        assert_eq!(
            normalize_store_search_query("https://apps.apple.com/ru/app/instagram/id389801252")
                .as_deref(),
            Some("389801252")
        );
        assert_eq!(
            normalize_store_search_query("https://apps.apple.com/ru/app/id6763012218").as_deref(),
            Some("6763012218")
        );
    }

    #[test]
    fn keeps_app_store_skips_play_and_marks_removed() {
        let payload = json!({
            "applications": [
                {
                    "name": "Play only",
                    "icon_url": "https://play-lh.googleusercontent.com/x",
                    "unitedPublisher": { "name": "Foo" },
                    "store_ids": ["1_com.foo.app"],
                    "store": [1],
                    "removed": false
                },
                {
                    "name": "Gone iPhone app",
                    "icon_url": "https://is1-ssl.mzstatic.com/image/thumb/x/0x514h.jpg",
                    "unitedPublisher": { "name": "Bar" },
                    "store_ids": ["2_1558481561"],
                    "store": [2],
                    "removed": true,
                    "removed_stores": [2]
                }
            ]
        });
        let apps = parse_search_apps(&payload, "test");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].app_store_id, "1558481561");
        assert!(apps[0].removed_from_store);
        assert!(apps[0].icon_url.contains("mzstatic.com"));
    }

    #[test]
    fn search_parser_keeps_the_full_api_page() {
        let applications = (0..105)
            .map(|n| {
                json!({
                    "name": format!("App {n}"),
                    "store_ids": [format!("2_{}", 1000000000 + n)],
                    "store": [2]
                })
            })
            .collect::<Vec<_>>();
        let apps = parse_search_apps(&json!({ "applications": applications }), "app");
        assert_eq!(apps.len(), 100);
    }

    #[test]
    fn cyrillic_search_gets_a_latin_fallback() {
        assert_eq!(
            search_query_variants("Т-Банк"),
            vec!["Т-Банк", "tbank"]
        );
        assert_eq!(search_query_variants("tbank"), vec!["tbank"]);
        assert_eq!(search_query_variants("Ёлка"), vec!["Ёлка", "elka"]);
    }

    #[test]
    fn itunes_result_shape_is_compatible_with_store_search() {
        let payload = json!({"results": [{
            "kind": "software", "trackId": 1234567890,
            "trackName": "Любовь и гуси", "artistName": "РСХБ",
            "artworkUrl512": "https://is1.mzstatic.com/icon.png"
        }]});
        let apps = parse_itunes_apps(&payload);
        assert_eq!(apps[0].app_store_id, "1234567890");
        assert_eq!(apps[0].name, "Любовь и гуси");
        assert!(!apps[0].icon_url.is_empty());
    }
}
