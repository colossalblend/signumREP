package pw.appbank.signum.data.apple

import android.util.Base64
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import pw.appbank.signum.data.api.DebugDump
import pw.appbank.signum.data.api.Http
import pw.appbank.signum.data.api.SignumJson
import pw.appbank.signum.data.api.anyStr
import pw.appbank.signum.data.api.obj
import pw.appbank.signum.data.api.str
import pw.appbank.signum.data.models.PurchaseHistoryApp
import pw.appbank.signum.data.models.PurchaseHistoryResult
import pw.appbank.signum.data.settings.SettingsStore
import java.security.MessageDigest
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import java.util.UUID

   
                                                                                           
                           
   
class PurchaseHistoryClient(
    private val store: SettingsStore,
) {
    private var pending: PendingAuth? = null
    private var rapDsid: String? = null
    private var rapXsrf: String? = null

    fun clear() {
        pending = null
        rapDsid = null
        rapXsrf = null
    }

    fun loadCached(email: String): PurchaseHistoryResult? {
        val json = store.loadHistoryCache(email) ?: return null
        return runCatching {
            val root = SignumJson.parseToJsonElement(json).jsonObject
            val apps = (root["apps"] as? JsonArray)?.mapNotNull { parseCached(it.jsonObject) } ?: emptyList()
            PurchaseHistoryResult(
                email = email,
                count = apps.size,
                apps = apps,
                fromCache = true,
                sessionSaved = !store.loadHistoryCookies(email).isNullOrBlank(),
            )
        }.getOrNull()
    }

    fun fetch(
        email: String,
        password: String?,
        twoFactor: String?,
        loadMore: Boolean,
        batchId: String?,
        forceRefresh: Boolean,
    ): PurchaseHistoryResult {
        val mail = email.trim().lowercase()
        if (!forceRefresh && !loadMore && twoFactor.isNullOrBlank()) {
            loadCached(mail)?.let { return it }
            val cookies = cookieMap(mail)
            if (!cookies.containsKey("myacinfo")) {
                return PurchaseHistoryResult(
                    email = mail,
                    count = 0,
                    apps = emptyList(),
                    message = "Нажмите «Обновить историю». После входа Apple может запросить код.",
                )
            }
        }
        val pw = password?.takeIf { it.isNotEmpty() } ?: store.loadPassword(mail)
            ?: error("Пароль Apple ID не найден. Войдите снова.")
        val code = twoFactor?.filter { it.isDigit() }.orEmpty().take(6)
        if (pending == null || !pending!!.matches(mail)) {
            pending = loadPending(mail)
        }

        if (pending != null && pending!!.matches(mail) && !pending!!.expired() && pending!!.verified) {
            val session = IdmsaSession.fromPending(pending!!)
            return runCatching { finishRap(mail, session, loadMore, batchId) }
                .onSuccess {
                    pending = null
                    store.deleteHistoryPending(mail)
                }
                .getOrElse { e ->
                    savePending(mail, pending!!)
                    throw e
                }
        }

        if (code.length >= 6) {
            val p = pending
            if (p == null || !p.matches(mail) || p.expired()) {
                store.deleteHistoryPending(mail)
                pending = null
                error("Сессия для кода истекла. Сотрите код, нажмите «Обновить историю», дождитесь нового кода и введите его.")
            }
            val session = IdmsaSession.fromPending(p)
            if (!p.verified) {
                session.verify2fa(code, p.phoneId, p.preferPhone)
            }
            val verified = session.toPending(mail, p.phoneId, p.preferPhone, verified = true)
            pending = verified
            savePending(mail, verified)
            return runCatching { finishRap(mail, session, loadMore, batchId) }
                .onSuccess {
                    pending = null
                    store.deleteHistoryPending(mail)
                }
                .getOrElse { e ->
                    savePending(mail, verified)
                    throw e
                }
        }

        if (pending != null && pending!!.matches(mail) && !pending!!.expired()) {
            if (forceRefresh) {
                runCatching {
                    val session = IdmsaSession.fromPending(pending!!)
                    pending = session.arm2fa(mail)
                    savePending(mail, pending!!)
                }
            }
            return PurchaseHistoryResult(
                email = mail,
                count = 0,
                apps = emptyList(),
                needs2fa = true,
                message = "Введите 6-значный код с iPhone для истории покупок.",
            )
        }

        val cookies = cookieMap(mail)
        if (cookies.containsKey("myacinfo")) {
            val viaCookies = runCatching { pullWithCookies(mail, cookies, loadMore, batchId) }
            viaCookies.getOrNull()?.let { return it }
            if (!forceRefresh) {
                viaCookies.exceptionOrNull()?.let { throw it }
            }
        }

        val session = IdmsaSession()
        session.bootstrap()
        val federate = session.postJson(
            "$AUTH/federate?isRememberMeEnabled=true",
            JSONObject().put("accountName", mail).put("rememberMe", true),
            withHc = false,
        )
        if (federate.status == 401 || federate.status == 403) {
            error(classifyAuth(federate.body))
        }

        val srp = SrpClient(mail.toByteArray())
        var init = session.postJson(
            "$AUTH/signin/init",
            JSONObject()
                .put("a", b64(srp.aPub))
                .put("accountName", mail)
                .put("protocols", JSONArray().put("s2k").put("s2k_fo")),
            withHc = false,
        )
        if (init.status == 503) {
            Thread.sleep(1200)
            init = session.postJson(
                "$AUTH/signin/init",
                JSONObject()
                    .put("a", b64(srp.aPub))
                    .put("accountName", mail)
                    .put("protocols", JSONArray().put("s2k").put("s2k_fo")),
                withHc = false,
            )
        }
        if (init.status >= 400) error(classifyAuth(init.body))
        val initJson = JSONObject(init.body.ifBlank { "{}" })
        val salt = b64d(initJson.optString("salt"))
        val b = b64d(initJson.optString("b"))
        val c = initJson.optString("c")
        val iters = initJson.optInt("iteration", 20000)
        val protocol = initJson.optString("protocol", "s2k")
        if (salt.isEmpty() || b.isEmpty()) error("Не удалось начать вход в историю покупок. Повторите.")
        val derived = deriveS2k(pw, protocol, salt, iters)
        val verifier = srp.processReply(derived, salt, b)
        val complete = session.postJson(
            "$AUTH/signin/complete?isRememberMeEnabled=true",
            JSONObject()
                .put("accountName", mail)
                .put("rememberMe", true)
                .put("m1", b64(verifier.proof))
                .put("c", c)
                .put("m2", b64(verifier.m2))
                .put("trustTokens", JSONArray()),
            withHc = true,
        )
        val authType = runCatching { JSONObject(complete.body.ifBlank { "{}" }).optString("authType") }.getOrDefault("")
        when {
            complete.status == 409 || authType == "hsa2" || authType == "2sa" || looksLike2fa(complete.body) && complete.status != 200 -> {
                pending = session.arm2fa(mail)
                savePending(mail, pending!!)
                return PurchaseHistoryResult(
                    email = mail,
                    count = 0,
                    apps = emptyList(),
                    needs2fa = true,
                    message = "Введите 6-значный код с iPhone для истории покупок.",
                )
            }
            complete.status == 401 || complete.status == 403 -> error(classifyAuth(complete.body))
            complete.status == 412 || authType == "sa" || looksLikeRepair(complete.body) -> session.runSaRepair()
            complete.status >= 400 -> error(classifyAuth(complete.body))
        }
        return finishRap(mail, session, loadMore, batchId)
    }

    private fun finishRap(
        email: String,
        session: IdmsaSession,
        loadMore: Boolean,
        batchId: String?,
    ): PurchaseHistoryResult {
        session.handoff()
        val result = session.fetchPurchases(email, loadMore, batchId, rapDsid, rapXsrf)
        rapDsid = result.dsid
        rapXsrf = result.xsrf
        persistCookies(email, result.cookies)
        if (result.apps.isNotEmpty() || loadCached(email) == null) {
            persistCache(email, result.apps)
        }
        return PurchaseHistoryResult(
            email = email,
            count = result.apps.size,
            apps = result.apps,
            message = if (result.apps.isEmpty()) {
                "История открыта, но покупок не найдено. Нажмите «Обновить историю»."
            } else {
                "Загружено покупок: ${result.apps.size}"
            },
            hasMore = result.hasMore,
            nextBatchId = result.nextBatchId,
            sessionSaved = result.cookies.isNotEmpty(),
            fromCache = false,
        )
    }

    private fun pullWithCookies(
        email: String,
        cookies: Map<String, String>,
        loadMore: Boolean,
        batchId: String?,
    ): PurchaseHistoryResult {
        val session = IdmsaSession()
        session.cookies.putAll(stripEphemeral(cookies))
        if (!session.cookies.containsKey("myacinfo")) error("Сессия истории устарела. Нажмите «Обновить».")
        session.getFollow("https://daiquiri-ext.itunes.apple.com/__logged_in/reportaproblem.apple.com")
        session.getFollow("$RAP/")
        val result = session.fetchPurchases(email, loadMore, batchId, rapDsid, rapXsrf)
        rapDsid = result.dsid
        rapXsrf = result.xsrf
        val merged = cookies.toMutableMap()
        merged.putAll(result.cookies)
        persistCookies(email, merged)
        if (result.apps.isNotEmpty() || loadCached(email) == null) {
            persistCache(email, result.apps)
        }
        return PurchaseHistoryResult(
            email = email,
            count = result.apps.size,
            apps = result.apps,
            message = if (result.apps.isEmpty()) {
                "История открыта, но покупок не найдено. Нажмите «Обновить историю»."
            } else {
                "Загружено покупок: ${result.apps.size}"
            },
            hasMore = result.hasMore,
            nextBatchId = result.nextBatchId,
            sessionSaved = true,
            fromCache = false,
        )
    }

    private fun parseCached(o: JsonObject): PurchaseHistoryApp? {
        val adam = o["adamId"]?.jsonPrimitive?.contentOrNull ?: return null
        val name = o["name"]?.jsonPrimitive?.contentOrNull ?: return null
        return PurchaseHistoryApp(
            adamId = adam,
            name = name,
            artist = o["artist"]?.jsonPrimitive?.contentOrNull,
            artwork = o["artwork"]?.jsonPrimitive?.contentOrNull,
            bundleId = o["bundleId"]?.jsonPrimitive?.contentOrNull,
        )
    }

    private fun cookieMap(email: String): MutableMap<String, String> {
        val raw = store.loadHistoryCookies(email) ?: return linkedMapOf()
        return raw.split(';').mapNotNull {
            val p = it.split('=', limit = 2)
            if (p.size == 2) p[0].trim() to p[1].trim() else null
        }.toMap().toMutableMap()
    }

    private fun savePending(email: String, p: PendingAuth) {
        val o = JSONObject()
            .put("email", p.email)
            .put("cookies", p.cookies.entries.joinToString(";") { "${it.key}=${it.value}" })
            .put("scnt", p.scnt)
            .put("sessionId", p.sessionId)
            .put("sessionToken", p.sessionToken)
            .put("repairToken", p.repairToken)
            .put("authAttributes", p.authAttributes)
            .put("phoneId", p.phoneId)
            .put("preferPhone", p.preferPhone)
            .put("frameId", p.frameId)
            .put("hcBits", p.hcBits)
            .put("hcChallenge", p.hcChallenge)
            .put("createdAt", p.createdAt)
            .put("verified", p.verified)
        store.storeHistoryPending(email, o.toString())
    }

    private fun loadPending(email: String): PendingAuth? {
        val raw = store.loadHistoryPending(email) ?: return null
        return runCatching {
            val o = JSONObject(raw)
            val cookies = o.optString("cookies").split(';').mapNotNull {
                val p = it.split('=', limit = 2)
                if (p.size == 2) p[0].trim() to p[1].trim() else null
            }.toMap()
            PendingAuth(
                email = o.optString("email", email),
                cookies = cookies,
                scnt = o.optString("scnt").takeIf { it.isNotBlank() },
                sessionId = o.optString("sessionId").takeIf { it.isNotBlank() },
                sessionToken = o.optString("sessionToken").takeIf { it.isNotBlank() },
                repairToken = o.optString("repairToken").takeIf { it.isNotBlank() },
                authAttributes = o.optString("authAttributes").takeIf { it.isNotBlank() },
                phoneId = if (o.has("phoneId") && !o.isNull("phoneId")) o.optLong("phoneId") else null,
                preferPhone = o.optBoolean("preferPhone"),
                frameId = o.optString("frameId").ifBlank { "daw-${UUID.randomUUID()}" },
                hcBits = o.optInt("hcBits", 10),
                hcChallenge = o.optString("hcChallenge").takeIf { it.isNotBlank() },
                createdAt = o.optLong("createdAt", System.currentTimeMillis()),
                verified = o.optBoolean("verified"),
            ).takeUnless { it.expired() }
        }.getOrNull()
    }

    private fun persistCookies(email: String, cookies: Map<String, String>) {
        val keep = stripEphemeral(cookies)
        if (keep.isEmpty()) return
        store.storeHistoryCookies(email, keep.entries.joinToString(";") { "${it.key}=${it.value}" })
    }

    private fun persistCache(email: String, apps: List<PurchaseHistoryApp>) {
        val json = buildString {
            append("{\"apps\":[")
            apps.forEachIndexed { i, a ->
                if (i > 0) append(',')
                append("{\"adamId\":").append(jsonStr(a.adamId))
                append(",\"name\":").append(jsonStr(a.name))
                a.artist?.let { append(",\"artist\":").append(jsonStr(it)) }
                a.artwork?.let { append(",\"artwork\":").append(jsonStr(it)) }
                a.bundleId?.let { append(",\"bundleId\":").append(jsonStr(it)) }
                append('}')
            }
            append("]}")
        }
        store.storeHistoryCache(email, json)
    }

    private fun jsonStr(s: String) = "\"${s.replace("\\", "\\\\").replace("\"", "\\\"")}\""

    private class PendingAuth(
        val email: String,
        val cookies: Map<String, String>,
        val scnt: String?,
        val sessionId: String?,
        val sessionToken: String?,
        val repairToken: String?,
        val authAttributes: String?,
        val phoneId: Long?,
        val preferPhone: Boolean,
        val frameId: String,
        val hcBits: Int,
        val hcChallenge: String?,
        val createdAt: Long = System.currentTimeMillis(),
        val verified: Boolean = false,
    ) {
        fun matches(mail: String) = email.equals(mail, true)
        fun expired() = System.currentTimeMillis() - createdAt > 10 * 60 * 1000
    }

    private class IdmsaSession {
        val cookies = linkedMapOf<String, String>()
        var scnt: String? = null
        var sessionId: String? = null
        var sessionToken: String? = null
        var repairToken: String? = null
        var authAttributes: String? = null
        var hcBits: Int = 10
        var hcChallenge: String? = null
        var frameId: String = "daw-${UUID.randomUUID()}"

        fun bootstrap() {
            getFollow("$IDMSA/IDMSWebAuth/signin?appIdKey=$WIDGET_KEY&path=%2F__logged_in%2Freportaproblem.apple.com&rv=1")
            getFollow(
                "$AUTH/authorize/signin?frame_id=$frameId&language=RU-RU&skVersion=7&iframeId=$frameId&client_id=$WIDGET_KEY&redirect_uri=$IDMSA&response_type=code&response_mode=web_message&state=$frameId&authVersion=latest",
            )
            getFollow("$AUTH/signin?widgetKey=$WIDGET_KEY")
        }

        fun handoff() {
            if (sessionToken.isNullOrBlank()) sessionToken = repairToken
            val grant = sessionToken.orEmpty()
            if (grant.isNotBlank()) {
                val form = "rememberMe=true&grantCode=${enc(grant)}&iframeId=${enc(frameId)}" +
                    "&requestUri=%2Fsignin&appIdKey=$WIDGET_KEY&language=RU-RU" +
                    "&path=%2F__logged_in%2Freportaproblem.apple.com&scnt=${enc(scnt.orEmpty())}"
                postForm("$IDMSA/IDMSWebAuth/signin", form)
            } else if (!cookies.containsKey("myacinfo")) {
                error(
                    "Код принят, но Apple не выдал токен сессии. " +
                        "Нажмите «Загрузить историю» ещё раз — повторно вводить код не нужно. " +
                        "Если не поможет, сотрите код и нажмите «Обновить историю».",
                )
            }
            getFollow("https://daiquiri-ext.itunes.apple.com/__logged_in/reportaproblem.apple.com")
            getFollow("$RAP/")
        }

        fun runSaRepair() {
            val q = "trustedWidgetDomain=${enc(IDMSA)}&widgetKey=$WIDGET_KEY&language=ru_RU"
            getFollow("https://appleid.apple.com/widget/account/repair?$q")
            getFollow("https://account.apple.com/widget/account/repair?$q")
            listOf(
                "https://appleid.apple.com/account/manage/repair/options",
                "https://appleid.apple.com/account/security/upgrade",
                "https://appleid.apple.com/account/security/upgrade/setuplater",
                "https://account.apple.com/account/security/upgrade",
                "https://account.apple.com/account/security/upgrade/setuplater",
            ).forEach { request("GET", it, repairHeaders(), null) }
            val h = authHeaders(false).toMutableMap()
            (repairToken ?: sessionToken)?.let { h["X-Apple-Repair-Session-Token"] = it }
            val resp = request("POST", "$AUTH/repair/complete", h, ByteArray(0))
            if (sessionToken.isNullOrBlank()) {
                runCatching {
                    val j = JSONObject(resp.body.ifBlank { "{}" })
                    for (key in listOf("sessionToken", "session_token", "X-Apple-Session-Token", "grantCode")) {
                        val t = j.optString(key)
                        if (t.isNotBlank()) {
                            sessionToken = t
                            break
                        }
                    }
                }
            }
            if (sessionToken.isNullOrBlank()) sessionToken = repairToken
        }

        fun arm2fa(email: String): PendingAuth {
            val frozenScnt = scnt
            val frozenSid = sessionId
            val frozenAttrs = authAttributes
            var phoneId: Long? = null
            var preferPhone = false
            for (minimal in listOf(true, false)) {
                scnt = frozenScnt
                sessionId = frozenSid
                authAttributes = frozenAttrs
                val headers = if (minimal) authHeadersMinimal() else authHeaders(false)
                headers.remove("Content-Type")
                val resp = request("GET", AUTH, headers, null, updateSession = true)
                if (resp.status == 200 || resp.status == 204) {
                    val info = runCatching { JSONObject(resp.body.ifBlank { "{}" }) }.getOrNull()
                    val phones = info?.optJSONArray("trustedPhoneNumbers")
                    if (phones != null && phones.length() > 0) {
                        phoneId = phones.optJSONObject(0)?.optLong("id")
                    }
                    val devices = info?.optJSONArray("trustedDevices")
                    val hasDevices = (devices?.length() ?: 0) > 0 || info?.optBoolean("noTrustedDevices", true) == false
                    preferPhone = phoneId != null && !hasDevices
                    break
                }
            }
            phoneId?.let { pid ->
                val body = JSONObject().put("phoneNumber", JSONObject().put("id", pid)).put("mode", "sms")
                request("PUT", "$AUTH/verify/phone", authHeadersMinimal(), body.toString().toByteArray())
            }
            scnt = frozenScnt ?: scnt
            sessionId = frozenSid ?: sessionId
            authAttributes = frozenAttrs ?: authAttributes
            return toPending(email, phoneId, preferPhone, verified = false)
        }

        fun verify2fa(code: String, phoneId: Long?, preferPhone: Boolean) {
            val frozenScnt = scnt
            val frozenSid = sessionId
            val frozenAttrs = authAttributes
            val simple = JSONObject().put("securityCode", JSONObject().put("code", code)).toString().toByteArray()
            val sms = JSONObject()
                .put("phoneNumber", JSONObject().put("id", phoneId ?: 1L))
                .put("securityCode", JSONObject().put("code", code))
                .put("mode", "sms")
                .toString()
                .toByteArray()
            val attempts = if (preferPhone) {
                listOf(
                    Triple("phone-sms", "$AUTH/verify/phone/securitycode", sms),
                    Triple("phone-simple", "$AUTH/verify/phone/securitycode", simple),
                    Triple("device", "$AUTH/verify/trusteddevice/securitycode", simple),
                )
            } else {
                listOf(
                    Triple("device", "$AUTH/verify/trusteddevice/securitycode", simple),
                    Triple("phone-simple", "$AUTH/verify/phone/securitycode", simple),
                    Triple("phone-sms", "$AUTH/verify/phone/securitycode", sms),
                )
            }
            for ((label, url, body) in attempts) {
                scnt = frozenScnt
                sessionId = frozenSid
                authAttributes = frozenAttrs
                val resp = requestOnce("POST", url, authHeadersMinimal(), body, updateSession = false)
                DebugDump.writeText("history-2fa-$label.txt", "st=${resp.status}\n${resp.body.take(500)}")
                if (resp.status == 200 || resp.status == 204) {
                    ingest(resp.headers, true)
                    takeTokens(resp)
                    trustAfter2fa()
                    return
                }
            }
            scnt = frozenScnt
            sessionId = frozenSid
            authAttributes = frozenAttrs
            error("Код не принят. Введите актуальный 6-значный код с iPhone.")
        }

        private fun trustAfter2fa() {
            for (headers in listOf(authHeadersMinimal(), authHeaders(false))) {
                val get = requestOnce("GET", "$AUTH/2sv/trust", headers, null, updateSession = true)
                takeTokens(get)
                val post = requestOnce("POST", "$AUTH/2sv/trust", headers, "{}".toByteArray(), updateSession = true)
                takeTokens(post)
                if (!sessionToken.isNullOrBlank() || cookies.containsKey("myacinfo")) return
            }
        }

        private fun takeTokens(hop: Hop) {
            ingest(hop.headers, true)
            hop.headers["location"]?.let { loc ->
                val grant = Regex("[?&](?:grantCode|sessionToken)=([^&]+)").find(loc)?.groupValues?.getOrNull(1)
                if (!grant.isNullOrBlank() && sessionToken.isNullOrBlank()) {
                    sessionToken = java.net.URLDecoder.decode(grant, "UTF-8")
                }
            }
            runCatching {
                val j = JSONObject(hop.body.ifBlank { "{}" })
                for (key in listOf(
                    "sessionToken", "session_token", "X-Apple-Session-Token",
                    "grantCode", "repairSessionToken",
                )) {
                    val t = j.optString(key).trim()
                    if (t.isNotBlank()) {
                        if (key.contains("repair", true)) {
                            if (repairToken.isNullOrBlank()) repairToken = t
                        } else if (sessionToken.isNullOrBlank()) {
                            sessionToken = t
                        }
                    }
                }
            }
        }

        fun fetchPurchases(
            email: String,
            loadMore: Boolean,
            batchId: String?,
            reuseDsid: String?,
            reuseXsrf: String?,
        ): RapPage {
            val start = batchId?.takeIf { loadMore && it.isNotBlank() }
            var dsid = reuseDsid.orEmpty()
            var xsrf = reuseXsrf.orEmpty()
            if (dsid.isBlank() || xsrf.isBlank() || start == null) {
                val headers = linkedMapOf(
                    "User-Agent" to UA,
                    "Accept" to "application/json, text/plain, */*",
                    "Accept-Language" to "ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7",
                    "X-Apple-Rap2-Api" to RAP_API,
                    "Referer" to "$RAP/",
                )
                var login = request("GET", "$RAP/api/login", headers, null)
                if (login.status != 200) {
                    login = request(
                        "GET",
                        "$RAP/api/login",
                        mapOf("Accept" to "application/json", "X-Apple-Rap2-Api" to RAP_API),
                        null,
                    )
                }
                if (login.status == 401 || login.status == 403 || login.status == 400 || login.body.trimStart().startsWith("<")) {
                    error("Сессия истории устарела. Нажмите «Обновить» — Apple может запросить код.")
                }
                val j = JSONObject(login.body.ifBlank { "{}" })
                dsid = j.optString("dsid").ifBlank { j.opt("dsid")?.toString().orEmpty() }
                xsrf = j.optString("token")
                if (dsid.isBlank() || xsrf.isBlank()) {
                    error("Сессия истории устарела. Нажмите «Обновить» — Apple может запросить код.")
                }
            }
            val payload = JSONObject().put("dsid", dsid)
            if (!start.isNullOrBlank()) payload.put("batchId", start)
            val searchHeaders = linkedMapOf(
                "User-Agent" to UA,
                "Content-Type" to "application/json",
                "Accept" to "application/json, text/plain, */*",
                "X-Apple-Rap2-Api" to RAP_API,
                "X-Apple-Xsrf-Token" to xsrf,
                "Origin" to RAP,
                "Referer" to "$RAP/",
            )
            val search = request("POST", "$RAP/api/purchase/search", searchHeaders, payload.toString().toByteArray())
            DebugDump.writeText("history-search.txt", "st=${search.status}\n${search.body.take(4000)}")
            if (search.status >= 400) {
                error("Не удалось получить историю покупок. Повторите попытку.")
            }
            val data = runCatching { SignumJson.parseToJsonElement(search.body).jsonObject }.getOrNull()
            val apps = extractPurchases(data)
            val next = data?.str("nextBatchId") ?: data?.str("nextBatchID") ?: data?.str("next_batch_id")
            return RapPage(apps, cookies.toMap(), next, !next.isNullOrBlank(), dsid, xsrf)
        }

        fun postJson(url: String, body: JSONObject, withHc: Boolean): Hop {
            return request("POST", url, authHeaders(withHc), body.toString().toByteArray())
        }

        fun postForm(url: String, form: String): Hop {
            val headers = linkedMapOf(
                "User-Agent" to UA,
                "Content-Type" to "application/x-www-form-urlencoded",
                "Accept" to "text/html,application/xhtml+xml",
                "Origin" to IDMSA,
                "Referer" to "$IDMSA/",
            )
            return request("POST", url, headers, form.toByteArray())
        }

        fun getFollow(url: String): Hop {
            val headers = linkedMapOf(
                "User-Agent" to UA,
                "Accept" to "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8",
            )
            return request("GET", url, headers, null)
        }

        private fun requestOnce(
            method: String,
            url: String,
            headers: Map<String, String>,
            body: ByteArray?,
            updateSession: Boolean,
        ): Hop {
            val builder = Request.Builder().url(url)
            headers.forEach { (k, v) -> if (v.isNotBlank()) builder.header(k, v) }
            if (cookies.isNotEmpty()) {
                builder.header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
            }
            if (method == "POST") {
                builder.post((body ?: ByteArray(0)).toRequestBody((headers["Content-Type"] ?: "application/json").toMediaType()))
            } else if (method == "PUT") {
                builder.put((body ?: ByteArray(0)).toRequestBody((headers["Content-Type"] ?: "application/json").toMediaType()))
            } else {
                builder.get()
            }
            return Http.apple.newCall(builder.build()).execute().use { resp ->
                val bytes = resp.body?.bytes() ?: ByteArray(0)
                val hdrs = resp.headers.toMultimap().mapKeys { it.key.lowercase() }.mapValues { it.value.firstOrNull().orEmpty() }.toMutableMap()
                resp.header("Location")?.let { hdrs["location"] = it }
                ingest(hdrs, updateSession)
                ingestSetCookie(resp.headers("Set-Cookie"))
                DebugDump.writeText(
                    "history-once.txt",
                    "url=$url st=${resp.code} token=${!sessionToken.isNullOrBlank()} myacinfo=${cookies.containsKey("myacinfo")}\n${bytes.toString(Charsets.UTF_8).take(400)}",
                )
                Hop(resp.code, bytes.toString(Charsets.UTF_8), hdrs)
            }
        }

        private fun request(
            method: String,
            startUrl: String,
            headers: Map<String, String>,
            body: ByteArray?,
            updateSession: Boolean = true,
        ): Hop {
            var url = startUrl
            var currentMethod = method
            var currentBody = body
            repeat(12) {
                val builder = Request.Builder().url(url)
                headers.forEach { (k, v) -> if (v.isNotBlank()) builder.header(k, v) }
                if (cookies.isNotEmpty()) {
                    builder.header("Cookie", cookies.entries.joinToString("; ") { "${it.key}=${it.value}" })
                }
                if (currentMethod == "POST") {
                    builder.post((currentBody ?: ByteArray(0)).toRequestBody(headers["Content-Type"]?.toMediaType()))
                } else if (currentMethod == "PUT") {
                    builder.put((currentBody ?: ByteArray(0)).toRequestBody(headers["Content-Type"]?.toMediaType()))
                } else {
                    builder.get()
                }
                Http.apple.newCall(builder.build()).execute().use { resp ->
                    val bytes = resp.body?.bytes() ?: ByteArray(0)
                    ingest(
                        resp.headers.toMultimap().mapKeys { it.key.lowercase() }.mapValues { it.value.firstOrNull().orEmpty() },
                        updateSession,
                    )
                    ingestSetCookie(resp.headers("Set-Cookie"))
                    if (resp.code in listOf(301, 302, 303, 307, 308)) {
                        val loc = resp.header("Location").orEmpty()
                        if (loc.isBlank()) return Hop(resp.code, bytes.toString(Charsets.UTF_8), emptyMap())
                        val next = resolveUrl(url, loc)
                        if (!isAuthHost(next)) return Hop(resp.code, bytes.toString(Charsets.UTF_8), emptyMap())
                        url = next
                        if (resp.code != 307 && resp.code != 308) {
                            currentMethod = "GET"
                            currentBody = null
                        }
                    } else {
                        DebugDump.writeText("history-last.txt", "url=$url st=${resp.code}\n${bytes.toString(Charsets.UTF_8).take(400)}")
                        return Hop(resp.code, bytes.toString(Charsets.UTF_8), resp.headers.toMultimap().mapValues { it.value.firstOrNull().orEmpty() })
                    }
                }
            }
            error("Не удалось завершить вход в историю покупок.")
        }

        fun ingest(headers: Map<String, String>, updateSession: Boolean) {
            if (!updateSession) return
            val h = headers.mapKeys { it.key.lowercase() }
            h["scnt"]?.takeIf { it.isNotBlank() }?.let { scnt = it }
            h["x-apple-id-session-id"]?.takeIf { it.isNotBlank() }?.let { sessionId = it }
            h["x-apple-session-token"]?.takeIf { it.isNotBlank() }?.let { sessionToken = it }
            h["x-apple-repair-session-token"]?.takeIf { it.isNotBlank() }?.let { repairToken = it }
            h["x-apple-auth-attributes"]?.takeIf { it.isNotBlank() }?.let { authAttributes = it }
            h["x-apple-hc-bits"]?.toIntOrNull()?.takeIf { it in 1..20 }?.let { hcBits = it }
            h["x-apple-hc-challenge"]?.takeIf { it.isNotBlank() }?.let { hcChallenge = it }
        }

        private fun ingestSetCookie(lines: List<String>) {
            for (line in lines) {
                val pair = line.substringBefore(';')
                val idx = pair.indexOf('=')
                if (idx <= 0) continue
                val k = pair.substring(0, idx).trim()
                val v = pair.substring(idx + 1).trim()
                if (v.isEmpty()) cookies.remove(k) else cookies[k] = v
            }
        }

        fun authHeaders(withHc: Boolean): MutableMap<String, String> {
            val h = linkedMapOf(
                "User-Agent" to UA,
                "Content-Type" to "application/json",
                "Accept" to "application/json, text/javascript, */*; q=0.01",
                "Accept-Language" to "ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7",
                "X-Apple-Widget-Key" to WIDGET_KEY,
                "X-Apple-OAuth-Client-Id" to WIDGET_KEY,
                "X-Apple-OAuth-Client-Type" to "firstPartyAuth",
                "X-Apple-OAuth-Redirect-URI" to IDMSA,
                "X-Apple-OAuth-Response-Type" to "code",
                "X-Apple-OAuth-Response-Mode" to "web_message",
                "X-Apple-OAuth-Require-Grant-Code" to "true",
                "X-Apple-OAuth-State" to frameId,
                "X-Apple-Frame-Id" to frameId,
                "X-Apple-App-Id" to APP_ID,
                "X-Apple-Locale" to "RU-RU",
                "X-Apple-Trusted-Domain" to IDMSA,
                "X-Apple-Privacy-Consent-Accepted" to "true",
                "X-Requested-With" to "XMLHttpRequest",
                "Origin" to IDMSA,
                "Referer" to "$IDMSA/",
                "X-Apple-I-FD-Client-Info" to
                    """{"U":"$UA","L":"ru-RU","Z":"GMT+03:00","V":"1.1","F":"Nba44j1e3NlY5BNlY5BSs5uQ084akLMeN.DmJa"}""",
            )
            scnt?.let { h["scnt"] = it }
            sessionId?.let { h["X-Apple-ID-Session-Id"] = it }
            sessionToken?.let { h["X-Apple-Session-Token"] = it }
            authAttributes?.let { h["X-Apple-Auth-Attributes"] = it }
            if (withHc) {
                val challenge = hcChallenge ?: UUID.randomUUID().toString().replace("-", "")
                h["X-Apple-HC"] = hashcash(hcBits, challenge)
            }
            return h
        }

        fun authHeadersMinimal(): MutableMap<String, String> {
            val h = linkedMapOf(
                "User-Agent" to UA,
                "Content-Type" to "application/json",
                "Accept" to "application/json",
                "X-Apple-Widget-Key" to WIDGET_KEY,
                "X-Requested-With" to "XMLHttpRequest",
                "Origin" to IDMSA,
                "Referer" to "$IDMSA/",
            )
            scnt?.let { h["scnt"] = it }
            sessionId?.let { h["X-Apple-Id-Session-Id"] = it }
            authAttributes?.let { h["X-Apple-Auth-Attributes"] = it }
            return h
        }

        fun repairHeaders(): MutableMap<String, String> {
            val h = linkedMapOf(
                "User-Agent" to UA,
                "Accept" to "application/json, text/plain, */*",
                "Content-Type" to "application/json",
                "Origin" to "https://appleid.apple.com",
                "Referer" to "https://appleid.apple.com/",
                "X-Apple-Widget-Key" to WIDGET_KEY,
                "X-Apple-Repair-App-Id" to APP_ID,
            )
            scnt?.let { h["scnt"] = it }
            sessionId?.let { h["X-Apple-ID-Session-Id"] = it }
            sessionToken?.let { h["X-Apple-Session-Token"] = it }
            return h
        }

        fun toPending(email: String, phoneId: Long?, preferPhone: Boolean, verified: Boolean = false) = PendingAuth(
            email = email,
            cookies = cookies.toMap(),
            scnt = scnt,
            sessionId = sessionId,
            sessionToken = sessionToken,
            repairToken = repairToken,
            authAttributes = authAttributes,
            phoneId = phoneId,
            preferPhone = preferPhone,
            frameId = frameId,
            hcBits = hcBits,
            hcChallenge = hcChallenge,
            verified = verified,
        )

        companion object {
            fun fromPending(p: PendingAuth) = IdmsaSession().apply {
                cookies.putAll(p.cookies)
                scnt = p.scnt
                sessionId = p.sessionId
                sessionToken = p.sessionToken
                repairToken = p.repairToken
                authAttributes = p.authAttributes
                frameId = p.frameId
                hcBits = p.hcBits
                hcChallenge = p.hcChallenge
            }
        }
    }

    private data class Hop(val status: Int, val body: String, val headers: Map<String, String>)
    private data class RapPage(
        val apps: List<PurchaseHistoryApp>,
        val cookies: Map<String, String>,
        val nextBatchId: String?,
        val hasMore: Boolean,
        val dsid: String?,
        val xsrf: String?,
    )

    companion object {
        private const val RAP = "https://reportaproblem.apple.com"
        private const val IDMSA = "https://idmsa.apple.com"
        private const val AUTH = "https://idmsa.apple.com/appleauth/auth"
        private const val WIDGET_KEY =
            "20379f32034f8867d352666ff2904d2152d5ff6843ee2db5ab5df863c14b1aef"
        private const val APP_ID = "4199"
        private const val RAP_API = "3.0.0"
        private const val UA =
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36"

        private fun b64(bytes: ByteArray) = Base64.encodeToString(bytes, Base64.NO_WRAP)
        private fun b64d(s: String): ByteArray = runCatching { Base64.decode(s, Base64.DEFAULT) }.getOrDefault(ByteArray(0))
        private fun enc(s: String) = java.net.URLEncoder.encode(s, "UTF-8")

        private fun deriveS2k(password: String, protocol: String, salt: ByteArray, iters: Int): ByteArray {
            val digest = MessageDigest.getInstance("SHA-256").digest(password.toByteArray())
            val material = if (protocol == "s2k_fo") digest.joinToString("") { "%02x".format(it) }.toByteArray() else digest
            val mac = javax.crypto.Mac.getInstance("HmacSHA256")
            mac.init(javax.crypto.spec.SecretKeySpec(material, "HmacSHA256"))
            val block = ByteArray(salt.size + 4)
            System.arraycopy(salt, 0, block, 0, salt.size)
            block[salt.size + 3] = 1
            var u = mac.doFinal(block)
            val out = u.copyOf()
            repeat(iters.coerceAtLeast(1) - 1) {
                u = mac.doFinal(u)
                for (i in out.indices) out[i] = (out[i].toInt() xor u[i].toInt()).toByte()
            }
            return out
        }

        private fun hashcash(bits: Int, challenge: String): String {
            val fmt = SimpleDateFormat("yyyyMMddHHmmss", Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") }
            val date = fmt.format(Date())
            val sha1 = MessageDigest.getInstance("SHA-1")
            var counter = 0L
            while (true) {
                val token = "1:$bits:$date:$challenge::$counter"
                val hash = sha1.digest(token.toByteArray())
                if (leadingZeroBits(hash) >= bits) return token
                counter++
                if (counter > 20_000_000L) return token
            }
        }

        private fun leadingZeroBits(hash: ByteArray): Int {
            var bits = 0
            for (b in hash) {
                val v = b.toInt() and 0xFF
                if (v == 0) bits += 8 else {
                    bits += v.countLeadingZeroBits() - 24
                    break
                }
            }
            return bits
        }

        private fun classifyAuth(body: String): String {
            val lower = body.lowercase()
            if ("locked" in lower || "заблок" in lower) {
                return "Apple ID временно заблокирован. Подождите немного и повторите вход."
            }
            if (looksLike2fa(body) && "password" !in lower && "-20101" !in lower) {
                return "Нужен код двухфакторной аутентификации. Введите 6 цифр с iPhone."
            }
            if ("-20101" in lower || "incorrect" in lower || "invalid" in lower && "password" in lower) {
                return "Неверный пароль Apple ID. Проверьте пароль и попробуйте снова."
            }
            runCatching {
                val svc = JSONObject(body.ifBlank { "{}" }).optJSONArray("serviceErrors")
                if (svc != null) {
                    for (i in 0 until svc.length()) {
                        val e = svc.optJSONObject(i) ?: continue
                        val code = e.optString("code")
                        val msg = e.optString("message").lowercase()
                        if (code == "-20209" || "locked" in msg) {
                            return "Apple ID временно заблокирован. Подождите немного и повторите вход."
                        }
                        if (code == "-20101" || "password" in msg || "incorrect" in msg) {
                            return "Неверный пароль Apple ID. Проверьте пароль и попробуйте снова."
                        }
                    }
                }
            }
            return "Не удалось открыть историю покупок. Повторите попытку."
        }

        private fun looksLike2fa(body: String): Boolean {
            val t = body.lowercase()
            return "hsa2" in t || "two-factor" in t || "twofactor" in t || "securitycode" in t
        }

        private fun looksLikeRepair(body: String): Boolean {
            val t = body.lowercase()
            return ("authtype" in t && "\"sa\"" in t) || "hsa2_enrollment" in t || "repairattribute" in t
        }

        private fun stripEphemeral(cookies: Map<String, String>): Map<String, String> =
            cookies.filterKeys { k ->
                val n = k.lowercase()
                n != "dqsid" && n != "user-context" && n != "jsessionid" && !n.startsWith("selfserv_")
            }

        private fun isAuthHost(url: String): Boolean {
            val l = url.lowercase()
            return "idmsa.apple.com" in l || "appleid.apple.com" in l || "account.apple.com" in l ||
                "gsa.apple.com" in l || "reportaproblem.apple.com" in l || "daiquiri" in l ||
                "itunes.apple.com" in l
        }

        private fun resolveUrl(base: String, loc: String): String {
            if (loc.startsWith("http://") || loc.startsWith("https://")) return loc
            if (loc.startsWith("//")) return "https:$loc"
            if (loc.startsWith("/")) {
                val uri = java.net.URI(base)
                return "${uri.scheme}://${uri.host}$loc"
            }
            return loc
        }

        private fun extractPurchases(root: JsonObject?): List<PurchaseHistoryApp> {
            if (root == null) return emptyList()
            val purchases = root["purchases"] as? JsonArray
                ?: (root["data"] as? JsonObject)?.get("purchases") as? JsonArray
                ?: (root["result"] as? JsonObject)?.get("purchases") as? JsonArray
                ?: (root["body"] as? JsonObject)?.get("purchases") as? JsonArray
                ?: JsonArray(emptyList())
            val out = mutableListOf<PurchaseHistoryApp>()
            val seen = mutableSetOf<String>()
            purchases.forEach { el ->
                val pur = el as? JsonObject ?: return@forEach
                val date = pur.anyStr("purchaseDate", "invoiceDate")
                val price = pur.anyStr("estimatedTotalAmount", "invoiceAmount")
                val plis = pur["plis"] as? JsonArray
                if (plis == null) {
                    addPurchaseItem(pur, date, price, seen, out)
                    return@forEach
                }
                plis.forEach { p ->
                    val o = p as? JsonObject ?: return@forEach
                    addPurchaseItem(o, date, price, seen, out)
                }
            }
            return out
        }

        private fun addPurchaseItem(
            o: JsonObject,
            date: String?,
            price: String?,
            seen: MutableSet<String>,
            out: MutableList<PurchaseHistoryApp>,
        ) {
            val adam = o.anyStr("adamId", "adamID", "storeItemId", "trackId") ?: return
            if (!seen.add(adam)) return
            val lc = o.obj("localizedContent")
            val name = lc?.anyStr("nameForDisplay", "name")
                ?: o.anyStr("itemName", "name", "title")
                ?: "Приложение"
            out += PurchaseHistoryApp(
                adamId = adam,
                name = name,
                artist = lc?.anyStr("detailForDisplay") ?: o.anyStr("artistName", "artist"),
                artwork = lc?.anyStr("artworkURL", "artworkUrl") ?: o.anyStr("artworkUrl", "artwork"),
                price = price,
                date = o.anyStr("pliDate") ?: date,
                bundleId = o.anyStr("bundleId"),
            )
        }
    }
}
