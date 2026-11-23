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

    // Данные аккаунта из ответа authenticate (accountInfo.address), как в
    // ipatool Account: имя нужно для iTunesMetadata.plist и `auth info`.
    private var accountAppleId: String? = null
    private var firstName: String = ""
    private var lastName: String = ""

    // Сумка bag.xml: адреса redownload/update/songDownloadDone. Берётся один
    // раз на час; без сети — значения по умолчанию, как у ipatool.
    @Volatile
    private var bagCache: Pair<Long, StoreProtocol.Bag>? = null

    fun isReady(): Boolean = !passwordToken.isNullOrBlank() && !dsPersonId.isNullOrBlank()

    fun clear() {
        passwordToken = null
        dsPersonId = null
        loginEmail = null
        loginPassword = null
        cookies.clear()
        versionIdCache.clear()
        bundleAdamCache.clear()
        accountAppleId = null
        firstName = ""
        lastName = ""
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
    @Synchronized
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
    @Synchronized
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

    private fun runAuthenticate(email: String, password: String, authCode: String?, allowRetry: Boolean = true): AuthOutcome {
        passwordToken = null
        dsPersonId = null
        val code = authCode?.filter { !it.isWhitespace() }.orEmpty()
        // ipatool v2 core, ported: sign in on idmsa.apple.com (JSON), verify the
        // 2FA code through /appleauth/auth, then mint the Store download session
        // from the one-time password ("o") cookie. Every request runs in a fresh
        // cookie session, exactly like ipatool's fresh jar per run.
        val session = ConcurrentHashMap<String, String>()

        // 1) POST /appleauth/auth/signin
        val signinBody = """{"accountNameText":${jsonStr(email)},"passwordText":${jsonStr(password)},"rememberMe":true}"""
        val (signinCode, signinHeaders, signinBytes) = try {
            idmsaRequest(IDMSA_SIGNIN, session, bodyJson = signinBody)
        } catch (e: java.io.IOException) {
            return AuthOutcome.Failed("Не удалось связаться с Apple ID (idmsa.apple.com). Проверьте интернет и повторите.")
        }
        logStoreAuth("pw-idmsa-signin", IDMSA_SIGNIN, signinCode, signinBytes)
        var scnt = signinHeaders["scnt"]?.takeIf { it.isNotBlank() }
        var sessionToken = signinHeaders["X-Apple-ID-Session-Token"]?.takeIf { it.isNotBlank() }
        when (signinCode) {
            200, 204 -> Unit
            409 -> {
                if (scnt.isNullOrBlank() || sessionToken.isNullOrBlank()) return AuthOutcome.Failed(STORE_SESSION_MESSAGE)
                if (code.isBlank()) {
                    // ipatool RequestVerificationCode: the PUT makes Apple push
                    // a fresh code to the trusted devices right away.
                    runCatching {
                        val (pushCode, _, pushBytes) = idmsaRequest(IDMSA_VERIFY_DEVICE, session, method = "PUT", scnt = scnt, token = sessionToken)
                        logStoreAuth("pw-idmsa-push", IDMSA_VERIFY_DEVICE, pushCode, pushBytes)
                    }
                    return AuthOutcome.Needs2fa(TWO_FACTOR_MESSAGE)
                }
                // 2) POST /appleauth/auth/verify/trusteddevice/securitycode
                val codeBody = """{"securityCode":{"code":${jsonStr(code)}}}"""
                val (secCode, secHeaders, secBytes) = idmsaRequest(IDMSA_SECURITY_CODE, session, bodyJson = codeBody, scnt = scnt, token = sessionToken)
                logStoreAuth("pw-idmsa-2fa", IDMSA_SECURITY_CODE, secCode, secBytes)
                if (secCode !in 200..299) {
                    return if (secCode == 400 || secCode == 401)
                        AuthOutcome.Failed("Код 2FA отклонён. Запросите новый код и попробуйте снова.")
                    else
                        AuthOutcome.Failed("Apple отклонила проверку кода 2FA (HTTP $secCode). Повторите позже.")
                }
                scnt = secHeaders["scnt"]?.takeIf { it.isNotBlank() } ?: scnt
                sessionToken = secHeaders["X-Apple-ID-Session-Token"]?.takeIf { it.isNotBlank() } ?: sessionToken
            }
            else -> {
                val err = idmsaErrorText(signinBytes)
                return AuthOutcome.Failed(
                    when {
                        signinCode == 401 || signinCode == 403 ->
                            err?.let { localizeStoreMessage(it) } ?: "Неверный Apple ID или пароль."
                        err != null -> localizeStoreMessage(err)
                        else -> "Apple ID отклонил вход (HTTP $signinCode). Повторите позже."
                    },
                )
            }
        }

        // 3) GET /appleauth/auth — refresh the session; this is where Apple
        // issues the one-time password cookie "o" (ipatool UpdateSession).
        val (refreshCode, refreshHeaders, refreshBytes) = idmsaRequest(IDMSA_SESSION, session, scnt = scnt, token = sessionToken)
        logStoreAuth("pw-idmsa-session", IDMSA_SESSION, refreshCode, refreshBytes)
        if (refreshCode !in 200..299) return AuthOutcome.Failed(STORE_SESSION_MESSAGE)
        scnt = refreshHeaders["scnt"]?.takeIf { it.isNotBlank() } ?: scnt
        sessionToken = refreshHeaders["X-Apple-ID-Session-Token"]?.takeIf { it.isNotBlank() } ?: sessionToken
        val otp = session["o"]
        if (otp.isNullOrBlank()) {
            android.util.Log.w("SignumStore", "idmsa session refresh returned no one-time password cookie")
            return AuthOutcome.Failed(STORE_SESSION_MESSAGE)
        }

        // 4) GET /appleauth/auth/account — dsid and profile name (ipatool
        // parseAccountInformation). Optional: the Store plist also carries DSID.
        var accDsid = ""
        var accEmail: String? = null
        val (accCode, _, accBytes) = idmsaRequest(IDMSA_ACCOUNT, session, scnt = scnt, token = sessionToken)
        logStoreAuth("pw-idmsa-account", IDMSA_ACCOUNT, accCode, accBytes)
        if (accCode in 200..299) {
            runCatching {
                val obj = SignumJson.parseToJsonElement(accBytes.toString(Charsets.UTF_8)).jsonObject
                accDsid = obj["dsid"]?.jsonPrimitive?.contentOrNull.orEmpty()
                accEmail = obj["email"]?.jsonPrimitive?.contentOrNull
                val name = obj["name"]?.jsonObject
                firstName = name?.get("firstName")?.jsonPrimitive?.contentOrNull.orEmpty()
                lastName = name?.get("lastName")?.jsonPrimitive?.contentOrNull.orEmpty()
            }
        }

        // 5) POST MZFinance authenticate with the one-time password — mints the
        // download session (passwordToken + dsPersonId), same as ipatool's
        // appstore.Authenticate.
        val form = okhttp3.FormBody.Builder()
            .add("appleId", email)
            .add("password", otp)
            .add("attempt", "4")
            .add("createSession", "true")
            .build()
        val storeUrl = "$STORE_AUTHENTICATE?guid=$storeGuid"
        val storeReq = Request.Builder()
            .url(storeUrl)
            .header("Accept", "*/*")
            .header("User-Agent", "Config/2.0")
            .header("X-Apple-Store-Front", storeFront)
            .apply {
                if (session.isNotEmpty()) header("Cookie", session.entries.joinToString("; ") { "${it.key}=${it.value}" })
            }
            .post(form)
            .build()
        val (storeCode, storeHeaders, storeBytes) = try {
            Http.itunesNoFollow.newCall(storeReq).execute().use { resp ->
                ingestCookiesInto(session, resp.headers("Set-Cookie"), resp.headers)
                Triple(resp.code, resp.headers, resp.body?.bytes() ?: ByteArray(0))
            }
        } catch (e: java.io.IOException) {
            return AuthOutcome.Failed("Не удалось связаться с сервером App Store. Проверьте интернет и повторите.")
        }
        logStoreAuth("pw-store-auth", storeUrl, storeCode, storeBytes)
        storeHeaders["x-set-apple-store-front"]?.let { storeFront = it }
        storeHeaders["x-apple-store-front"]?.let { storeFront = it }
        if (storeCode == 429) {
            return AuthOutcome.Failed("Apple временно ограничила вход (HTTP 429). Подождите несколько минут и повторите.")
        }
        val dict = parseStoreAuth(storeBytes)
        if (dict == null) {
            if (allowRetry) {
                android.util.Log.w("SignumStore", "store auth produced no parseable reply; retrying once")
                Thread.sleep(2500)
                return runAuthenticate(email, password, authCode, allowRetry = false)
            }
            return AuthOutcome.Failed(STORE_SESSION_MESSAGE)
        }
        dict.str("storeFront")?.let { storeFront = it }
        val failure = dict.str("failureType").orEmpty()
        val msg = dict.str("customerMessage").orEmpty()
        passwordToken = dict.str("passwordToken")
        dsPersonId = dict.str("dsPersonId") ?: dict.dict("accountInfo")?.str("dsPersonId")
            ?: storeHeaders["x-dsid"]?.trim()?.takeIf { it.isNotEmpty() && it.all(Char::isDigit) }
            ?: accDsid.takeIf { it.isNotEmpty() && it.all(Char::isDigit) }
        if (!passwordToken.isNullOrBlank() && !dsPersonId.isNullOrBlank()) {
            val info = dict.dict("accountInfo")
            val address = info?.dict("address")
            accountAppleId = info?.str("appleId") ?: accEmail ?: email
            if (firstName.isBlank()) firstName = address?.str("firstName").orEmpty()
            if (lastName.isBlank()) lastName = address?.str("lastName").orEmpty()
            StorePodRouting.validPod(storeHeaders["pod"])?.let { pod = it }
            // This fresh session is the live download session: promote it
            // into the client-wide jar.
            cookies.putAll(session)
            return AuthOutcome.Success
        }
        if (msg == CUSTOMER_MESSAGE_ACCOUNT_DISABLED) {
            return AuthOutcome.Failed("Аккаунт Apple ID отключён.")
        }
        if (failure.isNotBlank() && failure != "0") {
            return AuthOutcome.Failed(
                if (msg.isNotBlank()) localizeStoreMessage(msg) else "Apple отклонила вход (failureType $failure). Повторите позже.",
            )
        }
        return AuthOutcome.Failed(
            if (msg.isNotBlank() && !msg.contains("sign in", true)) msg else STORE_SESSION_MESSAGE,
        )
    }

    /** Minimal JSON request against idmsa.apple.com with a fresh cookie session. */
    private fun idmsaRequest(
        url: String,
        session: ConcurrentHashMap<String, String>,
        bodyJson: String? = null,
        method: String = if (bodyJson != null) "POST" else "GET",
        scnt: String? = null,
        token: String? = null,
    ): Triple<Int, okhttp3.Headers, ByteArray> {
        val b = Request.Builder()
            .url(url)
            .header("Accept", "application/json")
            .header("X-Apple-Widget-Key", IDMSA_SERVICE_KEY)
            .header("User-Agent", STORE_UA)
        when (method.uppercase(Locale.US)) {
            "POST" -> b.post((bodyJson ?: "").toRequestBody("application/json".toMediaType()))
            "PUT" -> b.put((bodyJson ?: "").toRequestBody("application/json".toMediaType()))
            "GET" -> b.get()
            else -> b.method(method, bodyJson?.toRequestBody("application/json".toMediaType()))
        }
        if (!scnt.isNullOrBlank()) b.header("scnt", scnt)
        if (!token.isNullOrBlank()) b.header("X-Apple-ID-Session-Token", token)
        if (session.isNotEmpty()) b.header("Cookie", session.entries.joinToString("; ") { "${it.key}=${it.value}" })
        return Http.itunesNoFollow.newCall(b.build()).execute().use { resp ->
            ingestCookiesInto(session, resp.headers("Set-Cookie"), resp.headers)
            Triple(resp.code, resp.headers, resp.body?.bytes() ?: ByteArray(0))
        }
    }

    /** Extract a human-readable error from an idmsa JSON error body. */
    private fun idmsaErrorText(bytes: ByteArray): String? {
        val obj = runCatching {
            SignumJson.parseToJsonElement(bytes.toString(Charsets.UTF_8)).jsonObject
        }.getOrNull() ?: return null
        obj["serviceErrors"]?.jsonArray?.let { arr ->
            for (item in arr) {
                val o = item.jsonObject
                o["message"]?.jsonPrimitive?.contentOrNull?.let { if (it.isNotBlank()) return it }
            }
        }
        for (key in listOf("authenticationErrorMessage", "validationErrorMessage", "errorMessage", "message")) {
            obj[key]?.jsonPrimitive?.contentOrNull?.let { if (it.isNotBlank()) return it }
        }
        return null
    }

    private fun jsonStr(s: String): String = buildString {
        append('"')
        for (c in s) when (c) {
            '"' -> append("\\\"")
            '\\' -> append("\\\\")
            '\n' -> append("\\n")
            '\r' -> append("\\r")
            '\t' -> append("\\t")
            else -> if (c < ' ') append("\\u%04x".format(c.code)) else append(c)
        }
        append('"')
    }

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
            accountAppleId = dict.str("accountAppleId")
            firstName = dict.str("firstName").orEmpty()
            lastName = dict.str("lastName").orEmpty()
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
            "accountAppleId" to accountAppleId,
            "firstName" to firstName,
            "lastName" to lastName,
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

    /** Сведения об аккаунте — `ipatool auth info`. */
    data class AccountInfo(
        val email: String,
        val name: String,
        val dsid: String,
        val storeFront: String,
        /** Код страны витрины («RU»); если витрина неизвестна — сама витрина. */
        val country: String,
        val pod: String,
    )

    @Synchronized
    fun accountInfo(): AccountInfo? {
        restoreTokens()
        if (!isReady()) return null
        val email = accountAppleId ?: loginEmail ?: store.activeEmail() ?: return null
        return AccountInfo(
            email = email,
            name = "$firstName $lastName".trim(),
            dsid = dsPersonId.orEmpty(),
            storeFront = storeFront,
            country = StoreProtocol.countryCodeFromStorefront(storeFront) ?: storeFront,
            pod = pod,
        )
    }

    /**
     * `ipatool auth revoke`: забыть сессию App Store активного аккаунта
     * (токен, куки, pod). Пароль и сам аккаунт в списке не трогаем — это
     * делает SignumEngine.logout().
     */
    @Synchronized
    fun revoke() {
        val email = store.activeEmail()
        clear()
        email?.let { store.deleteSessionBlob("store_$it") }
    }

    /**
     * `ipatool search`: iTunes Search по витрине вошедшего аккаунта
     * (entity=software,iPadSoftware, media=software), limit как у ipatool.
     */
    fun searchAccount(term: String, limit: Int = 5): List<AppStoreSearchHit> {
        val q = term.trim()
        if (q.isEmpty()) return emptyList()
        restoreTokens()
        val cc = StoreProtocol.countryCodeFromStorefront(storeFront)
            ?: error("Не удалось определить страну витрины Apple ID ($storeFront).")
        val req = Request.Builder().url(StoreProtocol.searchUrl(q, cc, limit.coerceIn(1, 200)))
            .header("User-Agent", STORE_UA).get().build()
        Http.itunes.newCall(req).execute().use { resp ->
            if (resp.code != 200) error("Поиск App Store не удался (HTTP ${resp.code}).")
            val root = SignumJson.parseToJsonElement(resp.body?.string().orEmpty()).jsonObject
            return root["results"]?.jsonArray.orEmpty().mapNotNull { el -> hitFromJson(el.jsonObject) }
        }
    }

    private fun hitFromJson(o: kotlinx.serialization.json.JsonObject): AppStoreSearchHit? {
        val adam = o["trackId"]?.jsonPrimitive?.content ?: return null
        return AppStoreSearchHit(
            adamId = adam,
            name = o["trackName"]?.jsonPrimitive?.content ?: "App",
            artist = o["artistName"]?.jsonPrimitive?.content,
            artistId = o["artistId"]?.jsonPrimitive?.content,
            artwork = o["artworkUrl100"]?.jsonPrimitive?.content ?: o["artworkUrl60"]?.jsonPrimitive?.content,
            bundleId = o["bundleId"]?.jsonPrimitive?.content,
            version = o["version"]?.jsonPrimitive?.content,
            minIos = o["minimumOsVersion"]?.jsonPrimitive?.content,
            price = o["formattedPrice"]?.jsonPrimitive?.content,
        )
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
     * История покупок «без кода» — как `ipatool list-purchases` (ipatool-cpp).
     *
     * GET p<pod>-buy.itunes.apple.com/commerce/account/purchases — тот же
     * commerce-API, что у окна «История покупок» в iTunes. Авторизует его уже
     * сохранённая сессия App Store (cookies + X-Dsid + X-Token), поэтому
     * reportaproblem и код 2FA не нужны. Apple отдаёт так не всё (скрытые и
     * семейные покупки, часть старых), поэтому интерфейс помечает список как
     * неполный. Окна по годам — у эндпоинта нет диапазона «всё сразу».
     */
    @Synchronized
    fun listPurchasesViaLibrary(): List<pw.appbank.signum.data.models.PurchaseHistoryApp> {
        restoreTokens()
        if (!isReady()) {
            error("Нет входа в App Store. Войдите в Apple ID на этой странице или откройте историю с кодом.")
        }
        val dsid = dsPersonId.orEmpty()
        val seenItems = LinkedHashMap<String, pw.appbank.signum.data.models.PurchaseHistoryApp>()
        val thisYear = java.time.Year.now().value
        var anyOk = false
        for (year in thisYear downTo 2008) {
            val seenOrders = HashSet<String>()
            for (page in 1..200) {
                val (code, body) = getPurchasesPage(dsid, page, "$year-all")
                if (code == 401 || code == 403) {
                    error("App Store не принял сохранённую сессию. Откройте историю с кодом или войдите в Apple ID заново.")
                }
                if (code != 200) {
                    Log.w("SignumStore", "list-purchases $year p$page HTTP $code")
                    break
                }
                val attr = runCatching {
                    SignumJson.parseToJsonElement(body).jsonObject["data"]?.jsonObject?.get("attributes")?.jsonObject
                }.getOrNull() ?: break
                anyOk = true
                var added = 0
                for (inv in attr["purchases"]?.jsonArray.orEmpty()) {
                    val invoice = inv.jsonObject
                    val orderId = invoice.str("order-id")
                    if (!orderId.isNullOrBlank() && !seenOrders.add(orderId)) continue
                    added++
                    val invDate = invoice.str("invoice-date")
                    for (it in invoice["items"]?.jsonArray.orEmpty()) {
                        val item = it.jsonObject
                        val id = item.str("item-id")?.trim()?.takeIf { v -> v.all(Char::isDigit) && v.isNotEmpty() } ?: continue
                        val kind = item.str("kind").orEmpty().lowercase()
                        if (kind.isNotEmpty() && (!kind.contains("app") || kind.contains("in-app"))) continue
                        if (seenItems.containsKey(id)) continue
                        seenItems[id] = pw.appbank.signum.data.models.PurchaseHistoryApp(
                            adamId = id,
                            name = item.str("item-name") ?: item.str("item-title") ?: "Приложение",
                            price = item.str("price"),
                            date = item.str("purchase-date") ?: invDate,
                        )
                    }
                }
                val complete = attr.str("is-complete")?.toBooleanStrictOrNull() ?: true
                if (complete || added == 0) break
            }
        }
        if (!anyOk) error("Через библиотеку историю получить не удалось. Откройте историю с кодом подтверждения.")
        return enrichFromItunes(seenItems.values.toList())
    }

    /** Одна страница commerce-истории; 302 (pod / cookie handshake) идём по Location как есть. */
    private fun getPurchasesPage(dsid: String, page: Int, range: String): Pair<Int, String> {
        var url = "https://p$pod-buy.itunes.apple.com/commerce/account/purchases" +
            "?isDeepLink=false&isJsonApiFormat=true&page=$page&range=${java.net.URLEncoder.encode(range, "UTF-8")}"
        repeat(5) {
            val req = Request.Builder()
                .url(url)
                .header("User-Agent", STORE_UA)
                .header("Accept-Language", "en-us")
                .header("X-Apple-Store-Front", storeFront)
                .header("X-Dsid", dsid)
                .apply {
                    passwordToken?.takeIf { it.isNotBlank() }?.let { header("X-Token", it) }
                    if (cookies.isNotEmpty()) header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
                }
                .get()
                .build()
            Http.apple.newCall(req).execute().use { resp ->
                ingestCookies(resp.headers("Set-Cookie"), resp.headers)
                if (resp.code in 301..303 || resp.code == 307) {
                    val loc = resp.header("Location").orEmpty()
                    if (loc.isBlank()) return resp.code to ""
                    url = StorePodRouting.redirectUrl(url, loc)
                    rememberPod(url)
                    return@repeat
                }
                return resp.code to resp.body?.string().orEmpty()
            }
        }
        return 310 to ""
    }

    /** Значки, разработчик и имя — пачками через публичный iTunes lookup. */
    private fun enrichFromItunes(
        apps: List<pw.appbank.signum.data.models.PurchaseHistoryApp>,
    ): List<pw.appbank.signum.data.models.PurchaseHistoryApp> {
        val found = HashMap<String, kotlinx.serialization.json.JsonObject>()
        for (chunk in apps.map { it.adamId }.chunked(150)) {
            for (country in listOf("ru", "us")) {
                val missing = chunk.filterNot(found::containsKey)
                if (missing.isEmpty()) break
                val url = "https://itunes.apple.com/lookup?id=${missing.joinToString(",")}&entity=software&country=$country"
                runCatching {
                    Http.itunes.newCall(Request.Builder().url(url).get().build()).execute().use { resp ->
                        val root = SignumJson.parseToJsonElement(resp.body?.string().orEmpty()).jsonObject
                        for (el in root["results"]?.jsonArray.orEmpty()) {
                            val o = el.jsonObject
                            o.str("trackId")?.let { found[it] = o }
                        }
                    }
                }
            }
        }
        return apps.map { app ->
            val o = found[app.adamId] ?: return@map app
            app.copy(
                name = o.str("trackName") ?: app.name,
                artist = o.str("artistName") ?: app.artist,
                artwork = o.str("artworkUrl100") ?: o.str("artworkUrl60") ?: app.artwork,
                bundleId = o.str("bundleId") ?: app.bundleId,
                price = o.str("formattedPrice") ?: app.price,
            )
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

    @Synchronized
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

    @Synchronized
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

    /**
     * `ipatool get-version-metadata`: маркетинговая версия, дата выпуска и
     * минимальная iOS одной сборки. Минимальная iOS читается из Info.plist
     * архива частичными запросами (только для настоящего .ipa) и кэшируется.
     */
    @Synchronized
    fun versionMetadata(adamId: String, externalVersionId: String): StoreProtocol.VersionMetadata {
        val adam = adamId.trim()
        val ext = externalVersionId.trim()
        require(adam.isNotEmpty() && ext.isNotEmpty() && ext != "latest") { "Нужны номер приложения и номер сборки." }
        ensureAuth()
        val dict = downloadPlist(adam, purchase = false, externalVersionId = ext)
        val song = firstSong(dict) ?: throw StoreProtocol.LicenseRequiredException()
        val meta = StoreProtocol.versionMetadataFromItem(song, ext)
        if (meta.displayVersion.isNotEmpty()) {
            val owner = store.activeEmail().orEmpty()
            versionLabelCache["$owner:$adam:$ext"] = meta.displayVersion
            store.storeHistoryCache("version_label_$owner:$adam:$ext", meta.displayVersion)
        }
        val minIos = runCatching { versionMinIos(adam, ext) }.getOrNull()
        return meta.copy(minimumOSVersion = minIos, supportedPlatforms = if (minIos != null) listOf("iOS") else emptyList())
    }

    fun ownsListing(appStoreId: String?, bundleId: String?, force: Boolean = false): Boolean {
        val adam = resolveAdamId(appStoreId, bundleId) ?: return false
        return listVersionIds(adam, force).isNotEmpty()
    }

    @Synchronized
    fun purchaseAndDownload(
        adamId: String,
        dest: File,
        externalVersionId: String? = null,
        alreadyOwned: Boolean = false,
        onProgress: (Float, String) -> Unit,
    ): File {
        ensureAuth()
        onProgress(8f, "Покупка в App Store…")
        var bought = false
        if (!alreadyOwned) {
            try {
                buy(adamId)
                bought = true
            } catch (error: Exception) {
                if (error is pw.appbank.signum.data.jobs.JobCancelledException) throw error
                val message = error.message.orEmpty().lowercase()
                val alreadyOwnedResponse = error is StoreProtocol.AlreadyPurchasedException ||
                    "already" in message ||
                    "уже" in message && ("куп" in message || "приобр" in message) ||
                    "purchased" in message
                // Платное приложение могло быть куплено раньше: пробуем скачать,
                // а если лицензии нет — сообщим об этом ниже.
                if (!alreadyOwnedResponse && error !is StoreProtocol.PaidAppNotSupportedException) throw error
                Log.i("SignumStore", "buy skipped: ${error.javaClass.simpleName}")
            }
        }
        onProgress(22f, "Запрашиваем IPA…")
        val plist = try {
            downloadPlist(adamId, purchase = true, externalVersionId = externalVersionId, acceptTerms = true)
        } catch (e: StoreProtocol.LicenseRequiredException) {
            // download --purchase: лицензии нет — получаем её и повторяем один раз.
            if (bought) throw e
            onProgress(15f, "Покупка в App Store…")
            val purchase = buy(adamId)
            // Apple ставит выдачу в очередь асинхронно — ждём, если ссылок в ответе нет.
            if (StoreProtocol.songList(purchase).isEmpty()) Thread.sleep(3000)
            onProgress(22f, "Запрашиваем IPA…")
            downloadPlist(adamId, purchase = true, externalVersionId = externalVersionId, acceptTerms = true)
        }
        val song = firstSong(plist) ?: error("App Store не вернул ссылку на приложение.")
        val url = song.str("URL") ?: error("App Store не вернул ссылку на приложение.")
        onProgress(35f, "Скачиваем IPA…")
        val refresh = {
            val again = downloadPlist(adamId, purchase = true, externalVersionId = externalVersionId)
            firstSong(again)?.str("URL")
        }
        downloadIpaFile(url, dest, onProgress, refresh)
        if (dest.length() < 1000) error("Скачанный файл повреждён")
        verifyMd5(song, dest, onProgress, url, refresh)
        // Отчёт о завершённой загрузке — как iTunes; не влияет на результат.
        runCatching { reportDownloadDone(song, adamId) }
            .onFailure { Log.i("SignumStore", "songDownloadDone: ${it.message}") }
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

    /**
     * Проверка md5 из ответа Store, как в ipatool: при несовпадении файл
     * удаляется и скачивается заново один раз; снова не сошлось — ошибка.
     */
    private fun verifyMd5(
        song: NSDictionary,
        dest: File,
        onProgress: (Float, String) -> Unit,
        url: String,
        refresh: () -> String?,
    ) {
        val expected = song.str("md5")?.lowercase() ?: return
        onProgress(65f, "Проверяем целостность IPA…")
        if (StoreProtocol.md5Hex(dest).equals(expected, ignoreCase = true)) return
        Log.w("SignumStore", "md5 mismatch, re-downloading once")
        dest.delete()
        onProgress(35f, "Файл повреждён при загрузке, скачиваем заново…")
        downloadIpaFile(url, dest, onProgress, refresh)
        val actual = StoreProtocol.md5Hex(dest)
        if (!actual.equals(expected, ignoreCase = true)) {
            dest.delete()
            error("IPA повреждён: контрольная сумма не совпала и после повторной загрузки (md5 $expected, получено $actual).")
        }
    }

    /** songDownloadDone: GET с pod аккаунта, songId и guid (report_download_done). */
    private fun reportDownloadDone(song: NSDictionary, adamId: String): Boolean {
        val songId = song.str("songId")?.toLongOrNull()?.takeIf { it != 0L } ?: adamId.toLong()
        val url = StoreProtocol.songDownloadDoneUrl(
            bag().songDownloadDoneEndpoint, pod, songId, song.str("download-id").orEmpty(), storeGuid,
        )
        val dsid = dsPersonId.orEmpty()
        val req = Request.Builder().url(url)
            .header("User-Agent", STORE_UA)
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US")
            .header("iCloud-DSID", dsid)
            .header("X-Dsid", dsid)
            .header("X-Apple-Store-Front", storeFront)
            .apply {
                if (cookies.isNotEmpty()) header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
            }
            .get().build()
        return Http.itunes.newCall(req).execute().use { resp ->
            resp.code == 200 && resp.body?.string().orEmpty().contains("<string>success</string>")
        }
    }

    /**
     * Достаёт из ответа `sinfs` и `metadata` и кладёт их внутрь IPA.
     * iTunesMetadata.plist дополняется как у iTunes/ipatool: Apple ID, имя,
     * downloadInfo, дата загрузки, is-purchased-redownload, storeCohort.
     */
    private fun injectLicense(dest: File, song: NSDictionary) {
        val sinfs = StoreProtocol.sinfs(song)
        val source = song.objectForKey("metadata") as? NSDictionary
        val metadata = source?.let { meta ->
            runCatching {
                Plist.xml(
                    StoreProtocol.patchedMetadata(
                        meta,
                        appleId = accountAppleId ?: loginEmail ?: store.activeEmail().orEmpty(),
                        firstName = firstName,
                        lastName = lastName,
                        storeFront = storeFront,
                        dsid = dsPersonId.orEmpty(),
                        nowMillis = System.currentTimeMillis(),
                    ),
                )
            }.getOrElse { runCatching { Plist.xml(meta) }.getOrNull() }
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
                // Some CDN edges ignore Range and return a complete 200 body.
                // Appending that body to the partial file would corrupt the IPA
                // and make the retry loop believe the file is still incomplete.
                val append = have > 0 && (code == 206 || range != null)
                val effectiveHave = if (have > 0 && !append) {
                    dest.delete()
                    0L
                } else {
                    have
                }
                val effectiveExpected = if (effectiveHave == 0L && !append && bodyLen > 0) {
                    bodyLen
                } else {
                    expected
                }
                java.io.FileOutputStream(dest, append).use { out ->
                    conn.inputStream.use { input ->
                        val buf = ByteArray(64 * 1024)
                        var read = if (append) effectiveHave else 0L
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
                            val totalMb = if (effectiveExpected > 0) "/${effectiveExpected / 1024 / 1024}" else ""
                            val pct = if (effectiveExpected > 0) {
                                35f + (read.toFloat() / effectiveExpected.toFloat()) * 34f
                            } else {
                                50f
                            }
                            onProgress(pct.coerceIn(35f, 69f), "Скачиваем IPA… $mb${totalMb} МБ")
                            if (effectiveExpected > 0 && read >= effectiveExpected) break
                        }
                    }
                }
                return effectiveExpected
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

    /**
     * Покупка бесплатного приложения — `ipatool purchase` (AppStore::purchase).
     *
     * Платное приложение не покупаем (PaidAppNotSupported). buyProduct идёт на
     * адрес с pod аккаунта; при 2059 «временно недоступно» — повтор с
     * pricingParameters=GAME (Arcade). Отказ сессии — один тихий повторный вход.
     * Диалог условий Apple принимаем и повторяем покупку (дополнение Signum).
     */
    private fun buy(adamId: String): NSDictionary {
        val adam = adamId.trim().toLongOrNull() ?: error("Неверный номер приложения: $adamId")
        val price = runCatching { lookupPrice(adam) }.getOrNull()
        if (price != null && price > 0.0) throw StoreProtocol.PaidAppNotSupportedException()
        return withSilentRelogin {
            try {
                purchaseOnce(adam, StoreProtocol.PRICING_APPSTORE)
            } catch (e: StoreProtocol.TemporarilyUnavailableException) {
                purchaseOnce(adam, StoreProtocol.PRICING_ARCADE)
            }
        }
    }

    private fun purchaseOnce(adam: Long, pricing: String): NSDictionary {
        val body = StoreProtocol.purchasePayload(storeGuid, adam, pricing)
        val ani = anisette.getData(gs)
        var (status, dict) = postStore(StoreProtocol.purchaseUrl(pod), body, ani)
        if (dict != null && isTermsDialog(dict) && acceptStoreTerms(ani, dict)) {
            val again = postStore(StoreProtocol.purchaseUrl(pod), body, ani)
            status = again.first
            dict = again.second
        }
        val d = dict ?: NSDictionary()
        if (isTermsDialog(d)) error("Apple просит подтвердить условия. Подождите минуту и повторите установку.")
        return StoreProtocol.checkPurchase(status, d)
    }

    /** Цена из iTunes lookup на витрине аккаунта; null — не удалось узнать. */
    private fun lookupPrice(adam: Long): Double? {
        val cc = StoreProtocol.countryCodeFromStorefront(storeFront) ?: return null
        val req = Request.Builder().url(StoreProtocol.lookupUrl(cc, adamId = adam.toString()))
            .header("User-Agent", STORE_UA).get().build()
        return Http.itunes.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) return@use null
            val root = SignumJson.parseToJsonElement(resp.body?.string().orEmpty()).jsonObject
            root["results"]?.jsonArray?.firstOrNull()?.jsonObject?.get("price")?.jsonPrimitive?.contentOrNull?.toDoubleOrNull()
        }
    }

    /**
     * Один тихий повторный вход по сохранённому паролю при отказе сессии
     * (silent_relogin в ipatool): 2034 / 2042 / 1008 / «Sign In to the iTunes Store».
     */
    private fun <T> withSilentRelogin(block: () -> T): T = try {
        block()
    } catch (e: StoreProtocol.PasswordTokenExpiredException) {
        Log.i("SignumStore", "session expired, silent re-login")
        passwordToken = null
        cookies.clear()
        authenticate(force = true)
        block()
    }

    private fun downloadPlist(
        adamId: String,
        purchase: Boolean,
        externalVersionId: String?,
        versionOnly: Boolean = false,
        acceptTerms: Boolean = false,
    ): NSDictionary {
        val ext = externalVersionId?.trim()?.takeIf { it.isNotEmpty() && it != "latest" }
        return try {
            withSilentRelogin {
                resolveDownload(adamId, ext, needSinfs = purchase && !versionOnly, acceptTerms = acceptTerms)
            }
        } catch (e: StoreProtocol.LicenseRequiredException) {
            // Список версий: «не куплено» — это пустой ответ, а не ошибка.
            if (versionOnly) NSDictionary() else throw e
        } catch (e: StoreProtocol.PasswordTokenExpiredException) {
            if (versionOnly) error(PASSWORD_TOKEN_MESSAGE) else throw e
        }
    }

    /**
     * Каскад скачивания ipatool 1.3.2 (resolve_download), без локального
     * состояния библиотеки:
     *  1) volumeStoreDownloadProduct (с passwordToken: kbsync на Android не
     *     вычислить — storeagent здесь не эмулируется), с повтором 2042
     *     hasBeenAuthedForBuy;
     *  2) redownloadProduct с версией, закреплённой через mdm-lockup;
     *  3) updateProduct со строгой проверкой ответа.
     * Ответ «обслужен», если есть элемент (а для скачивания — и sinf).
     * Ничего не отдало и лицензии нет — LicenseRequiredException.
     */
    private fun resolveDownload(
        adamId: String,
        externalVersionId: String?,
        needSinfs: Boolean,
        acceptTerms: Boolean,
    ): NSDictionary {
        val adam = adamId.trim().toLongOrNull() ?: error("Неверный номер приложения: $adamId")
        val ani = anisette.getData(gs)

        // ── 1) volumeStoreDownloadProduct ────────────────────────────────────
        var vs = volumeStoreDownload(adamId, externalVersionId, needSinfs, authed = false, ani = ani)
        if (acceptTerms && isTermsDialog(vs) && acceptStoreTerms(ani, vs)) {
            vs = volumeStoreDownload(adamId, externalVersionId, needSinfs, authed = false, ani = ani)
        }
        if (StoreProtocol.served(vs, needSinfs)) return vs
        var vsFailure = StoreProtocol.failureOf(vs)
        if (StoreProtocol.isAuthorizationDialog(vs)) {
            Log.i("SignumStore", "volumeStoreDownload: auth dialog, retry hasBeenAuthedForBuy")
            vs = volumeStoreDownload(adamId, externalVersionId, needSinfs, authed = true, ani = ani)
            if (StoreProtocol.served(vs, needSinfs)) return vs
            vsFailure = StoreProtocol.failureOf(vs)
        }
        val vsMessage = StoreProtocol.scalar(vs, "customerMessage")
        if (StoreProtocol.isSessionFailure(vs) || isPasswordTokenFailure(vsFailure, vsMessage) ||
            isUnknownStoreFailure(vsFailure, vsMessage) || isItunesSignIn(vsFailure, vsMessage)
        ) {
            throw StoreProtocol.PasswordTokenExpiredException()
        }
        if (isTermsDialog(vs)) error("Apple просит подтвердить условия. Подождите минуту и повторите установку.")
        var customerMessage = vsMessage

        // Закреплённая версия для redownload и updateProduct (пусто — без закрепления).
        val pin = externalVersionId ?: runCatching { lookupLatestExternalVersionId(adam) }
            .onFailure { Log.i("SignumStore", "platform lookup: ${it.message} — unpinned redownload") }
            .getOrNull().orEmpty()
        val bag = bag()

        // ── 2) redownloadProduct, затем 3) updateProduct ─────────────────────
        val (rdStatus, rdRaw) = postDownloadFamily("${bag.redownloadEndpoint}?guid=$storeGuid", adam, pin)
        val rd = rdRaw ?: NSDictionary()
        if (StoreProtocol.served(rd, needSinfs)) return rd
        val rdUnavailable = rdStatus != 500 && StoreProtocol.isRedownloadUnavailable(rd)
        StoreProtocol.scalar(rd, "customerMessage").takeIf { it.isNotEmpty() }?.let { customerMessage = it }
        val wantUpdate = (rdStatus == 500 && rd.count() == 0) ||
            StoreProtocol.isUnavailableResponse(rdStatus, rd) ||
            rdUnavailable ||
            (needSinfs && rdStatus == 200 && StoreProtocol.scalar(rd, "failureType").isEmpty() && !StoreProtocol.hasSinfs(rd)) ||
            (!needSinfs && StoreProtocol.songList(rd).isEmpty())
        if (wantUpdate && pin.isNotEmpty()) {
            try {
                val (upStatus, upRaw) = postDownloadFamily("${bag.updateEndpoint}?guid=$storeGuid", adam, pin)
                val up = StoreProtocol.checkUpdateResponse(upStatus, upRaw ?: NSDictionary(), adam, pin, null)
                if (StoreProtocol.served(up, needSinfs)) return up
                StoreProtocol.scalar(up, "customerMessage").takeIf { it.isNotEmpty() }?.let { customerMessage = it }
                if (rdUnavailable) throw StoreProtocol.LicenseRequiredException()
            } catch (e: StoreProtocol.LicenseRequiredException) {
                throw e
            } catch (e: StoreProtocol.PasswordTokenExpiredException) {
                throw e
            } catch (e: StoreProtocol.StoreException) {
                if (rdUnavailable) throw StoreProtocol.LicenseRequiredException()
                if (customerMessage.isEmpty()) customerMessage = e.message.orEmpty()
            } catch (e: java.io.IOException) {
                if (rdUnavailable) throw StoreProtocol.LicenseRequiredException()
                if (customerMessage.isEmpty()) customerMessage = e.message.orEmpty()
            }
        } else if (rdUnavailable) {
            throw StoreProtocol.LicenseRequiredException()
        }
        // Лицензионные отказы старого ответа (1002 и т. п.) — тоже «не куплено».
        if (vsFailure != StoreProtocol.FAILURE_LICENSE_NOT_FOUND && isLicenseFailure(vsFailure, vsMessage) &&
            StoreProtocol.songList(rd).isEmpty()
        ) {
            throw StoreProtocol.LicenseRequiredException()
        }
        val failure = StoreProtocol.cascadeFailure(vsFailure, customerMessage, vs)
        if (failure is StoreProtocol.LicenseRequiredException) throw failure
        throw StoreProtocol.StoreException(localizeStoreMessage(failure.message.orEmpty()))
    }

    /** Шаг 1 каскада: volumeStoreDownloadProduct на адресе pod аккаунта. */
    private fun volumeStoreDownload(
        adamId: String,
        externalVersionId: String?,
        purchase: Boolean,
        authed: Boolean,
        ani: AnisetteData,
    ): NSDictionary {
        val fields = mutableMapOf<String, Any?>(
            "creditDisplay" to "",
            "guid" to storeGuid,
            "salableAdamId" to adamId,
            "passwordToken" to passwordToken,
            "dsPersonId" to dsPersonId,
        )
        if (purchase) fields["ageCheck"] = true
        if (!externalVersionId.isNullOrBlank()) fields["externalVersionId"] = externalVersionId
        // 2042 AuthTokenResumeFreeBuy: «Загрузить» в диалоге подтверждения.
        if (authed) fields["hasBeenAuthedForBuy"] = "true"
        val body = Plist.dict(*fields.entries.map { it.key to it.value }.toTypedArray())
        val url = "https://p$pod-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/volumeStoreDownloadProduct?guid=$storeGuid"
        return postStore(url, body, ani).second ?: NSDictionary()
    }

    /**
     * redownloadProduct / updateProduct: тело download_request_payload и
     * заголовки ipatool (Content-Type, iCloud-DSID, X-Dsid) плюс куки сессии.
     */
    private fun postDownloadFamily(startUrl: String, adam: Long, pin: String): Pair<Int, NSDictionary?> {
        val dsid = dsPersonId ?: throw StoreProtocol.PasswordTokenExpiredException()
        val payload = Plist.xml(StoreProtocol.downloadRequestPayload(storeGuid, adam, pin))
        val owner = store.activeEmail()
        var status = 0
        val bytes = StorePodRouting.execute(
            startUrl,
            send = { url ->
                check(store.activeEmail() == owner) { "Apple ID изменился. Повторите операцию." }
                val req = Request.Builder().url(url)
                    .header("Content-Type", "application/x-apple-plist")
                    .header("User-Agent", STORE_UA)
                    .header("iCloud-DSID", dsid)
                    .header("X-Dsid", dsid)
                    .apply {
                        if (cookies.isNotEmpty()) header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
                    }
                    .post(payload.toRequestBody("application/x-apple-plist".toMediaType()))
                    .build()
                Http.itunesNoFollow.newCall(req).execute().use { resp ->
                    ingestCookies(resp.headers("Set-Cookie"), resp.headers)
                    status = resp.code
                    StorePodRouting.Reply(resp.code, resp.body?.bytes() ?: ByteArray(0), resp.header("Location"), null)
                }
            },
            onPod = { },
        )
        // decode_plist_safe: пустое или не-plist тело — пустой словарь.
        val dict = if (bytes.toString(Charsets.UTF_8).isBlank()) NSDictionary()
        else runCatching { Plist.parse(bytes) }.getOrElse { NSDictionary() }
        return status to dict
    }

    /**
     * Последняя внешняя версия через MZStorePlatform (p=mdm-lockup) — каталоги
     * enterprisestore, iphone, ipad по очереди, страна витрины аккаунта.
     * Без закрепления версии redownload может отдать сборку чужой платформы.
     */
    private fun lookupLatestExternalVersionId(adam: Long): String {
        val cc = StoreProtocol.countryCodeFromStorefront(storeFront)
            ?: error("Неизвестная витрина аккаунта: $storeFront")
        var last = ""
        for (catalog in StoreProtocol.PLATFORM_CATALOGS) {
            val req = Request.Builder().url(StoreProtocol.platformLookupUrl(adam, catalog, cc)).get().build()
            val body = Http.itunes.newCall(req).execute().use { resp ->
                if (resp.code != 200) error("platform version lookup request failed: ${resp.code}")
                resp.body?.string().orEmpty()
            }
            val root = runCatching { SignumJson.parseToJsonElement(body).jsonObject }.getOrNull()
                ?: error("platform version lookup returned invalid JSON")
            when (val r = StoreProtocol.parsePlatformLookup(root, adam)) {
                is StoreProtocol.PlatformLookup.Found -> return r.externalVersionId
                is StoreProtocol.PlatformLookup.Missing -> last = r.reason
            }
        }
        error("app $adam in storefront $cc (catalogs: enterprisestore, iphone, ipad): $last")
    }

    /** bag.xml?guid=…&ix=6 с UA Configurator; при сбое — адреса по умолчанию. */
    private fun bag(): StoreProtocol.Bag {
        bagCache?.let { (at, bag) -> if (System.currentTimeMillis() - at in 0..3_600_000) return bag }
        val bag = runCatching {
            val req = Request.Builder().url("${StoreProtocol.BAG_URL}?guid=$storeGuid&ix=6")
                .header("Accept", "application/xml")
                .header("User-Agent", StoreProtocol.BAG_USER_AGENT)
                .get().build()
            Http.itunes.newCall(req).execute().use { resp ->
                check(resp.code == 200) { "bag HTTP ${resp.code}" }
                StoreProtocol.parseBag(Plist.parse(resp.body?.bytes() ?: ByteArray(0)), Plist::parse)
            }
        }.onFailure { Log.i("SignumStore", "bag: ${it.message}, defaults") }
            .getOrDefault(StoreProtocol.Bag())
        bagCache = System.currentTimeMillis() to bag
        return bag
    }

    private fun postStorePlist(startUrl: String, body: NSDictionary, ani: AnisetteData): NSDictionary? =
        postStore(startUrl, body, ani).second

    private fun postStore(startUrl: String, body: NSDictionary, ani: AnisetteData): Pair<Int, NSDictionary?> {
        val owner = store.activeEmail()
        val originalPod = pod
        val originalCookies = cookies.toMap()
        var status = 0
        val bytes = StorePodRouting.execute(
            startUrl,
            send = { url ->
                check(store.activeEmail() == owner) { "Apple ID изменился. Повторите операцию." }
                val req = authorizedRequest(url, body, ani)
                Http.itunesNoFollow.newCall(req).execute().use { resp ->
                    check(store.activeEmail() == owner) { "Apple ID изменился. Повторите операцию." }
                    ingestCookies(resp.headers("Set-Cookie"), resp.headers)
                    status = resp.code
                    StorePodRouting.Reply(
                        resp.code, resp.body?.bytes() ?: ByteArray(0),
                        resp.header("Location"), resp.header("pod") ?: cookies["itspod"],
                    )
                }
            },
            onPod = { pod = it },
        )
        if ((originalPod != pod || originalCookies != cookies) && isReady() && store.activeEmail() == owner) persistStore()
        val dict = runCatching { Plist.parse(bytes) }.getOrElse {
            pw.appbank.signum.data.api.DebugDump.logFail("SignumStore", bytes, it)
            null
        }
        return status to dict
    }

    private fun ingestCookies(setCookies: List<String>, headers: okhttp3.Headers) =
        ingestCookiesInto(cookies, setCookies, headers)

    private fun ingestCookiesInto(target: ConcurrentHashMap<String, String>, setCookies: List<String>, headers: okhttp3.Headers) {
        for (line in setCookies) {
            val pair = line.substringBefore(';')
            val idx = pair.indexOf('=')
            if (idx <= 0) continue
            val k = pair.substring(0, idx).trim()
            val v = pair.substring(idx + 1).trim()
            if (v.isEmpty()) target.remove(k) else target[k] = v
        }
        StorePodRouting.validPod(headers["pod"] ?: target["itspod"])?.let { pod = it }
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

    /**
     * adamId по bundle ID — `lookup` ipatool: сначала витрина аккаунта, затем
     * запрос без страны (как было раньше) на случай другого региона.
     */
    private fun lookupAdamByBundle(bundleId: String): String? {
        val cc = StoreProtocol.countryCodeFromStorefront(storeFront)
        if (cc != null) {
            runCatching { lookupAdam(StoreProtocol.lookupUrl(cc, bundleId = bundleId)) }.getOrNull()?.let { return it }
        }
        return lookupAdam(
            "https://itunes.apple.com/lookup?bundleId=${java.net.URLEncoder.encode(bundleId, "UTF-8")}&entity=software&limit=1",
        )
    }

    private fun lookupAdam(url: String): String? {
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
            val body = StoreProtocol.downloadRequestPayload(storeGuid, adamId.toLong(), ext)
            val request = Request.Builder().url("${bag().updateEndpoint}?guid=$storeGuid")
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

        // ipatool v2 login flow, ported: idmsa endpoints + the oauth service
        // key ipatool sends as X-Apple-Widget-Key.
        private const val IDMSA_SIGNIN = "https://idmsa.apple.com/appleauth/auth/signin"
        private const val IDMSA_VERIFY_DEVICE = "https://idmsa.apple.com/appleauth/auth/verify/trusteddevice"
        private const val IDMSA_SECURITY_CODE = "https://idmsa.apple.com/appleauth/auth/verify/trusteddevice/securitycode"
        private const val IDMSA_SESSION = "https://idmsa.apple.com/appleauth/auth"
        private const val IDMSA_ACCOUNT = "https://idmsa.apple.com/appleauth/auth/account"
        private const val STORE_AUTHENTICATE = "https://buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/authenticate"
        private const val IDMSA_SERVICE_KEY = "d718df87e05b79a094e85315bf0cf6bec4cdcc80ddbf01e13d3a748a7e4580b2e5704ba869069b4067696d540531b476507c4de5053db8ee08770cc6b8aa2f81"

        // Match ipatool's Configurator UA exactly (pkg/http/constants.go); the
        // FairPlay SAP signature is minted in a Configurator/CommerceKit context.
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
