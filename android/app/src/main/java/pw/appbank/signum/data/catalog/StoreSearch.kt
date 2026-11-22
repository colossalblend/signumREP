package pw.appbank.signum.data.catalog

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import pw.appbank.signum.data.api.Http
import pw.appbank.signum.data.api.SignumJson
import pw.appbank.signum.data.models.AppStoreSearchHit
import pw.appbank.signum.data.models.CatalogApp
import pw.appbank.signum.data.models.StoreDeveloper

/**
 * Поиск по App Store — тот же порядок, что у ПК (`store_search.rs`).
 *
 * AppMagic быстрее для полнотекстового поиска (и знает снятые с продажи
 * приложения), iTunes — источник истины для ID и свежих приложений. Ответы
 * обоих сводятся в один список: одно приложение — одна строка, а у строки
 * AppMagic недостающие значок и разработчик берутся из iTunes.
 */
object StoreSearch {
    private const val APPMAGIC_SEARCH = "https://appmagic.rocks/api/v2/search"
    private const val APPMAGIC_INFO = "https://appmagic.rocks/api/v2/applications/app-info"
    private const val ITUNES_SEARCH = "https://itunes.apple.com/search"
    private const val ITUNES_LOOKUP = "https://itunes.apple.com/lookup"
    private const val RESULT_LIMIT = 100
    private const val DEVELOPER_LIMIT = 20
    private const val ICON_LOOKUP_LIMIT = 24
    private const val ICON_LOOKUP_CONCURRENCY = 8
    private const val ICON_LOOKUP_TIMEOUT_MS = 4_000L
    private const val USER_AGENT =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36"

    private val client: OkHttpClient get() = Http.itunes

    // ── Запрос ─────────────────────────────────────────────────────────────

    /** Номер приложения, ссылка App Store или текст; `null` — искать нечего. */
    fun normalizeQuery(raw: String): String? {
        val trimmed = raw.trim()
        if (trimmed.isEmpty()) return null
        extractAppleId(trimmed)?.let { return it }
        val compact = trimmed.filterNot(Char::isWhitespace)
        if (isAdamId(compact)) return compact
        if (trimmed.codePointCount(0, trimmed.length) < 2) return null
        return trimmed
    }

    fun isAdamId(value: String): Boolean {
        val digits = value.trim()
        return digits.length in 8..12 && digits.all { it in '0'..'9' }
    }

    private val urlId = Regex("""(?i)(?:apps\.apple\.com|itunes\.apple\.com|itms-apps:)\S*/id(\d{8,12})""")
    private val queryId = Regex("""(?i)(?:\?|&)id=(\d{8,12})""")
    private val tokenId = Regex("""(?i)(?:^|[^a-z0-9])id\s*(\d{8,12})(?:[^0-9]|$)""")

    fun extractAppleId(raw: String): String? =
        urlId.find(raw)?.groupValues?.get(1)
            ?: queryId.find(raw)?.groupValues?.get(1)
            ?: tokenId.find(raw)?.groupValues?.get(1)

    /** Кириллица дополнительно ищется латиницей: AppMagic знает «sber», а не «сбер». */
    fun queryVariants(query: String): List<String> {
        val variants = mutableListOf(query)
        if (query.any(::isCyrillic)) {
            val latin = buildString {
                query.forEach { ch -> append(translit(ch.lowercaseChar()) ?: ch.toString()) }
            }
            val compact = latin.filter { it.isLetterOrDigit() && it.code < 128 }
            if (compact.isNotEmpty() && compact != query) variants += compact
        }
        return variants
    }

    private fun isCyrillic(ch: Char) = ch in 'А'..'я' || ch == 'Ё' || ch == 'ё'

    private fun translit(ch: Char): String? = when (ch) {
        'а' -> "a"; 'б' -> "b"; 'в' -> "v"; 'г' -> "g"; 'д' -> "d"
        'е', 'ё' -> "e"; 'ж' -> "zh"; 'з' -> "z"; 'и' -> "i"; 'й' -> "y"
        'к' -> "k"; 'л' -> "l"; 'м' -> "m"; 'н' -> "n"; 'о' -> "o"
        'п' -> "p"; 'р' -> "r"; 'с' -> "s"; 'т' -> "t"; 'у' -> "u"
        'ф' -> "f"; 'х' -> "h"; 'ц' -> "ts"; 'ч' -> "ch"; 'ш' -> "sh"
        'щ' -> "sch"; 'ъ', 'ь' -> ""; 'ы' -> "y"; 'э' -> "e"; 'ю' -> "yu"
        'я' -> "ya"
        else -> null
    }

    // ── Поиск приложений ───────────────────────────────────────────────────

    /**
     * Приложения по запросу. [ownedOnAccount] спрашивает покупки Apple ID —
     * зовётся, только если введён номер, которого нет ни в одном магазине
     * (приложение снято с продажи, но куплено).
     */
    suspend fun searchApps(
        rawQuery: String,
        ownedOnAccount: (suspend (String) -> Boolean)? = null,
    ): List<AppStoreSearchHit> = coroutineScope {
        val query = normalizeQuery(rawQuery) ?: return@coroutineScope emptyList()
        val magic = queryVariants(query).map { variant ->
            async(Dispatchers.IO) { runCatching { appMagic(variant, query) } }
        }
        val itunes = async(Dispatchers.IO) { itunesSearch(query) }
        val magicResults = magic.awaitAll()
        val firstError = magicResults.firstNotNullOfOrNull { it.exceptionOrNull() }
            ?.takeIf { it !is CancellationException }
        var hits = mergeHits(magicResults.mapNotNull { it.getOrNull() }, itunes.await())
        if (isAdamId(query) && hits.none { it.adamId == query } && ownedOnAccount != null) {
            if (ownedOnAccount(query)) {
                hits = listOf(AppStoreSearchHit(adamId = query, name = "Приложение $query", foundOnAccount = true)) + hits
            }
        }
        if (hits.isEmpty() && firstError != null) {
            throw IllegalStateException("Не удалось выполнить поиск. Проверьте интернет и повторите.", firstError)
        }
        hits.take(RESULT_LIMIT)
    }

    /**
     * Сводит ответы AppMagic (по вариантам запроса) и iTunes в один список
     * без повторов по App Store ID. Порядок AppMagic сохраняется, iTunes
     * дописывает недостающие поля и добавляет то, чего AppMagic не знает.
     */
    fun mergeHits(magic: List<List<AppStoreSearchHit>>, itunes: List<AppStoreSearchHit>): List<AppStoreSearchHit> {
        val out = ArrayList<AppStoreSearchHit>()
        val index = HashMap<String, Int>()
        for (page in magic) {
            for (hit in page) {
                val key = hit.adamId.trim()
                if (key.isEmpty() || key in index) continue
                index[key] = out.size
                out += hit
            }
            if (out.size >= RESULT_LIMIT) break
        }
        for (hit in itunes) {
            val key = hit.adamId.trim()
            if (key.isEmpty()) continue
            val at = index[key]
            if (at == null) {
                index[key] = out.size
                out += hit
            } else {
                out[at] = enrich(out[at], hit)
            }
        }
        return out.take(RESULT_LIMIT)
    }

    /** Строка AppMagic + те же данные из iTunes: ничего не теряем, ничего не дублируем. */
    private fun enrich(base: AppStoreSearchHit, extra: AppStoreSearchHit): AppStoreSearchHit {
        val icon = when {
            base.artwork.isNullOrBlank() -> extra.artwork
            !base.artwork.contains("mzstatic.com") && !extra.artwork.isNullOrBlank() -> extra.artwork
            else -> base.artwork
        }
        return base.copy(
            artist = base.artist?.takeIf { it.isNotBlank() } ?: extra.artist,
            artistId = base.artistId ?: extra.artistId,
            artwork = icon,
            bundleId = base.bundleId ?: extra.bundleId,
            version = base.version ?: extra.version,
            minIos = base.minIos ?: extra.minIos,
            price = base.price ?: extra.price,
            // iTunes отдаёт только то, что сейчас в магазине.
            removedFromStore = false,
        )
    }

    private fun appMagic(variant: String, query: String): List<AppStoreSearchHit> {
        val url = APPMAGIC_SEARCH.toHttpUrl().newBuilder()
            .addQueryParameter("name", variant)
            .addQueryParameter("limit", "100")
            .build()
        val request = Request.Builder().url(url)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "ru,en;q=0.9")
            .header("Origin", "https://appmagic.rocks")
            .header("Referer", "https://appmagic.rocks/")
            .build()
        val body = client.newCall(request).execute().use { response ->
            check(response.isSuccessful) { "Не удалось выполнить поиск. Повторите попытку." }
            response.body?.string().orEmpty()
        }
        check(!body.trimStart().startsWith("<")) { "Не удалось выполнить поиск. Повторите попытку." }
        return parseAppMagic(body, query)
    }

    fun parseAppMagic(body: String, query: String): List<AppStoreSearchHit> {
        val root = runCatching { SignumJson.parseToJsonElement(body) as? JsonObject }.getOrNull()
            ?: error("Не удалось обработать результаты поиска.")
        val apps = root["applications"] as? JsonArray ?: return emptyList()
        val seen = HashSet<String>()
        val out = apps.mapNotNull { el ->
            val app = el as? JsonObject ?: return@mapNotNull null
            val adam = iosAdamId(app) ?: return@mapNotNull null
            val name = app.text("name")
                ?: (app["names_array"] as? JsonArray)?.firstOrNull()?.let { (it as? JsonPrimitive)?.content?.trim() }
                    ?.takeIf { it.isNotEmpty() }
                ?: return@mapNotNull null
            if (!seen.add(adam)) return@mapNotNull null
            AppStoreSearchHit(
                adamId = adam,
                name = name,
                artist = (app["unitedPublisher"] as? JsonObject)?.text("name") ?: app.text("publisher_name"),
                artwork = app.text("icon_url") ?: app.text("icon"),
                removedFromStore = removedFromAppStore(app),
            )
        }.take(RESULT_LIMIT)
        return if (query.all { it in '0'..'9' }) out.sortedBy { if (it.adamId == query) 0 else 1 } else out
    }

    private fun iosAdamId(app: JsonObject): String? {
        val ids = (app["store_ids"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.content } ?: return null
        for (prefix in listOf("2_", "3_")) {
            ids.firstOrNull { it.startsWith(prefix) && isAdamId(it.removePrefix(prefix)) }
                ?.let { return it.removePrefix(prefix) }
        }
        return null
    }

    private fun storeListHas(app: JsonObject, key: String, store: Long): Boolean =
        (app[key] as? JsonArray)?.any { (it as? JsonPrimitive)?.longOrNull == store } == true

    private fun removedFromAppStore(app: JsonObject): Boolean {
        if ((app["removed"] as? JsonPrimitive)?.booleanOrNull == true) return true
        val goneIos = storeListHas(app, "removed_stores", 2) || storeListHas(app, "removed_stores", 3)
        val liveIos = storeListHas(app, "store", 2) || storeListHas(app, "store", 3)
        return goneIos && !liveIos
    }

    private fun itunesSearch(query: String): List<AppStoreSearchHit> {
        val pages = if (query.all { it in '0'..'9' }) {
            listOf(itunesGet(ITUNES_LOOKUP, "id" to query, "entity" to "software"))
        } else {
            listOf("ru", "us").map { country ->
                itunesGet(ITUNES_SEARCH, "term" to query, "entity" to "software", "country" to country, "limit" to "50")
            }
        }
        return pages.flatten().distinctBy { it.adamId }
    }

    private fun itunesGet(base: String, vararg params: Pair<String, String>): List<AppStoreSearchHit> =
        runCatching {
            val url = base.toHttpUrl().newBuilder().apply {
                params.forEach { (k, v) -> addQueryParameter(k, v) }
            }.build()
            val request = Request.Builder().url(url).header("User-Agent", USER_AGENT).get().build()
            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) return@use emptyList()
                parseItunes(response.body?.string().orEmpty())
            }
        }.getOrElse { if (it is CancellationException) throw it else emptyList() }

    /** Ответ iTunes search/lookup. Первая строка lookup разработчика — сам разработчик, её пропускаем. */
    fun parseItunes(body: String): List<AppStoreSearchHit> {
        val root = runCatching { SignumJson.parseToJsonElement(body) as? JsonObject }.getOrNull() ?: return emptyList()
        val results = root["results"] as? JsonArray ?: return emptyList()
        return results.mapNotNull { el ->
            val o = el as? JsonObject ?: return@mapNotNull null
            if (o.text("kind") != "software") return@mapNotNull null
            AppStoreSearchHit(
                adamId = o.numeric("trackId") ?: return@mapNotNull null,
                name = o.text("trackName") ?: return@mapNotNull null,
                artist = o.text("artistName"),
                artistId = o.numeric("artistId"),
                artwork = itunesIcon(o),
                bundleId = o.text("bundleId"),
                version = o.text("version"),
                minIos = o.text("minimumOsVersion"),
                price = o.text("formattedPrice"),
            )
        }
    }

    private fun itunesIcon(o: JsonObject): String? =
        o.text("artworkUrl512") ?: o.text("artworkUrl100") ?: o.text("artworkUrl60")

    /**
     * Значки AppMagic бывают от Android-версии — подменяем их значком iOS.
     * Отдельный шаг: список показываем сразу, значки дорисовываются следом.
     */
    suspend fun fillIosIcons(hits: List<AppStoreSearchHit>): List<AppStoreSearchHit> = coroutineScope {
        val jobs = hits.withIndex()
            .filter { (_, hit) -> !hit.foundOnAccount && hit.artwork?.contains("mzstatic.com") != true }
            .take(ICON_LOOKUP_LIMIT)
        if (jobs.isEmpty()) return@coroutineScope hits
        val gate = Semaphore(ICON_LOOKUP_CONCURRENCY)
        val icons = jobs.map { (index, hit) ->
            async(Dispatchers.IO) {
                gate.withPermit {
                    index to withTimeoutOrNull(ICON_LOOKUP_TIMEOUT_MS) {
                        runInterruptible { appInfoIcon(hit.adamId) }
                    }
                }
            }
        }.awaitAll().filter { it.second != null }.toMap()
        if (icons.isEmpty()) hits
        else hits.mapIndexed { index, hit -> icons[index]?.let { hit.copy(artwork = it) } ?: hit }
    }

    private fun appInfoIcon(adam: String): String? {
        for (country in listOf("US", "RU")) {
            val payload = buildJsonObject {
                put("store", 2)
                put("storeApplicationID", adam)
                put("country", country)
            }.toString()
            val request = Request.Builder().url(APPMAGIC_INFO)
                .header("User-Agent", USER_AGENT)
                .header("Accept", "application/json, text/plain, */*")
                .header("Origin", "https://appmagic.rocks")
                .header("Referer", "https://appmagic.rocks/")
                .post(payload.toRequestBody("application/json".toMediaType()))
                .build()
            val icon = runCatching {
                client.newCall(request).execute().use { response ->
                    if (!response.isSuccessful) return@use null
                    val root = SignumJson.parseToJsonElement(response.body?.string().orEmpty()) as? JsonObject
                    (root?.get("data") as? JsonObject)?.text("icon_url")
                }
            }.getOrNull()
            if (!icon.isNullOrBlank()) return icon
        }
        return null
    }

    // ── Разработчики ───────────────────────────────────────────────────────

    /**
     * Разработчики, чьё имя похоже на запрос: iTunes `attribute=softwareDeveloper`
     * по двум витринам, без повторов, самые крупные первыми.
     */
    suspend fun searchDevelopers(rawQuery: String): List<StoreDeveloper> = coroutineScope {
        val query = normalizeQuery(rawQuery) ?: return@coroutineScope emptyList()
        if (isAdamId(query)) return@coroutineScope emptyList()
        val pages = listOf("ru", "us").map { country ->
            async(Dispatchers.IO) {
                runCatching {
                    val url = ITUNES_SEARCH.toHttpUrl().newBuilder()
                        .addQueryParameter("term", query)
                        .addQueryParameter("entity", "software")
                        .addQueryParameter("attribute", "softwareDeveloper")
                        .addQueryParameter("country", country)
                        .addQueryParameter("limit", "60")
                        .build()
                    val request = Request.Builder().url(url).header("User-Agent", USER_AGENT).get().build()
                    client.newCall(request).execute().use { response ->
                        if (!response.isSuccessful) emptyList()
                        else groupByDeveloper(response.body?.string().orEmpty())
                    }
                }.getOrElse { if (it is CancellationException) throw it else emptyList() }
            }
        }.awaitAll()
        val seen = HashSet<String>()
        pages.flatten()
            .filter { seen.add(it.artistId) }
            .sortedByDescending { it.appCount }
            .take(DEVELOPER_LIMIT)
    }

    /** Apple отдаёт плоский список приложений — собираем их по разработчикам. */
    fun groupByDeveloper(body: String): List<StoreDeveloper> {
        val root = runCatching { SignumJson.parseToJsonElement(body) as? JsonObject }.getOrNull() ?: return emptyList()
        val results = root["results"] as? JsonArray ?: return emptyList()
        val order = LinkedHashMap<String, StoreDeveloper>()
        for (el in results) {
            val o = el as? JsonObject ?: continue
            // Строка самого разработчика (без trackId) — не его приложение.
            if (o.text("kind") != "software") continue
            val id = o.numeric("artistId") ?: continue
            val name = o.text("artistName") ?: continue
            val current = order[id] ?: StoreDeveloper(id, name, 0, emptyList())
            val icons = if (current.icons.size < 3) itunesIcon(o)?.let { current.icons + it } ?: current.icons else current.icons
            order[id] = current.copy(appCount = current.appCount + 1, icons = icons)
        }
        return order.values.toList()
    }

    /** Все приложения разработчика в российской и американской витринах, по алфавиту. */
    suspend fun developerApps(artistId: String): List<AppStoreSearchHit> = coroutineScope {
        val id = artistId.trim()
        require(id.isNotEmpty() && id.all { it in '0'..'9' }) { "Не указан разработчик." }
        val pages = listOf("ru", "us").map { country ->
            async(Dispatchers.IO) {
                itunesGet(ITUNES_LOOKUP, "id" to id, "entity" to "software", "country" to country, "limit" to "200")
            }
        }.awaitAll()
        val apps = pages.flatten().distinctBy { it.adamId }.sortedBy { it.name.lowercase() }
        if (apps.isEmpty()) error("У этого разработчика не нашлось приложений для iPhone.")
        apps
    }

    // ── Объединение с каталогом ────────────────────────────────────────────

    /** Результат App Store; `catalog` — карточка каталога с тем же App Store ID. */
    data class MergedHit(val hit: AppStoreSearchHit, val catalog: CatalogApp?)

    /**
     * Одно приложение — одна карточка. Результат App Store с тем же ID, что у
     * приложения каталога, заменяется карточкой каталога (у неё проверенные
     * версии и восстановление из покупок); если она уже видна выше —
     * [shownCatalogIds] — не повторяется вовсе.
     */
    fun mergeWithCatalog(
        hits: List<AppStoreSearchHit>,
        catalog: List<CatalogApp>,
        shownCatalogIds: Set<String>,
    ): List<MergedHit> {
        val byStoreId = HashMap<String, CatalogApp>()
        catalog.forEach { app ->
            (app.app_store_ids + listOfNotNull(app.app_store_id)).forEach { raw ->
                val key = raw.trim()
                if (key.isNotEmpty()) byStoreId.putIfAbsent(key, app)
            }
        }
        val seenCatalog = HashSet<String>()
        val seenHits = HashSet<String>()
        val out = ArrayList<MergedHit>()
        for (hit in hits) {
            val key = hit.adamId.trim()
            if (!seenHits.add(key)) continue
            val app = byStoreId[key]
            if (app != null) {
                if (app.id in shownCatalogIds || !seenCatalog.add(app.id)) continue
                // Карточка каталога, дополненная данными магазина: значок, если
                // у каталога его нет.
                val enriched = if (app.icon_url.isNullOrBlank() && !hit.artwork.isNullOrBlank()) {
                    app.copy(icon_url = hit.artwork)
                } else app
                out += MergedHit(hit, enriched)
            } else {
                out += MergedHit(hit, null)
            }
        }
        return out
    }

    // ── JSON ──────────────────────────────────────────────────────────────

    private fun JsonObject.text(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString || it.longOrNull != null }
            ?.content?.trim()?.takeIf { it.isNotEmpty() && !it.equals("null", true) }

    /** Числовое поле, которое Apple отдаёт то числом, то строкой. */
    private fun JsonObject.numeric(key: String): String? {
        val p = this[key] as? JsonPrimitive ?: return null
        p.longOrNull?.let { return it.toString() }
        return p.content.trim().takeIf { it.isNotEmpty() && it.all { ch -> ch in '0'..'9' } }
    }
}
