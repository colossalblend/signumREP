package pw.appbank.signum.data.api

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.Request
import pw.appbank.signum.data.catalog.CatalogResolve
import pw.appbank.signum.data.models.APP_BUILD
import pw.appbank.signum.data.models.APP_VERSION
import pw.appbank.signum.data.models.TestFlightOffer
import pw.appbank.signum.data.models.AppActivation
import pw.appbank.signum.data.models.CatalogApp
import pw.appbank.signum.data.models.CatalogAppDetail
import pw.appbank.signum.data.models.CatalogBanner
import pw.appbank.signum.data.models.CatalogFeed
import pw.appbank.signum.data.models.CatalogGroup
import pw.appbank.signum.data.models.CatalogVersion
import pw.appbank.signum.data.models.DEFAULT_API
import pw.appbank.signum.data.models.OtaUpdateInfo

class SignumApiException(message: String) : Exception(message)

class SignumApi {
    fun fetchCatalogFeed(): CatalogFeed {
        val root = get("main.php")
        val list = root["apps"] as? JsonArray
            ?: root["data"] as? JsonArray
            ?: throw SignumApiException("Не удалось загрузить каталог")
        val bust = System.currentTimeMillis().toString()
        val apps = list.mapNotNull { el ->
            val obj = el as? JsonObject ?: return@mapNotNull null
            parseCatalogApp(obj, bust)
        }
        return CatalogFeed(
            apps = apps,
            groups = parseGroups(root),
            banners = parseBanners(root),
        )
    }

    fun fetchAppInfo(id: String): JsonObject {
        val root = get("info.php", mapOf("id" to id))
        return (root["data"] as? JsonObject) ?: root
    }

    fun fetchCatalogAppDetail(id: String): CatalogAppDetail {
        val root = fetchAppInfo(id)
        val bust = System.currentTimeMillis().toString()
        val versions = root.arr("versions")?.mapNotNull { el ->
            parseCatalogVersion(el as? JsonObject ?: return@mapNotNull null, bust)
        }.orEmpty()
        return CatalogAppDetail(
            id = root.anyStr("id") ?: id,
            title = root.anyStr("title", "name") ?: id,
            iconUrl = root.anyStr("icon_url")?.let { cacheBust(it, bust) },
            inStore = root.anyBool("in_store", false),
            versions = versions,
            aliases = root.strList("aliases"),
        )
    }

    fun checkOta(): OtaUpdateInfo {
        val root = runCatching { get("ota.php") }.getOrNull() ?: return OtaUpdateInfo()
        val latest = root.str("version") ?: root.str("latest_version")
        val build = root.str("build")
        val url = root.str("download_url") ?: root.str("url")
        val notes = root.str("notes") ?: root.str("message")
        val has = latest != null && latest != APP_VERSION
        return OtaUpdateInfo(
            hasUpdate = has || root.bool("has_update"),
            version = latest,
            build = build,
            downloadUrl = url,
            notes = notes,
            mandatory = root.bool("mandatory"),
            sha256 = root.str("sha256"),
        )
    }

    fun fetchTb(id: String): JsonObject = get("tb.php", mapOf("id" to id))

    private fun parseCatalogApp(obj: JsonObject, bust: String): CatalogApp? {
        val id = obj.anyStr("id") ?: return null
        if (!obj.anyBool("active", true) && !obj.anyBool("is_active", true)) return null
        val title = obj.anyStr("title", "name") ?: return null
        return CatalogApp(
            id = id,
            title = title,
            icon_url = obj.anyStr("icon_url")?.let { cacheBust(it, bust) },
            badge = obj.anyStr("badge"),
            version = obj.anyStr("version"),
            min_ios = obj.anyStr("min_ios"),
            active = obj.anyBool("active", true),
            in_store = obj.anyBool("in_store", false),
            color = obj.anyStr("color"),
            aliases = obj.strList("aliases").ifEmpty { null },
            category = obj.anyStr("category"),
            activation = decodeActivation(obj.obj("activation")),
            bundle_id = obj.anyStr("bundle_id"),
            app_store_id = obj.anyStr("app_store_id", "store_id", "adam_id"),
            testflight = obj.obj("testflight")
                ?.anyStr("url")
                ?.trim()
                ?.takeIf { it.startsWith("https://testflight.apple.com/") }
                ?.let { TestFlightOffer(it) },
        )
    }

    private fun parseCatalogVersion(obj: JsonObject, bust: String): CatalogVersion? {
        val storeId = obj.anyStr("app_store_id", "store_id", "adam_id")
        val bundle = obj.anyStr("bundle_id")
        if ((bundle.isNullOrBlank() || bundle == "NONE") && storeId.isNullOrBlank()) return null
        return CatalogVersion(
            title = obj.anyStr("title", "label").orEmpty(),
            version = obj.anyStr("version").orEmpty(),
            bundleId = bundle,
            appStoreId = storeId,
            minIos = obj.anyStr("min_ios"),
            iconUrl = obj.anyStr("icon_url")?.let { cacheBust(it, bust) },
            inStore = obj.anyBool("in_store", false),
            isLatest = obj.anyBool("is_latest", false),
            description = obj.anyStr("description"),
        )
    }

    private fun parseGroups(root: JsonObject): List<CatalogGroup> {
        val arr = root.arr("groups")
        val parsed = arr?.mapNotNull { el ->
            val o = el as? JsonObject ?: return@mapNotNull null
            val id = o.anyStr("id") ?: return@mapNotNull null
            CatalogGroup(id = id, title = o.anyStr("title") ?: id, priority = o.int("priority"))
        }.orEmpty()
        if (parsed.isNotEmpty()) return parsed.sortedByDescending { it.priority ?: 0 }
        return CatalogResolve.DEFAULT_GROUPS
    }

    private fun parseBanners(root: JsonObject): List<CatalogBanner> {
        val arr = root.arr("banners") ?: return emptyList()
        return arr.mapNotNull { el ->
            val o = el as? JsonObject ?: return@mapNotNull null
            if (o["active"] != null && !o.bool("active", true)) return@mapNotNull null
            CatalogBanner(
                id = o.str("id").orEmpty(),
                title = o.str("title").orEmpty(),
                text = o.str("text"),
                style = o.str("style"),
                app_id = o.str("app_id"),
                action_label = o.str("action_label"),
                priority = o.int("priority"),
                active = o.bool("active", true),
            )
        }
    }

    private fun cacheBust(url: String, bust: String): String {
        if (url.contains("signum_cb=")) return url
        val sep = if (url.contains('?')) '&' else '?'
        return "$url${sep}signum_cb=$bust"
    }

    private fun get(endpoint: String, extra: Map<String, String> = emptyMap()): JsonObject {
        val builder = DEFAULT_API.trimEnd('/').toHttpUrl()
            .newBuilder()
            .addPathSegment(endpoint.trimStart('/'))
            .addQueryParameter("vers", APP_VERSION)
            .addQueryParameter("build", APP_BUILD)
            .addQueryParameter("platform", "android")
            .addQueryParameter("devkey", "")
        extra.forEach { (k, v) -> builder.addQueryParameter(k, v) }
        val url = builder.build()
        if (url.host != Hx.apiHost()) {
            throw SignumApiException("Не удалось подключиться к каталогу")
        }
        val req = Request.Builder().url(url).get().header("Accept", "application/json").build()
        Http.api.newCall(req).execute().use { resp ->
            val body = resp.body?.string().orEmpty()
            val root = runCatching { SignumJson.parseToJsonElement(body).jsonObject }
                .getOrElse { throw SignumApiException("Сервис вернул некорректные данные") }
            if (!resp.isSuccessful) {
                val err = root.str("error").orEmpty()
                if (resp.code == 426 || err == "client_outdated") {
                    throw SignumApiException(
                        root.str("message") ?: "Приложение недоступно. Обновите Signum.",
                    )
                }
                throw SignumApiException(
                    root.str("message")
                        ?: root.str("error")
                        ?: "Не удалось получить данные (${resp.code})",
                )
            }
            return root
        }
    }

    companion object {
        fun decodeActivation(obj: JsonObject?): AppActivation? {
            if (obj == null) return null
            return runCatching { SignumJson.decodeFromJsonElement<AppActivation>(obj) }.getOrNull()
        }
    }
}
