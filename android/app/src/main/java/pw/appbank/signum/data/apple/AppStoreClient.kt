package pw.appbank.signum.data.apple

import android.util.Log
import com.dd.plist.NSArray
import com.dd.plist.NSData
import com.dd.plist.NSDictionary
import com.dd.plist.NSNumber
import com.dd.plist.NSString
import android.util.Base64
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import pw.appbank.signum.data.api.Http
import pw.appbank.signum.data.api.SignumJson
import pw.appbank.signum.data.api.str
import pw.appbank.signum.data.catalog.CatalogResolve
import pw.appbank.signum.data.models.AppStoreSearchHit
import pw.appbank.signum.data.signing.IpaTweaks
import pw.appbank.signum.data.models.AppStoreVersion
import pw.appbank.signum.data.models.LoginResult
import pw.appbank.signum.data.settings.SettingsStore
import java.io.File
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.TimeZone
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

class AppStoreClient(
    private val store: SettingsStore,
    private val anisette: AnisetteClient,
    private val gs: GrandSlamClient,
) {
    private val sap = SapSigner(store)
    private var passwordToken: String? = null
    private var dsPersonId: String? = null
    private var loginEmail: String? = null
    private var loginPassword: String? = null
    private var storeFront: String = "143469-2,8"                                  
    private val guid: String = loadOrCreateGuid()
    private val storeGuid: String = guid.replace(".", "")
    private var pod: String = "25"
    private val cookies = ConcurrentHashMap<String, String>()
    private val versionIdCache = ConcurrentHashMap<String, List<String>>()
    private val versionLabelCache = ConcurrentHashMap<String, String>()
    private val bundleAdamCache = ConcurrentHashMap<String, String>()

    fun isReady(): Boolean = !passwordToken.isNullOrBlank() && !dsPersonId.isNullOrBlank()

    fun clear() {
        passwordToken = null
        dsPersonId = null
        loginEmail = null
        loginPassword = null
        cookies.clear()
        versionIdCache.clear()
        bundleAdamCache.clear()
        pod = "25"
        storeFront = "143469-2,8"
    }

    /**
     * ipatool-style direct Store login: post the credentials (2FA code appended
     * to the password, exactly like ipatool) to the iTunes authenticate endpoint
     * with the FairPlay SAP signature that attests this device. No GrandSlam SRP
     * and no anisette trusted-device dance — Apple drives 2FA through the Store
     * itself (empty failureType + MZFinance.BadLogin customer message), which
     * both sends the push and accepts the code on resubmit.
     */
    fun login(email: String, password: String, code: String?): LoginResult {
        val em = email.trim().lowercase(Locale.US)
        if (em.isBlank() || password.isBlank()) error("Введите Apple ID и пароль")
        clear()
        loginEmail = em
        loginPassword = password
        sap.useAccountIdentity(em, store.load().ipatoolEngine == "v6" || store.load().ipatoolEngine == "auto")
        store.storePassword(em, password)
        store.markAccountAdded(em)
        return when (val outcome = runAuthenticate(em, password, code)) {
            is AuthOutcome.Success -> {
                store.setActiveEmail(em)
                persistStore()
                android.util.Log.i("SignumStore", "login-ok direct-authenticate")
                LoginResult(email = em, message = "Apple ID добавлен и выбран для установок")
            }
            is AuthOutcome.Needs2fa -> LoginResult(email = em, needs2fa = true, message = outcome.message)
            is AuthOutcome.Failed -> error(outcome.message)
        }
    }

    /** Re-request a fresh 2FA code by resubmitting the login without a code. */
    fun resendCode(): LoginResult {
        val em = loginEmail ?: store.activeEmail() ?: error("Сначала введите Apple ID и пароль.")
        val pw = loginPassword ?: store.loadPassword(em) ?: error("Введите пароль заново.")
        return login(em, pw, null)
    }

    fun hasSavedCredentials(): Boolean {
        val em = loginEmail ?: store.activeEmail() ?: return false
        return !(loginPassword ?: store.loadPassword(em)).isNullOrBlank()
    }

    /**
     * Re-authenticate an existing account whose passwordToken expired, using the
     * saved password. Kept as `authenticate(force)` for the download/purchase
     * callers; throws (asking for re-login) if Apple now wants a fresh 2FA code.
     */
    fun authenticate(force: Boolean = false): Boolean {
        if (!force) {
            restoreTokens()
            if (isReady()) return true
        } else {
            passwordToken = null
            dsPersonId = null
            cookies.clear()
        }
        val em = loginEmail?.ifBlank { null } ?: store.activeEmail()
            ?: error("Сначала войдите в Apple ID на странице «Аккаунт».")
        val pw = loginPassword ?: store.loadPassword(em)
            ?: error(STORE_SESSION_MESSAGE)
        loginEmail = em
        loginPassword = pw
        return when (val outcome = runAuthenticate(em, pw, null)) {
            is AuthOutcome.Success -> {
                store.setActiveEmail(em)
                persistStore()
                android.util.Log.i("SignumStore", "auth-ok direct-authenticate")
                true
            }
            is AuthOutcome.Needs2fa ->
                error("Требуется повторный вход с кодом 2FA. Откройте «Аккаунт» и войдите снова.")
            is AuthOutcome.Failed -> error(outcome.message)
        }
    }

    private sealed class AuthOutcome {
        object Success : AuthOutcome()
        data class Needs2fa(val message: String) : AuthOutcome()
        data class Failed(val message: String) : AuthOutcome()
    }

    private fun runAuthenticate(email: String, password: String, authCode: String?): AuthOutcome {
        passwordToken = null
        dsPersonId = null
        val code = authCode?.filter { !it.isWhitespace() }.orEmpty()
        val credential = password + code
        // Anisette headers are best-effort device attestation; the SAP signature
        // is the primary one (ipatool sends anisette when available, empty else).
        val ani = runCatching { anisette.getData(gs) }.getOrNull()
        val endpoints = listOf(
            "https://auth.itunes.apple.com/auth/v1/native/fast/",
            "https://p25-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/authenticate?Pod=25&PRH=25",
            "https://p6-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/authenticate?Pod=6&PRH=6",
            "https://buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/authenticate",
        )
        var lastFailure: AuthOutcome = AuthOutcome.Failed(STORE_SESSION_MESSAGE)
        for (endpoint in endpoints) {
            var url = endpoint
            for (attempt in 1..2) {
                val xml = Plist.xml(
                    Plist.dict(
                        "appleId" to email,
                        "attempt" to attempt.toString(),
                        "guid" to storeGuid,
                        "password" to credential,
                        "rmp" to "0",
                        "why" to "signIn",
                    ),
                )
                // sapfix: Apple rejects an unsigned authenticate with 403
                // (AMD-Action::SP). Sign the exact posted body via the SAP helper.
                val sapHeader = runCatching { sap.headerValue(xml) }.getOrNull()
                if (sapHeader.isNullOrBlank()) {
                    android.util.Log.w("SignumStore", "authenticate: SAP signature unavailable")
                }
                val req = Request.Builder()
                    .url(url)
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .header("Accept", "*/*")
                    .header("User-Agent", STORE_UA)
                    .apply {
                        if (cookies.isNotEmpty()) {
                            header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
                        }
                        ani?.storeHeaders()?.forEach { (k, v) -> header(k, v) }
                        if (!sapHeader.isNullOrBlank()) header("X-Apple-ActionSignature", sapHeader)
                    }
                    .post(xml.toRequestBody("application/x-www-form-urlencoded".toMediaType()))
                    .build()
                val response = try {
                    StorePodRouting.execute(
                        url,
                        send = { nextUrl ->
                            url = nextUrl
                            val routed = req.newBuilder().url(nextUrl).removeHeader("Cookie").apply {
                                if (cookies.isNotEmpty()) header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
                            }.build()
                            Http.itunesNoFollow.newCall(routed).execute().use { resp ->
                                ingestCookies(resp.headers("Set-Cookie"), resp.headers)
                                StorePodRouting.Reply(
                                    resp.code, Triple(resp.code, resp.headers, resp.body?.bytes() ?: ByteArray(0)),
                                    resp.header("Location"), resp.header("pod") ?: cookies["itspod"],
                                )
                            }
                        },
                        onPod = { pod = it },
                        resolveRedirect = ::resolveAuthRedirect,
                    )
                } catch (e: java.net.UnknownHostException) {
                    // A failed auth hostname must not prevent the remaining
                    // supported Store endpoints from being tried.
                    lastFailure = AuthOutcome.Failed("Не удалось найти сервер App Store. Проверьте интернет и повторите вход.")
                    break
                }
                val (respCode, headers, bytes) = response
                logStoreAuth("pw-itunes", url, respCode, bytes)
                if (respCode == 204 || respCode == 403 || respCode == 401 || respCode == 404 || respCode == 503) {
                    lastFailure = AuthOutcome.Failed(STORE_SESSION_MESSAGE)
                    break
                }
                val dict = parseStoreAuth(bytes)
                if (dict == null) {
                    lastFailure = AuthOutcome.Failed(STORE_SESSION_MESSAGE)
                    break
                }
                headers["x-set-apple-store-front"]?.let { storeFront = it }
                headers["x-apple-store-front"]?.let { storeFront = it }
                dict.str("storeFront")?.let { storeFront = it }
                val failure = dict.str("failureType").orEmpty()
                val msg = dict.str("customerMessage").orEmpty()
                passwordToken = dict.str("passwordToken")
                dsPersonId = dict.str("dsPersonId") ?: dict.dict("accountInfo")?.str("dsPersonId")
                if (!passwordToken.isNullOrBlank() && !dsPersonId.isNullOrBlank()) {
                    return AuthOutcome.Success
                }
                // Гейт 2FA — это ПУСТОЙ failureType вместе с MZFinance.BadLogin.
                // Проверки на пустоту здесь не было, а на неверный пароль Apple
                // отвечает тем же текстом, но с заполненным failureType (-5000).
                // Из-за этого любой неверный пароль объявлялся «код отправлен на
                // iPhone», а ветка «неверный Apple ID или пароль» ниже была
                // недостижима. В движке на ПК это условие стоит правильно.
                if ((failure.isBlank() && msg == CUSTOMER_MESSAGE_BAD_LOGIN) ||
                    looksLike2fa(failure, msg)
                ) {
                    return if (code.isBlank()) {
                        AuthOutcome.Needs2fa(TWO_FACTOR_MESSAGE)
                    } else {
                        AuthOutcome.Failed("Код 2FA отклонён. Запросите новый код и попробуйте снова.")
                    }
                }
                if (failure == FAILURE_INVALID_CREDENTIALS) {
                    // ipatool: attempt 1 always retries once before erroring.
                    if (attempt == 1) continue
                    lastFailure = AuthOutcome.Failed(
                        if (code.isNotBlank()) "Код 2FA отклонён." else "Неверный Apple ID или пароль.",
                    )
                    break
                }
                if (msg == CUSTOMER_MESSAGE_ACCOUNT_DISABLED) {
                    return AuthOutcome.Failed("Аккаунт Apple ID отключён.")
                }
                if (failure.isNotBlank() && failure != "0") {
                    lastFailure = AuthOutcome.Failed(
                        if (msg.isNotBlank()) localizeStoreMessage(msg) else STORE_SESSION_MESSAGE,
                    )
                    break
                }
                lastFailure = AuthOutcome.Failed(
                    if (msg.isNotBlank() && !msg.contains("sign in", true)) msg else STORE_SESSION_MESSAGE,
                )
                break
            }
        }
        return lastFailure
    }

    private fun looksLike2fa(failure: String, msg: String): Boolean =
        failure == "2034" || msg.contains("two-factor", true) ||
            (msg.contains("code", true) && msg.contains("enter", true))

    private fun logStoreAuth(secretName: String, url: String, code: Int, bytes: ByteArray) {
        val preview = runCatching { bytes.toString(Charsets.UTF_8) }.getOrDefault("")
            .replace(Regex("[A-Za-z0-9+/=]{24,}"), "***")
            .replace('\n', ' ')
            .take(220)
        val host = url.substringAfter("://").substringBefore("/")
        val line = "SIGNUM_STORE $secretName $code $host bytes=${bytes.size} $preview"
        System.err.println(line)
        runCatching { File(store.filesDir(), "store-auth.log").appendText(line + "\n") }
    }

    private fun appleClientTime(): String =
        DateTimeFormatter.ISO_INSTANT.format(Instant.now().atOffset(ZoneOffset.UTC))

    private fun parseStoreAuth(bytes: ByteArray): NSDictionary? {
        runCatching { return Plist.parse(bytes) }
        val text = runCatching { bytes.toString(Charsets.UTF_8) }.getOrDefault("").trim()
        if (!text.startsWith("{")) return null
        val obj = runCatching { SignumJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return null
        val d = NSDictionary()
        obj["passwordToken"]?.jsonPrimitive?.contentOrNull?.let { d.put("passwordToken", NSString(it)) }
        obj["dsPersonId"]?.jsonPrimitive?.contentOrNull?.let { d.put("dsPersonId", NSString(it)) }
        obj["storeFront"]?.jsonPrimitive?.contentOrNull?.let { d.put("storeFront", NSString(it)) }
        obj["failureType"]?.jsonPrimitive?.contentOrNull?.let { d.put("failureType", NSString(it)) }
        obj["customerMessage"]?.jsonPrimitive?.contentOrNull?.let { d.put("customerMessage", NSString(it)) }
        return if (d.count() > 0) d else null
    }

    private fun resolveUrl(base: String, loc: String): String {
        return StorePodRouting.redirectUrl(base, loc)
    }

    private fun resolveAuthRedirect(base: String, loc: String): String {
        val resolved = resolveUrl(base, loc)
        rememberPod(resolved)
        val host = runCatching { java.net.URI(resolved).host.orEmpty() }.getOrDefault("")
        if (host.equals("auth.itunes.apple.com", ignoreCase = true) &&
            resolved.contains("/WebObjects/", ignoreCase = true)
        ) {
            return "https://p$pod-buy.itunes.apple.com" +
                java.net.URI(resolved).rawPath.orEmpty() +
                (java.net.URI(resolved).rawQuery?.let { "?$it" } ?: "")
        }
        if (loc.startsWith("/") && host.contains("auth.itunes.apple.com", ignoreCase = true)) {
            return "https://p$pod-buy.itunes.apple.com$loc"
        }
        return resolved
    }

    fun restoreTokens() {
        val email = store.activeEmail() ?: return
        val blob = store.loadSessionBlob("store_$email") ?: return
        runCatching {
            val dict = Plist.parse(android.util.Base64.decode(blob, android.util.Base64.NO_WRAP))
            passwordToken = dict.str("passwordToken")
            dsPersonId = dict.str("dsPersonId")
            dict.str("storeFront")?.let { storeFront = it }
            StorePodRouting.validPod(dict.str("pod"))?.let { pod = it }
            dict.str("cookies")?.split(';')?.forEach { part ->
                val idx = part.indexOf('=')
                if (idx > 0) cookies[part.substring(0, idx).trim()] = part.substring(idx + 1).trim()
            }
        }
    }

    private fun persistStore() {
        val email = loginEmail ?: store.activeEmail() ?: return
        val dict = Plist.dict(
            "passwordToken" to passwordToken,
            "dsPersonId" to dsPersonId,
            "storeFront" to storeFront,
            "pod" to pod,
            "cookies" to cookies.entries.joinToString(";") { "${it.key}=${it.value}" },
        )
        store.storeSessionBlob(
            "store_$email",
            android.util.Base64.encodeToString(Plist.xml(dict), android.util.Base64.NO_WRAP),
        )
    }

    /**
     * Номер приложения в App Store, если человек ввёл его, а не название.
     *
     * Годится и голый номер, и ссылка вида apps.apple.com/ru/app/…/id123456789 —
     * её проще скопировать с телефона, чем переписывать цифры.
     */
    fun adamIdFromQuery(query: String): String? {
        val q = query.trim()
        val compact = q.filterNot(Char::isWhitespace)
        if (compact.length in 8..12 && compact.all { it in '0'..'9' }) return compact
        return Regex("""(?i)(?:^|[/ ?&])id\s*=?\s*(\d{8,12})(?!\d)""")
            .find(q)?.groupValues?.get(1)
    }

    fun searchAppMagic(term: String): List<AppStoreSearchHit> {
        val q = adamIdFromQuery(term) ?: term.trim()
        if (q.isEmpty()) return emptyList()
        val url = "https://appmagic.rocks/api/v2/search?name=${java.net.URLEncoder.encode(q, "UTF-8")}&limit=100"
        val request = Request.Builder().url(url)
            .header("User-Agent", "Mozilla/5.0")
            .header("Referer", "https://appmagic.rocks/").build()
        return Http.itunes.newCall(request).execute().use { response ->
            check(response.isSuccessful) { "Не удалось выполнить поиск AppMagic" }
            val root = SignumJson.parseToJsonElement(response.body?.string().orEmpty()).jsonObject
            root["applications"]?.jsonArray.orEmpty().mapNotNull { element ->
                val app = element.jsonObject
                val id = app["store_ids"]?.jsonArray.orEmpty().mapNotNull { it.jsonPrimitive.contentOrNull }
                    .firstOrNull { it.startsWith("2_") || it.startsWith("3_") }
                    ?.substringAfter('_')?.takeIf { it.isNotEmpty() && it.all(Char::isDigit) }
                    ?: return@mapNotNull null
                AppStoreSearchHit(
                    adamId = id,
                    name = app["name"]?.jsonPrimitive?.contentOrNull ?: return@mapNotNull null,
                    artist = app["unitedPublisher"]?.jsonObject?.get("name")?.jsonPrimitive?.contentOrNull,
                    artwork = (app["icon"] as? kotlinx.serialization.json.JsonPrimitive)?.contentOrNull,
                )
            }.distinctBy { it.adamId }
        }
    }

    fun search(term: String, country: String = "ru"): List<AppStoreSearchHit> {
        val q = term.trim()
        if (q.isEmpty()) return emptyList()
        // Поиск по названию номер не понимает: на запрос из одних цифр Apple
        // отвечает пустым списком. Для номера есть отдельный запрос — lookup.
        val byId = adamIdFromQuery(q)
        val url = if (byId != null) {
            "https://itunes.apple.com/lookup?id=$byId&entity=software&country=$country"
        } else {
            "https://itunes.apple.com/search?term=${java.net.URLEncoder.encode(q, "UTF-8")}" +
                "&entity=software&limit=25&country=$country"
        }
        val req = Request.Builder().url(url).header("User-Agent", STORE_UA).get().build()
        Http.itunes.newCall(req).execute().use { resp ->
            val body = resp.body?.string().orEmpty()
            val root = SignumJson.parseToJsonElement(body).jsonObject
            val results = root["results"]?.jsonArray ?: return emptyList()
            return results.mapNotNull { el ->
                val o = el.jsonObject
                val adam = o["trackId"]?.jsonPrimitive?.content ?: return@mapNotNull null
                AppStoreSearchHit(
                    adamId = adam,
                    name = o["trackName"]?.jsonPrimitive?.content ?: "App",
                    artist = o["artistName"]?.jsonPrimitive?.content,
                    artistId = o["artistId"]?.jsonPrimitive?.content,
                    artwork = o["artworkUrl100"]?.jsonPrimitive?.content
                        ?: o["artworkUrl60"]?.jsonPrimitive?.content,
                    bundleId = o["bundleId"]?.jsonPrimitive?.content,
                    version = o["version"]?.jsonPrimitive?.content,
                    minIos = o["minimumOsVersion"]?.jsonPrimitive?.content,
                    price = o["formattedPrice"]?.jsonPrimitive?.content,
                )
            }
        }
    }

    /**
     * Приложения одного разработчика.
     *
     * Отдельный запрос lookup: обычный поиск по названию их не соберёт, а
     * человеку часто нужно «что ещё у этой студии», когда точного названия он
     * не помнит.
     */
    fun developerApps(artistId: String, country: String = "ru"): List<AppStoreSearchHit> {
        val id = artistId.trim()
        if (id.isEmpty() || !id.all { it.isDigit() }) return emptyList()
        val url = "https://itunes.apple.com/lookup?id=$id&entity=software&limit=100&country=$country"
        val req = Request.Builder().url(url).header("User-Agent", STORE_UA).get().build()
        Http.itunes.newCall(req).execute().use { resp ->
            val body = resp.body?.string().orEmpty()
            val root = SignumJson.parseToJsonElement(body).jsonObject
            val results = root["results"]?.jsonArray ?: return emptyList()
            return results.mapNotNull { el ->
                val o = el.jsonObject
                // Первым элементом lookup отдаёт самого разработчика — у него
                // нет trackId, и в список приложений он не годится.
                val adam = o["trackId"]?.jsonPrimitive?.content ?: return@mapNotNull null
                AppStoreSearchHit(
                    adamId = adam,
                    name = o["trackName"]?.jsonPrimitive?.content ?: "App",
                    artist = o["artistName"]?.jsonPrimitive?.content,
                    artistId = o["artistId"]?.jsonPrimitive?.content,
                    artwork = o["artworkUrl100"]?.jsonPrimitive?.content
                        ?: o["artworkUrl60"]?.jsonPrimitive?.content,
                    bundleId = o["bundleId"]?.jsonPrimitive?.content,
                    version = o["version"]?.jsonPrimitive?.content,
                    minIos = o["minimumOsVersion"]?.jsonPrimitive?.content,
                    price = o["formattedPrice"]?.jsonPrimitive?.content,
                )
            }
        }
    }

    fun listVersionIds(adamId: String, force: Boolean = false): List<String> {
        val key = adamId.trim()
        if (key.isEmpty()) return emptyList()
        val owner = store.activeEmail() ?: error("Сначала войдите в Apple ID")
        val ownedKey = "$owner:$key"
        if (!force) versionIdCache[ownedKey]?.let { return it }
        if (!force) store.loadHistoryCache("versions_$ownedKey")?.let { raw ->
            runCatching {
                val cached = SignumJson.parseToJsonElement(raw).jsonObject
                val timestamp = cached["at"]?.jsonPrimitive?.content?.toLongOrNull() ?: 0
                if (System.currentTimeMillis() - timestamp in 0..86_400_000) {
                    val ids = cached["ids"]?.jsonArray.orEmpty().map { it.jsonPrimitive.content }
                    versionIdCache[ownedKey] = ids
                    return ids
                }
            }
        }
        ensureAuth()
        val download = downloadPlist(key, purchase = false, externalVersionId = null, versionOnly = true)
        val song = firstSong(download)
        if (song == null) {
            return emptyList()
        }
        val (ids, latest) = extractVersionIds(song)
        val ordered = if (latest != null && ids.contains(latest)) {
            listOf(latest) + ids.filter { it != latest }
        } else {
            ids
        }
        if (store.activeEmail() != owner) error("Apple ID изменился. Откройте приложение заново.")
        versionIdCache[ownedKey] = ordered
        store.storeHistoryCache("versions_$ownedKey", kotlinx.serialization.json.buildJsonObject {
            put("at", kotlinx.serialization.json.JsonPrimitive(System.currentTimeMillis()))
            put("ids", kotlinx.serialization.json.JsonArray(ordered.map { kotlinx.serialization.json.JsonPrimitive(it) }))
        }.toString())
        return ordered
    }

    fun resolveAdamId(appStoreId: String?, bundleId: String?): String? {
        val adam = appStoreId?.trim()?.takeIf { it.isNotEmpty() && it.all(Char::isDigit) }
        if (adam != null) return adam
        val bundle = bundleId?.trim()?.takeIf { it.isNotEmpty() && it != "NONE" } ?: return null
        bundleAdamCache[bundle]?.let { return it }
        val found = lookupAdamByBundle(bundle) ?: return null
        bundleAdamCache[bundle] = found
        return found
    }

    fun versionDisplay(adamId: String, externalVersionId: String): String {
        val ext = externalVersionId.trim()
        if (ext.isEmpty() || ext == "latest") return ext
        val owner = store.activeEmail().orEmpty()
        val cacheKey = "$owner:$adamId:$ext"
        versionLabelCache[cacheKey]?.let { return it }
        store.loadHistoryCache("version_label_$cacheKey")?.let { return it }
        val value = runCatching {
            val download = downloadPlist(adamId, purchase = false, externalVersionId = ext, versionOnly = true)
            val song = firstSong(download) ?: return@runCatching ext
            marketingVersion(song) ?: ext
        }.getOrDefault(ext)
        if (value != ext && store.activeEmail() == owner) {
            versionLabelCache[cacheKey] = value
            store.storeHistoryCache("version_label_$cacheKey", value)
        }
        return value
    }

    fun listVersions(adamId: String, force: Boolean = false): List<AppStoreVersion> {
        val ids = listVersionIds(adamId, force)
        if (ids.isEmpty()) return emptyList()
        return ids.mapIndexed { index, id ->
            AppStoreVersion(
                version = versionDisplay(adamId, id),
                externalVersionId = id,
                isLatest = index == 0,
                alreadyOwned = true,
            )
        }
    }

    fun ownsListing(appStoreId: String?, bundleId: String?, force: Boolean = false): Boolean {
        val adam = resolveAdamId(appStoreId, bundleId) ?: return false
        return listVersionIds(adam, force).isNotEmpty()
    }

    fun purchaseAndDownload(
        adamId: String,
        dest: File,
        externalVersionId: String? = null,
        alreadyOwned: Boolean = false,
        onProgress: (Float, String) -> Unit,
    ): File {
        ensureAuth()
        onProgress(8f, "Покупка в App Store…")
        if (!alreadyOwned) {
            runCatching { buy(adamId) }
        }
        onProgress(22f, "Запрашиваем IPA…")
        val plist = downloadPlist(adamId, purchase = true, externalVersionId = externalVersionId, acceptTerms = true)
        val song = firstSong(plist) ?: error("App Store не вернул ссылку на приложение.")
        val url = song.str("URL") ?: error("App Store не вернул ссылку на приложение.")
        onProgress(35f, "Скачиваем IPA…")
        downloadIpaFile(url, dest, onProgress) {
            val again = downloadPlist(adamId, purchase = true, externalVersionId = externalVersionId)
            firstSong(again)?.str("URL")
        }
        if (dest.length() < 1000) error("Скачанный файл повреждён")
        // Лицензию Apple отдаёт отдельно от файла, в теле того же ответа. Без
        // неё приложение ставится, но вылетает при запуске: iOS не получает
        // ключ FairPlay. Десктоп вшивал её давно, Android — нет.
        onProgress(66f, "Вшиваем лицензию…")
        runCatching { injectLicense(dest, song) }.getOrElse { error ->
            Log.e("SignumStore", "не удалось вшить лицензию", error)
            error("Не удалось встроить лицензию Apple в IPA: ${error.message ?: "неизвестная ошибка"}")
        }
        onProgress(70f, "IPA скачан")
        return dest
    }

    /** Достаёт из ответа `sinfs` и `metadata` и кладёт их внутрь IPA. */
    private fun injectLicense(dest: File, song: NSDictionary) {
        val sinfs = (song.objectForKey("sinfs") as? NSArray)
            ?.array
            ?.mapNotNull { (it as? NSDictionary)?.objectForKey("sinf") as? NSData }
            ?.map { it.bytes() }
            .orEmpty()
        val metadata = (song.objectForKey("metadata") as? NSDictionary)?.let { meta ->
            runCatching { Plist.xml(meta) }.getOrNull()
        }
        if (sinfs.isEmpty() && metadata == null) return
        IpaTweaks.injectStoreLicense(dest, metadata, sinfs)
    }

    private fun downloadIpaFile(
        firstUrl: String,
        dest: File,
        onProgress: (Float, String) -> Unit,
        refreshUrl: () -> String?,
    ) {
        dest.parentFile?.mkdirs()
        var url = forceHttps(firstUrl)
        var last: Exception? = null
        var expected = -1L
        for (attempt in 0 until 8) {
            val have = dest.takeIf { it.exists() }?.length() ?: 0L
            if (attempt > 0) {
                val fresh = runCatching { refreshUrl()?.let(::forceHttps) }.getOrNull()
                if (!fresh.isNullOrBlank()) url = fresh
            }
            try {
                android.util.Log.i("SignumDl", "try=${attempt + 1} have=$have expected=$expected host=${runCatching { java.net.URI(url).host }.getOrNull()}")
                expected = pullCdn(url, dest, have, expected, onProgress)
                val size = dest.length()
                android.util.Log.i("SignumDl", "chunk-end bytes=$size expected=$expected")
                if (expected > 0 && size < expected) {
                    last = java.io.IOException("неполный файл $size из $expected")
                    onProgress(35f, "Связь оборвалась, докачиваем… ${size / 1024 / 1024} МБ")
                    Thread.sleep(500L * (attempt + 1))
                    continue
                }
                if (size >= 1000) return
                last = IllegalStateException("Скачанный файл слишком маленький")
            } catch (e: Exception) {
                if (e is pw.appbank.signum.data.jobs.JobCancelledException) throw e
                last = e
                android.util.Log.e("SignumDl", "fail try=${attempt + 1} have=${dest.length()}", e)
                if (dest.length() < 1000) dest.delete()
                onProgress(35f, "Связь оборвалась, докачиваем… ${dest.length() / 1024 / 1024} МБ")
                Thread.sleep(600L * (attempt + 1))
            }
        }
        val msg = last?.message.orEmpty()
        if (msg.contains("reset", true) || msg.contains("timeout", true) ||
            msg.contains("Connection", true) || last is java.net.SocketException
        ) {
            error("Скачивание IPA оборвалось на ${dest.length() / 1024 / 1024} МБ. Повторите установку — докачаем.")
        }
        error(last?.message ?: "Не удалось скачать IPA")
    }

    private fun pullCdn(
        startUrl: String,
        dest: File,
        have: Long,
        knownExpected: Long,
        onProgress: (Float, String) -> Unit,
    ): Long {
        var current = startUrl
        repeat(8) {
            val conn = (java.net.URL(current).openConnection() as java.net.HttpURLConnection).apply {
                instanceFollowRedirects = false
                connectTimeout = 20_000
                readTimeout = 15_000
                useCaches = false
                requestMethod = "GET"
                setRequestProperty("User-Agent", STORE_UA)
                setRequestProperty("Accept", "*/*")
                setRequestProperty("Accept-Encoding", "identity")
                setRequestProperty("Connection", "close")
                if (cookies.isNotEmpty()) {
                    setRequestProperty("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
                }
                if (have > 0) setRequestProperty("Range", "bytes=$have-")
            }
            try {
                val code = conn.responseCode
                val range = conn.getHeaderField("Content-Range")
                val bodyLen = conn.contentLengthLong
                android.util.Log.i(
                    "SignumDl",
                    "http=$code len=$bodyLen range=$range enc=${conn.contentEncoding} loc=${conn.getHeaderField("Location")}",
                )
                if (code == 301 || code == 302 || code == 303 || code == 307 || code == 308) {
                    val loc = conn.getHeaderField("Location")?.trim().orEmpty()
                    if (loc.isEmpty()) error("Редирект без адреса при скачивании IPA")
                    current = forceHttps(java.net.URL(java.net.URL(current), loc).toString())
                    return@repeat
                }
                if (code == 416) {
                    if (have >= 1000) return have
                    dest.delete()
                    error("Сервер отклонил докачку, начинаем заново")
                }
                if (code != 200 && code != 206) {
                    error("Не удалось скачать IPA ($code)")
                }
                val rangeTotal = range?.substringAfter('/')?.toLongOrNull()
                val expected = rangeTotal
                    ?: if (bodyLen > 0) have + bodyLen else knownExpected
                val append = have > 0 && (code == 206 || range != null)
                java.io.FileOutputStream(dest, append).use { out ->
                    conn.inputStream.use { input ->
                        val buf = ByteArray(64 * 1024)
                        var read = if (append) have else 0L
                        var sinceFlush = 0
                        while (true) {
                            val n = input.read(buf)
                            if (n <= 0) break
                            out.write(buf, 0, n)
                            read += n
                            sinceFlush += n
                            if (sinceFlush >= 512 * 1024) {
                                out.flush()
                                sinceFlush = 0
                            }
                            val mb = read / 1024 / 1024
                            val totalMb = if (expected > 0) "/${expected / 1024 / 1024}" else ""
                            val pct = if (expected > 0) {
                                35f + (read.toFloat() / expected.toFloat()) * 34f
                            } else {
                                50f
                            }
                            onProgress(pct.coerceIn(35f, 69f), "Скачиваем IPA… $mb${totalMb} МБ")
                            if (expected > 0 && read >= expected) break
                        }
                    }
                }
                return expected
            } finally {
                conn.disconnect()
            }
        }
        error("Слишком много редиректов при скачивании IPA")
    }

    private fun forceHttps(url: String): String {
        return if (url.startsWith("http://", ignoreCase = true)) {
            "https://" + url.removePrefix("http://").removePrefix("HTTP://")
        } else {
            url
        }
    }

    private fun buy(adamId: String) {
        val ani = anisette.getData(gs)
        val body = Plist.dict(
            "guid" to storeGuid,
            "salableAdamId" to adamId,
            "price" to "0",
            "pricingParameters" to "STDQ",
            "productType" to "C",
            "appExtVrsId" to "0",
        )
        var dict = postStorePlist(
            "https://buy.itunes.apple.com/WebObjects/MZBuy.woa/wa/buyProduct",
            body,
            ani,
        )
        if (dict != null && isTermsDialog(dict) && acceptStoreTerms(ani, dict)) {
            dict = postStorePlist(
                "https://buy.itunes.apple.com/WebObjects/MZBuy.woa/wa/buyProduct",
                body,
                ani,
            )
        }
        val fail = dict?.str("failureType")
        val msg = dict?.str("customerMessage")
        if (fail != null && fail != "0" && msg?.contains("already", true) != true) {
            if (msg?.contains("not found", true) == true) return
            if (isTermsDialog(dict)) {
                error("Apple просит подтвердить условия. Подождите минуту и повторите установку.")
            }
            if (!msg.isNullOrBlank()) error(localizeStoreMessage(msg))
        }
    }

    private fun downloadPlist(
        adamId: String,
        purchase: Boolean,
        externalVersionId: String?,
        versionOnly: Boolean = false,
        acceptTerms: Boolean = false,
    ): NSDictionary {
        val ani = anisette.getData(gs)
        val fields = mutableMapOf<String, Any?>(
            "creditDisplay" to "",
            "guid" to storeGuid,
            "salableAdamId" to adamId,
            "passwordToken" to passwordToken,
            "dsPersonId" to dsPersonId,
        )
        if (purchase) fields["ageCheck"] = true
        if (!externalVersionId.isNullOrBlank() && externalVersionId != "latest") {
            fields["externalVersionId"] = externalVersionId
        }
        val body = Plist.dict(*fields.entries.map { it.key to it.value }.toTypedArray())
        val url = "https://p$pod-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/volumeStoreDownloadProduct?guid=$storeGuid"
        var dict = postStorePlist(url, body, ani)
            ?: error("Не удалось получить данные App Store")
        if (acceptTerms && isTermsDialog(dict) && acceptStoreTerms(ani, dict)) {
            dict = postStorePlist(url, body, ani)
                ?: error("Не удалось получить данные App Store")
        }
        val fail = dict.str("failureType")
        val msg = dict.str("customerMessage")
        if (isPasswordTokenFailure(fail, msg) || isUnknownStoreFailure(fail, msg) || isItunesSignIn(fail, msg)) {
            passwordToken = null
            cookies.clear()
            if (!versionOnly) {
                authenticate(force = true)
                val retryUrl = "https://p$pod-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/volumeStoreDownloadProduct?guid=$storeGuid"
                fields["passwordToken"] = passwordToken
                fields["dsPersonId"] = dsPersonId
                val retryBody = Plist.dict(*fields.entries.map { it.key to it.value }.toTypedArray())
                val retry = postStorePlist(retryUrl, retryBody, anisette.getData(gs))
                    ?: error("Не удалось получить данные App Store")
                val fail2 = retry.str("failureType")
                val msg2 = retry.str("customerMessage")
                if (fail2 != null && fail2 != "0") error(localizeStoreMessage(msg2 ?: fail2))
                return retry
            }
            if (isPasswordTokenFailure(fail, msg)) error(PASSWORD_TOKEN_MESSAGE)
            error(localizeStoreMessage(msg ?: fail ?: "Не удалось проверить вход в App Store."))
        }
        if (versionOnly && isLicenseFailure(fail, msg)) return dict
        // Примерно с 1 сентября 2026 Apple перестала отвечать на
        // volumeStoreDownloadProduct для сборок, выпущенных с этой даты, —
        // приходит пустой songList. Без второй ступени такие сборки не
        // скачиваются совсем и не показывают версию.
        if (firstSong(dict) == null) {
            val alt = updateProductPlist(adamId, externalVersionId, ani)
            if (alt != null && firstSong(alt) != null) return alt
        }
        if (versionOnly && fail != null && fail != "0") {
            // Network, expired authentication and other Store failures do not
            // prove that the app is absent from the account.
            error(localizeStoreMessage(msg ?: fail))
        }
        if (!versionOnly && fail != null && fail != "0") {
            if (isTermsDialog(dict)) {
                error("Apple просит подтвердить условия. Подождите минуту и повторите установку.")
            }
            error(localizeStoreMessage(msg ?: fail))
        }
        return dict
    }

    /**
     * Вторая ступень скачивания — DownloadDispatch.
     *
     * Ключи отличаются от volumeStoreDownloadProduct намеренно: здесь
     * `appExtVrsId` вместо `externalVersionId`, числовой `salableAdamId` и
     * `serialNumber`. Со старой формой тела Apple отвечает пустым songList,
     * и на этом легко потерять час.
     */
    private fun updateProductPlist(
        adamId: String,
        externalVersionId: String?,
        ani: AnisetteData,
    ): NSDictionary? {
        val adam = adamId.trim().toLongOrNull() ?: return null
        val fields = mutableMapOf<String, Any?>(
            "creditDisplay" to "",
            "guid" to storeGuid,
            "salableAdamId" to adam,
            "serialNumber" to "0",
        )
        if (!externalVersionId.isNullOrBlank() && externalVersionId != "latest") {
            fields["appExtVrsId"] = externalVersionId
        }
        val body = Plist.dict(*fields.entries.map { it.key to it.value }.toTypedArray())
        return runCatching {
            postStorePlist("$UPDATE_PRODUCT_URL?guid=$storeGuid", body, ani)
        }.getOrNull()
    }

    private fun postStorePlist(startUrl: String, body: NSDictionary, ani: AnisetteData): NSDictionary? {
        val owner = store.activeEmail()
        val originalPod = pod
        val originalCookies = cookies.toMap()
        val bytes = StorePodRouting.execute(
            startUrl,
            send = { url ->
                check(store.activeEmail() == owner) { "Apple ID изменился. Повторите операцию." }
                val req = authorizedRequest(url, body, ani)
                Http.itunesNoFollow.newCall(req).execute().use { resp ->
                    check(store.activeEmail() == owner) { "Apple ID изменился. Повторите операцию." }
                    ingestCookies(resp.headers("Set-Cookie"), resp.headers)
                    StorePodRouting.Reply(
                        resp.code, resp.body?.bytes() ?: ByteArray(0),
                        resp.header("Location"), resp.header("pod") ?: cookies["itspod"],
                    )
                }
            },
            onPod = { pod = it },
        )
        if ((originalPod != pod || originalCookies != cookies) && isReady() && store.activeEmail() == owner) persistStore()
        return runCatching { Plist.parse(bytes) }.getOrElse {
            pw.appbank.signum.data.api.DebugDump.logFail("SignumStore", bytes, it)
            null
        }
    }

    private fun ingestCookies(setCookies: List<String>, headers: okhttp3.Headers) {
        for (line in setCookies) {
            val pair = line.substringBefore(';')
            val idx = pair.indexOf('=')
            if (idx <= 0) continue
            val k = pair.substring(0, idx).trim()
            val v = pair.substring(idx + 1).trim()
            if (v.isEmpty()) cookies.remove(k) else cookies[k] = v
        }
        StorePodRouting.validPod(headers["pod"] ?: cookies["itspod"])?.let { pod = it }
        headers["x-set-apple-store-front"]?.let { storeFront = it }
        headers["x-apple-store-front"]?.let { storeFront = it }
    }

    private fun rememberPod(url: String) {
        StorePodRouting.podFromUrl(url)?.let { pod = it }
        StorePodRouting.validPod(Regex("""[?&]Pod=(\d+)""", RegexOption.IGNORE_CASE).find(url)?.groupValues?.getOrNull(1))?.let { pod = it }
    }

    private fun localizeStoreMessage(raw: String): String {
        val t = raw.lowercase()
        if (isTermsText(t)) {
            return "Apple просит подтвердить условия. Подождите минуту и повторите установку."
        }
        if ("unknown error" in t || "sign in to the itunes" in t || "itunes store" in t) {
            return "App Store просит повторный вход. Откройте «Аккаунт», выйдите и войдите снова, затем повторите установку."
        }
        return raw
    }

    private fun isTermsText(text: String): Boolean {
        val t = text.lowercase()
        return "terms and conditions" in t
            || "terms & conditions" in t
            || "media services" in t
            || "мультимедийн" in t
            || "положение и условия" in t
            || ("условия" in t && ("измен" in t || "приня" in t || "соглаш" in t))
    }

    private fun isTermsDialog(dict: NSDictionary?): Boolean {
        if (dict == null) return false
        val dialog = dict.dict("dialog")
        val blob = listOf(
            dict.str("failureType"),
            dict.str("customerMessage"),
            dict.str("explanation"),
            dialog?.str("message"),
            dialog?.str("explanation"),
            dialog?.str("explanationKey"),
            dialog?.str("kind"),
        ).filterNotNull().joinToString(" ")
        return isTermsText(blob)
    }

    private fun dialogActionUrl(dict: NSDictionary): String? {
        val action = dict.dict("dialog")?.dict("okButtonAction") ?: return null
        return action.str("url") ?: action.str("URL")
    }

    private fun acceptStoreTerms(ani: AnisetteData, source: NSDictionary?): Boolean {
        val token = passwordToken ?: return false
        val dsid = dsPersonId ?: return false
        val body = Plist.dict(
            "guid" to storeGuid,
            "dsPersonId" to dsid,
            "passwordToken" to token,
            "agreeToTerms" to true,
            "hasAgreedToTerms" to true,
        )
        val urls = buildList {
            dialogActionUrl(source ?: NSDictionary())?.let { add(it) }
            add("https://p$pod-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/acceptTermsAndConditions?guid=$storeGuid")
            add("https://buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/acceptTermsAndConditions")
            add("https://p$pod-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/acceptRedownloadTerms?guid=$storeGuid")
            add("https://buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/com.apple.jingle.app.finance.DirectAction/acceptTermsAndConditions")
        }.distinct()
        for (url in urls) {
            val dict = postStorePlist(url, body, ani) ?: continue
            android.util.Log.i("SignumStore", "terms-accept url=${url.take(80)} fail=${dict.str("failureType")}")
            dict.str("passwordToken")?.let { passwordToken = it }
            persistStore()
            if (!isTermsDialog(dict) && (dict.str("failureType").isNullOrBlank() || dict.str("failureType") == "0")) {
                return true
            }
            if (dict.str("passwordToken") != null && !isTermsDialog(dict)) return true
        }
        return false
    }

    private fun isUnknownStoreFailure(fail: String?, msg: String?): Boolean {
        val text = "${fail.orEmpty()} ${msg.orEmpty()}".lowercase()
        return "unknown error" in text
    }

    private fun isItunesSignIn(fail: String?, msg: String?): Boolean {
        val text = "${fail.orEmpty()} ${msg.orEmpty()}".lowercase()
        return "sign in to the itunes" in text || "sign in to the app store" in text
    }

    private fun lookupAdamByBundle(bundleId: String): String? {
        val url = "https://itunes.apple.com/lookup?bundleId=${java.net.URLEncoder.encode(bundleId, "UTF-8")}&entity=software&limit=1"
        val req = Request.Builder().url(url).header("User-Agent", STORE_UA).get().build()
        return Http.itunes.newCall(req).execute().use { resp ->
            val body = resp.body?.string().orEmpty()
            val root = runCatching { SignumJson.parseToJsonElement(body).jsonObject }.getOrNull() ?: return@use null
            val first = root["results"]?.jsonArray?.firstOrNull()?.jsonObject ?: return@use null
            first["trackId"]?.jsonPrimitive?.content?.trim()?.takeIf { it.all(Char::isDigit) }
        }
    }

    private fun extractVersionIds(song: NSDictionary): Pair<List<String>, String?> {
        val meta = song.dict("metadata") ?: song.dict("item-meta-data") ?: song
        val idKeys = listOf(
            "softwareVersionExternalIdentifiers",
            "software-version-external-identifiers",
            "externalVersionIdentifiers",
            "external-version-identifiers",
        )
        val ids = linkedSetOf<String>()
        for (key in idKeys) {
            ids += stringIds(song, key)
            if (meta !== song) ids += stringIds(meta, key)
        }
        val latestKeys = listOf(
            "softwareVersionExternalIdentifier",
            "software-version-external-identifier",
            "externalVersionId",
            "external-id",
        )
        var latest: String? = null
        for (key in latestKeys) {
            val value = numericId(song.objectForKey(key)) ?: numericId(meta.objectForKey(key))
            if (!value.isNullOrBlank()) {
                latest = value
                ids += value
                break
            }
        }
        if (ids.isEmpty()) {
            android.util.Log.d("SignumStore", "no version ids; song keys=${song.keys.joinToString()}")
        }
        return ids.toList() to latest
    }

    private fun marketingVersion(song: NSDictionary): String? {
        val meta = song.dict("metadata") ?: song
        val keys = listOf(
            "bundleShortVersionString",
            "bundle-short-version-string",
            "shortVersionString",
            "displayVersion",
            "versionString",
            "bundleVersion",
        )
        for (key in keys) {
            val raw = song.str(key) ?: meta.str(key) ?: continue
            if (raw.all { it.isDigit() } && raw.length >= 8) continue
            return raw
        }
        return null
    }

    private fun stringIds(dict: NSDictionary, key: String): List<String> {
        val value = dict.objectForKey(key) ?: return emptyList()
        return when (value) {
            is NSArray -> (0 until value.count()).mapNotNull { i -> numericId(value.objectAtIndex(i)) }
            else -> listOfNotNull(numericId(value))
        }
    }

    private fun numericId(value: Any?): String? {
        if (value == null) return null
        val raw = when (value) {
            is NSNumber -> value.longValue().takeIf { it != 0L }?.toString() ?: value.toString()
            is NSString -> value.content.trim()
            else -> value.toString().trim()
        }.trim()
        return raw.takeIf { it.isNotEmpty() && it != "null" && it.any(Char::isDigit) }
    }

    private fun isPasswordTokenFailure(fail: String?, msg: String?): Boolean {
        val text = "${fail.orEmpty()} ${msg.orEmpty()}".lowercase()
        return "password token" in text || "passwordtoken" in text && "expir" in text ||
            fail == "2059" || "sign in" in text && "required" in text
    }

    private fun isLicenseFailure(fail: String?, msg: String?): Boolean {
        val text = "${fail.orEmpty()} ${msg.orEmpty()}".lowercase()
        return "license" in text || "not purchased" in text || "songlist" in text ||
            fail == "9610" || fail == "1002"
    }

    private fun firstSong(dict: NSDictionary): NSDictionary? {
        val list = dict.objectForKey("songList") as? NSArray ?: return null
        if (list.count() == 0) return null
        return list.objectAtIndex(0) as? NSDictionary
    }

    private fun authorizedRequest(url: String, body: NSDictionary, ani: AnisetteData): Request {
        val token = passwordToken ?: error("Сессия Apple ID для App Store устарела. Откройте «Аккаунт», выйдите и войдите снова.")
        val dsid = dsPersonId ?: error("Сессия Apple ID для App Store устарела. Откройте «Аккаунт», выйдите и войдите снова.")
        val payload = Plist.xml(body)
        val sapHeader = runCatching { sap.headerValue(payload) }.getOrNull()
        return Request.Builder()
            .url(url)
            .header("Content-Type", "application/x-apple-plist")
            .header("User-Agent", STORE_UA)
            .header("X-Apple-Store-Front", storeFront)
            .header("X-Dsid", dsid)
            .header("iCloud-DSID", dsid)
            .header("X-Token", token)
            .apply {
                if (!sapHeader.isNullOrBlank()) {
                    header("X-Apple-ActionSignature", sapHeader)
                }
                if (cookies.isNotEmpty()) {
                    header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
                }
                ani.headers().forEach { (k, v) -> header(k, v) }
            }
            .post(payload.toRequestBody("application/x-apple-plist".toMediaType()))
            .build()
    }

    private fun ensureAuth() {
        restoreTokens()
        if (!isReady()) authenticate()
    }

    // --- Минимальная система по каждой сборке ---------------------------------
    //
    // App Store такого не отдаёт, знает только сама сборка. Читаем её из архива
    // на раздаче кусками (см. IpaMinIos) и запоминаем навсегда: для пары
    // «приложение + сборка» значение уже никогда не изменится.

    private val minIosCache = ConcurrentHashMap<String, String>()

    @Volatile
    private var minIosLoaded = false

    private fun minIosKey(adamId: String, ext: String) = "$adamId:$ext"

    private fun minIosFile() = File(store.filesDir(), "min-ios.properties")

    @Synchronized
    private fun loadMinIosCache() {
        if (minIosLoaded) return
        minIosLoaded = true
        val file = minIosFile()
        if (!file.exists()) return
        runCatching {
            val props = java.util.Properties()
            file.inputStream().use { props.load(it) }
            for (name in props.stringPropertyNames()) {
                props.getProperty(name)?.let { minIosCache[name] = it }
            }
        }
    }

    // ponytail: файл переписывается целиком; записей тут сотни, если однажды
    // станут десятки тысяч — дописывать построчно.
    @Synchronized
    private fun rememberMinIos(values: Map<String, String>) {
        if (values.isEmpty()) return
        minIosCache.putAll(values)
        runCatching {
            val props = java.util.Properties()
            for ((k, v) in minIosCache) props.setProperty(k, v)
            minIosFile().outputStream().use { props.store(it, null) }
        }
    }

    /** Что про минимальную систему уже известно, без обращения к сети. */
    fun cachedVersionMinIos(adamId: String, externalIds: List<String>): Map<String, String> {
        val adam = adamId.trim()
        if (adam.isEmpty()) return emptyMap()
        loadMinIosCache()
        return externalIds.mapNotNull { raw ->
            val ext = raw.trim()
            minIosCache[minIosKey(adam, ext)]?.let { ext to it }
        }.toMap()
    }

    /**
     * Ссылка на архив сборки. Просить её надо запросом с покупкой: ответ
     * «только сведения» отдаёт адрес, на который раздача отвечает отказом.
     * Адрес сразу переводим на защищённое соединение — обычное на телефоне
     * запрещено настройками сети.
     */
    private fun versionIpaUrl(adamId: String, ext: String): String? {
        val dispatch = runCatching {
            val body = Plist.dict(
                "creditDisplay" to "", "guid" to storeGuid, "salableAdamId" to adamId.toLong(),
                "serialNumber" to "0", "appExtVrsId" to ext,
            )
            val request = Request.Builder().url("$UPDATE_PRODUCT_URL?guid=$storeGuid")
                .header("User-Agent", STORE_UA)
                .header("iCloud-DSID", dsPersonId.orEmpty()).header("X-Dsid", dsPersonId.orEmpty())
                .header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
                .post(Plist.xml(body).toRequestBody("application/x-apple-plist".toMediaType())).build()
            Http.itunes.newCall(request).execute().use { response ->
                val song = firstSong(Plist.parse(response.body?.bytes() ?: ByteArray(0)))
                val meta = song?.dict("metadata")
                if (meta?.str("itemId") != adamId || meta.str("softwareVersionExternalIdentifier") != ext) null
                else song.str("URL")?.let(::forceHttps)
            }
        }.getOrNull()
        if (!dispatch.isNullOrBlank()) return dispatch
        return runCatching {
            val dict = downloadPlist(adamId, purchase = true, externalVersionId = ext)
            firstSong(dict)?.str("URL")?.trim()?.takeIf { it.isNotEmpty() }?.let { forceHttps(it) }
        }.getOrNull()
    }

    /** Минимальная система одной сборки. Ничего не вышло — вернём null. */
    fun versionMinIos(adamId: String, externalVersionId: String): String? {
        val adam = adamId.trim()
        val ext = externalVersionId.trim()
        if (adam.isEmpty() || ext.isEmpty() || ext == "latest") return null
        loadMinIosCache()
        minIosCache[minIosKey(adam, ext)]?.let { return it }
        ensureAuth()
        val url = versionIpaUrl(adam, ext) ?: return null
        val value = runCatching { IpaMinIos.read(url, STORE_UA) }
            .onFailure { android.util.Log.i("SignumStore", "min-ios $adam:$ext ${it.message}") }
            .getOrNull() ?: return null
        rememberMinIos(mapOf(minIosKey(adam, ext) to value))
        return value
    }

    /**
     * Минимальная система сразу для пачки сборок — для списка версий на экране.
     * Ссылки берём по очереди, потому что запросы в App Store идут через одну
     * общую сессию, а вот чтение архивов уже раскладываем по потокам: там всё
     * время уходит на ожидание сети.
     */
    fun versionMinIosBatch(adamId: String, externalIds: List<String>, onReady: (String, String) -> Unit = { _, _ -> }): Map<String, String> {
        val adam = adamId.trim()
        val known = cachedVersionMinIos(adam, externalIds).toMutableMap()
        known.forEach { (ext, value) -> onReady(ext, value) }
        if (adam.isEmpty()) return known
        val missing = externalIds.map { it.trim() }
            .filter { it.isNotEmpty() && it != "latest" && !known.containsKey(it) }
            .distinct()
        if (missing.isEmpty()) return known
        ensureAuth()
        val urls = missing.mapNotNull { ext -> versionIpaUrl(adam, ext)?.let { ext to it } }
        if (urls.isEmpty()) return known
        val pool = java.util.concurrent.Executors.newFixedThreadPool(
            minOf(MIN_IOS_THREADS, urls.size),
        )
        val fresh = try {
            pool.invokeAll(
                urls.map { (ext, url) ->
                    java.util.concurrent.Callable {
                        val value = runCatching { IpaMinIos.read(url, STORE_UA) }.getOrNull()
                        if (value != null) {
                            rememberMinIos(mapOf(minIosKey(adam, ext) to value))
                            onReady(ext, value)
                        }
                        ext to value
                    }
                },
            ).mapNotNull { future -> runCatching { future.get() }.getOrNull() }
                .mapNotNull { (ext, value) -> value?.let { ext to it } }
        } finally {
            pool.shutdown()
        }
        rememberMinIos(fresh.associate { (ext, value) -> minIosKey(adam, ext) to value })
        known.putAll(fresh)
        return known
    }

    /**
     * Самая свежая сборка, которая запустится на этой системе.
     *
     * Минимальная система с новыми сборками только растёт, поэтому ищем делением
     * пополам: у приложения с пятьюстами сборками хватает девяти проверок вместо
     * пятисот, а каждая проверка — это запрос в App Store и чтение архива.
     */
    fun findVersionForDevice(
        adamId: String,
        deviceIos: String,
        force: Boolean = false,
    ): AppStoreVersionFit? {
        val adam = adamId.trim()
        val ios = deviceIos.trim()
        if (adam.isEmpty() || ios.isEmpty()) return null
        val ids = listVersionIds(adam, force)
        if (ids.isEmpty()) return null
        // Для деления пополам нужен строгий порядок от новых к старым, а список
        // приходит в порядке выпуска с самой свежей впереди. Номер сборки растёт
        // вместе с версией, по нему и упорядочиваем.
        val ordered = ids.map { it.trim() }
            .filter { it.isNotEmpty() }
            .distinct()
            .sortedByDescending { it.toLongOrNull() ?: 0L }
        var low = 0
        var high = ordered.size
        var found: Pair<String, String>? = null
        while (low < high) {
            val mid = (low + high) / 2
            val ext = ordered[mid]
            val min = versionMinIos(adam, ext)
            // ponytail: сборку, которую не удалось прочитать, считаем
            // неподходящей, чтобы поиск не встал; в худшем случае предложим на
            // пару версий старее. Если начнёт мазать — опрашивать соседнюю.
            if (min != null && CatalogResolve.compareIos(ios, min) >= 0) {
                found = ext to min
                high = mid
            } else {
                low = mid + 1
            }
        }
        val hit = found ?: return null
        return AppStoreVersionFit(
            externalVersionId = hit.first,
            version = versionDisplay(adam, hit.first),
            minIos = hit.second,
            isLatest = ids.firstOrNull()?.trim() == hit.first,
        )
    }

    private fun loadOrCreateGuid(): String {
        val existing = store.load().let {
                                
            null
        }
        val file = File(store.filesDir(), "store_guid.txt")
        if (file.exists()) return file.readText().trim()
        val hex = UUID.randomUUID().toString().replace("-", "").take(12).uppercase(Locale.US)
        val formatted = hex.chunked(2).joinToString(".")
        file.writeText(formatted)
        return formatted
    }

    companion object {
        // Сколько сборок читаем одновременно: столько же, сколько на компьютере.
        // Больше не даёт выигрыша — упирается в раздачу Apple, а не в телефон.
        private const val MIN_IOS_THREADS = 4

        // ipatool's exact Store-side signals (pkg/appstore/constants.go).
        private const val CUSTOMER_MESSAGE_BAD_LOGIN = "MZFinance.BadLogin.Configurator_message"
        private const val CUSTOMER_MESSAGE_ACCOUNT_DISABLED = "Your account is disabled."
        private const val FAILURE_INVALID_CREDENTIALS = "-5000"
        private const val TWO_FACTOR_MESSAGE =
            "Введите 6-значный код 2FA с доверенного устройства Apple."

        // Match ipatool's Configurator UA exactly (pkg/http/constants.go); the
        // FairPlay SAP signature is minted in a Configurator/CommerceKit context.
        /**
         * DownloadDispatch. Сумку (bag) Apple отдаёт этот адрес сама; ipatool
         * читает его оттуда и при молчании сумки падает ровно на этот URL,
         * так что константа совпадает с тем, что уже работает на практике.
         */
        private const val UPDATE_PRODUCT_URL =
            "https://downloaddispatch.itunes.apple.com/up/updateProduct"

        private const val CONFIGURATOR_UA =
            "Configurator/2.17 (Macintosh; OS X 15.2; 24C5089c) AppleWebKit/0620.1.16.11.6"
        private const val STORE_UA = CONFIGURATOR_UA
        private const val STORE_SESSION_MESSAGE =
            "Вход в Apple ID уже есть, но App Store не выдал сессию загрузок. Подождите минуту и повторите — это не пароль."
        const val PASSWORD_TOKEN_MESSAGE =
            "Сессия Apple ID для App Store устарела (password token).\n\n" +
                "Откройте «Аккаунт» → выйдите из Apple ID и войдите снова " +
                "(Apple может запросить код 2FA). После этого список версий появится."
    }
}
