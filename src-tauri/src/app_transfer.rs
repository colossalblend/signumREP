use crate::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferItem {
    pub name: String,
    #[serde(default)]
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub bundle_id: Option<String>,
    #[serde(default)]
    pub app_store_id: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct TransferPreviewItem {
    pub item: TransferItem,
    pub status: String,
    pub message: String,
    pub min_ios: Option<String>,
}

#[derive(Serialize)]
pub struct TransferPreview {
    pub target_device: String,
    pub target_udid: String,
    pub items: Vec<TransferPreviewItem>,
}

fn normalize_items(items: Vec<TransferItem>) -> Result<Vec<TransferItem>, String> {
    if items.is_empty() || items.len() > 500 { return Err("Выберите от 1 до 500 приложений.".into()); }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for mut item in items {
        item.name = item.name.trim().to_string();
        if item.name.is_empty() || item.name.len() > 512 { return Err("Укажите название приложения.".into()); }
        for value in [&mut item.catalog_id, &mut item.bundle_id, &mut item.app_store_id] {
            *value = value.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        }
        if let Some(id) = &item.catalog_id {
            if id.len() > 128 || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
                return Err("Недопустимый идентификатор каталога.".into());
            }
        }
        if let Some(bundle) = &item.bundle_id {
            if bundle.len() > 255 || !bundle.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
                return Err("Недопустимый Bundle ID.".into());
            }
        }
        if item.app_store_id.as_deref().is_some_and(|id| normalized_app_store_id(Some(id)).is_none()) {
            return Err("Недопустимый App Store ID.".into());
        }
        let key = item.catalog_id.as_ref().map(|id| format!("catalog:{id}"))
            .or_else(|| item.bundle_id.as_ref().map(|id| format!("bundle:{id}")))
            .or_else(|| item.app_store_id.as_ref().map(|id| format!("store:{id}")))
            .ok_or("У приложения нет идентификатора.")?;
        if seen.insert(key) { out.push(item); }
    }
    Ok(out)
}

async fn store_metadata(item: &TransferItem) -> Option<JsonValue> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().ok()?;
    let key = if item.app_store_id.is_some() { "id" } else { "bundleId" };
    let id = item.app_store_id.as_deref().or(item.bundle_id.as_deref())?;
    for country in ["ru", "us", "kz"] {
        let response = client.get("https://itunes.apple.com/lookup")
            .query(&[(key, id), ("country", country), ("entity", "software")]).send().await;
        let Ok(response) = response else { continue };
        let Ok(value) = response.json::<JsonValue>().await else { continue };
        let Some(found) = value.get("results").and_then(JsonValue::as_array).and_then(|items| items.first()) else { continue };
        if let Some(bundle) = item.bundle_id.as_deref() {
            if found.get("bundleId").and_then(JsonValue::as_str) != Some(bundle) { continue; }
        }
        return Some(found.clone());
    }
    None
}

fn match_catalog_item(item: &TransferItem, details: &HashMap<String, (JsonValue, Vec<JsonValue>)>) -> Option<TransferItem> {
    for (id, (info, versions)) in details {
        if versions.iter().any(|version| item.bundle_id.as_deref().is_some_and(|bundle| version.get("bundle_id").and_then(JsonValue::as_str) == Some(bundle))) {
            return Some(TransferItem { catalog_id: Some(id.clone()), name: info.get("title").and_then(JsonValue::as_str).unwrap_or(&item.name).into(), ..item.clone() });
        }
    }
    None
}

#[tauri::command]
pub async fn preview_app_transfer(app: AppHandle, source_udid: String, target_udid: String) -> Result<TransferPreview, String> {
    if source_udid == target_udid { return Err("Выберите два разных iPhone.".into()); }
    let devices = list_connected_devices_inner(&app).await?;
    if !devices.iter().any(|device| device.udid == source_udid) { return Err("Старый iPhone отключён. Подключите его и повторите попытку.".into()); }
    let device = devices.into_iter().find(|device| device.udid == target_udid).ok_or("Новый iPhone отключён.")?;
    let items: Vec<TransferItem> = list_apps_for_device(&app, Some(&source_udid)).await?.into_iter().filter(|item| !item.is_placeholder).map(|item| TransferItem {
        name: item.name.unwrap_or_else(|| item.bundle_id.clone()), catalog_id: None,
        bundle_id: Some(item.bundle_id), app_store_id: item.app_store_id, version: item.version,
    }).collect();
    let installed = list_apps_for_device(&app, Some(&target_udid)).await?.into_iter()
        .filter(|item| !item.is_placeholder)
        .map(|item| (item.bundle_id.clone(), item)).collect::<HashMap<_, _>>();
    let needs_catalog = !items.is_empty();
    let details = if needs_catalog {
        let feed = fetch_catalog(app.clone()).await.unwrap_or_default();
        let ids = feed.iter().filter_map(|item| item.get("id").and_then(JsonValue::as_str)).map(str::to_string).collect::<Vec<_>>();
        fetch_catalog_versions_batch(&app, &ids).await.unwrap_or_default()
    } else { HashMap::new() };
    let mut rows = Vec::new();
    for original in items {
        let mut item = if original.catalog_id.is_none() { match_catalog_item(&original, &details).unwrap_or(original) } else { original };
        let mut min_ios = None;
        let mut message = "Будет выбрана доступная версия; владение проверяется перед постановкой в очередь.".to_string();
        let mut status = "ready";
        if item.bundle_id.as_deref().is_some_and(|bundle| installed.contains_key(bundle)) {
            status = "installed";
            message = "Уже установлено на выбранном iPhone.".into();
        } else if let Some(catalog_id) = item.catalog_id.as_deref() {
            if let Some((_, versions)) = details.get(catalog_id) {
                if versions.iter().any(|version| version.get("bundle_id").and_then(JsonValue::as_str).is_some_and(|bundle| installed.contains_key(bundle))) {
                    status = "installed";
                    message = "Одна из версий уже установлена на выбранном iPhone.".into();
                }
                min_ios = best_compatible_store_version(versions, device.product_version.as_deref())
                    .or_else(|| versions.first()).and_then(|version| version.get("min_ios").and_then(JsonValue::as_str)).map(str::to_string);
            } else { status = "unavailable"; message = "Приложение отсутствует в текущем каталоге.".into(); }
        } else if let Some(found) = store_metadata(&item).await {
            item.app_store_id = found.get("trackId").and_then(JsonValue::as_u64).map(|id| id.to_string());
            min_ios = found.get("minimumOsVersion").and_then(JsonValue::as_str).map(str::to_string);
            message = "Найдено в App Store. Будет установлена текущая доступная версия.".into();
        } else if item.app_store_id.is_none() {
            status = "unavailable";
            message = "Не найдено в каталоге и App Store; без App Store ID автоматическая установка невозможна.".into();
        } else { message = "Снято с продажи или недоступно в регионе. Проверим наличие в покупках Apple ID.".into(); }
        if status == "ready" && device_supports_min_ios(device.product_version.as_deref(), min_ios.as_deref()) == Some(false) {
            status = "incompatible";
            message = format!("Требуется iOS {}.", min_ios.as_deref().unwrap_or("новее"));
        }
        rows.push(TransferPreviewItem { item, status: status.into(), message, min_ios });
    }
    Ok(TransferPreview { target_udid: device.udid, target_device: device.device_name.unwrap_or_else(|| "iPhone".into()), items: rows })
}

#[derive(Deserialize)]
pub struct EnqueueTransferInput { pub items: Vec<TransferItem>, pub target_udid: String }

#[tauri::command]
pub async fn enqueue_app_transfer(app: AppHandle, input: EnqueueTransferInput) -> Result<CatalogInstallManyResult, String> {
    let items = normalize_items(input.items)?;
    require_auth(&app)?;
    let device = select_device(app.clone(), SelectDeviceInput { udid: input.target_udid.clone() }).await?;
    if device.udid != input.target_udid { return Err("Выбран другой iPhone. Обновите список перед установкой.".into()); }
    let installed = list_installed_apps_inner(&app).await?.into_iter()
        .filter(|item| !item.is_placeholder)
        .map(|item| (item.bundle_id.clone(), item)).collect::<HashMap<_, _>>();
    let mut result = CatalogInstallManyResult { job_ids: Vec::new(), skipped: Vec::new() };
    for item in items {
        let identity = item.catalog_id.clone().or_else(|| item.bundle_id.clone()).or_else(|| item.app_store_id.clone()).unwrap();
        let attempt = async {
            if item.bundle_id.as_deref().is_some_and(|bundle| installed.contains_key(bundle)) { return Err("Уже установлено на выбранном iPhone.".into()); }
            let payload = if let Some(id) = item.catalog_id.clone() {
                let offer = resolve_catalog_install(app.clone(), app.state::<AppState>(), ResolveCatalogInput { app_id: id, restore_only: false }).await?;
                if installed.contains_key(&offer.bundle_id) { return Err("Уже установлено на выбранном iPhone.".into()); }
                if offer.source != "restore" { return Err("Нет подтверждённой версии в покупках Apple ID.".into()); }
                if offer.device_compatible == Some(false) { return Err("Версия не подходит для iOS выбранного iPhone.".into()); }
                CatalogInstallInput { title: offer.title, bundle_id: offer.bundle_id, app_store_id: offer.app_store_id, min_ios: offer.min_ios,
                    version: Some(offer.version), in_store: offer.in_store, already_owned: true, external_version_id: None, temporary_us_region: false, temporary_region: None }
            } else {
                let found = store_metadata(&item).await;
                let mut payload = CatalogInstallInput { title: item.name.clone(), bundle_id: item.bundle_id.clone().unwrap_or_default(), app_store_id: item.app_store_id.clone(),
                    min_ios: found.as_ref().and_then(|value| value.get("minimumOsVersion").and_then(JsonValue::as_str)).map(str::to_string),
                    version: None, in_store: false, already_owned: false, external_version_id: None, temporary_us_region: false, temporary_region: None };
                if let Some(found) = found {
                    payload.app_store_id = found.get("trackId").and_then(JsonValue::as_u64).map(|id| id.to_string());
                    payload.bundle_id = found.get("bundleId").and_then(JsonValue::as_str).unwrap_or(&payload.bundle_id).into();
                }
                payload = normalize_catalog_install_input(payload)?;
                if device_supports_min_ios(device.product_version.as_deref(), payload.min_ios.as_deref()) == Some(false) { return Err("Приложение требует более новую iOS.".into()); }
                let state = app.state::<AppState>();
                let _guard = state.ipatool_lock.lock().await;
                ensure_active_ipatool_session(&app).await?;
                payload.already_owned = probe_version_on_account_for_catalog_version(&app, &payload.bundle_id, payload.app_store_id.as_deref(), &JsonValue::Null).await
                    .ok_or("Не удалось проверить покупки Apple ID. Повторите попытку.")?;
                if !payload.already_owned { return Err("Нет подтверждённой версии в покупках Apple ID. Добавьте приложение через поиск.".into()); }
                payload
            };
            if selected_device_udid(&app).as_deref() != Some(input.target_udid.as_str()) { return Err("Выбран другой iPhone. Проверьте перенос заново.".into()); }
            enqueue_catalog_install(app.clone(), app.state::<AppState>(), payload).await
        }.await;
        match attempt { Ok(id) => result.job_ids.push(id), Err(reason) => result.skipped.push(CatalogInstallSkipped { app_id: identity, reason: format!("{}: {reason}", item.name) }) }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn items_are_validated_deduplicated_and_matched_by_exact_bundle() {
        let item = TransferItem { name: " Банк ".into(), catalog_id: None, bundle_id: Some("com.bank.app".into()), app_store_id: None, version: Some("1.0".into()) };
        let normalized = normalize_items(vec![item.clone(), item.clone()]).unwrap();
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].name, "Банк");
        let details = HashMap::from([("bank".into(), (serde_json::json!({"title":"Банк"}), vec![serde_json::json!({"bundle_id":"com.bank.app"})]))]);
        assert_eq!(match_catalog_item(&item, &details).unwrap().catalog_id.as_deref(), Some("bank"));
        let mut other = item;
        other.bundle_id = Some("com.bank.app2".into());
        assert!(match_catalog_item(&other, &details).is_none());
        other.catalog_id = Some("../outside".into());
        assert!(normalize_items(vec![other]).is_err());
    }
}
