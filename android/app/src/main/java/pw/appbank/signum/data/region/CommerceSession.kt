package pw.appbank.signum.data.region

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import pw.appbank.signum.data.api.DebugDump
import pw.appbank.signum.data.api.Http

/**
 * Разговор с платёжной частью Apple поверх уже открытого входа в Apple ID.
 *
 * Браузер здесь не нужен: страница смены страны — это обычные JSON-запросы,
 * просто их принимают только с куками входа и с заголовками, как будто запрос
 * пришёл со страницы apps.apple.com.
 */
internal class CommerceSession(private val cookies: MutableMap<String, String>) {

    /**
     * Apple различает витрины по числовому коду. Пока страна не сменилась,
     * запросы к оплате должны идти со старой витриной, иначе она отвечает отказом.
     */
    private var storeFront: String? = storeFrontFor(cookies["site"]) ?: RUS_FRONT

    private class Hop(val status: Int, val body: String) {
        val ok: Boolean get() = status in 200..299
    }

    // ---------------------------------------------------------------- транспорт

    private fun call(method: String, url: String, payload: String?, hops: Int = 0): Hop {
        val headers = linkedMapOf(
            "User-Agent" to UA,
            "Content-Type" to "application/json",
            "Accept" to "application/json, text/plain, */*",
            "Accept-Language" to "en-US,en;q=0.9",
        )
        val accountRequest = url.startsWith("https://account.apple.com/") ||
            url.startsWith("https://appleid.apple.com/")
        val gsRequest = url.startsWith(AUTH_GS)
        if (accountRequest) {
            headers["Referer"] = "https://account.apple.com/"
        } else {
            // Платёжные хосты отвечают только на запросы «со страницы магазина».
            headers["Referer"] = "https://apps.apple.com/"
            headers["Origin"] = "https://apps.apple.com"
            if (gsRequest || url.startsWith(BUY)) headers["Authorization"] = AMP_BEARER
            if (url.startsWith(BUY)) storeFront?.let { headers["X-Apple-Store-Front"] = it }
        }
        val commerceOnly = gsRequest || url.startsWith(BUY) || url.startsWith(AMP)
        cookieHeader(commerceOnly)?.let { headers["Cookie"] = it }

        val builder = Request.Builder().url(url)
        headers.forEach { (k, v) -> if (v.isNotBlank()) builder.header(k, v) }
        if (method == "POST") {
            builder.post((payload ?: "").toByteArray().toRequestBody(JSON_MEDIA))
        } else {
            builder.get()
        }

        return Http.apple.newCall(builder.build()).execute().use { resp ->
            val body = resp.body?.bytes()?.toString(Charsets.UTF_8).orEmpty()
            storeSetCookies(resp.headers("Set-Cookie"))
            resp.header("x-set-apple-store-front")?.takeIf { it.isNotBlank() }?.let { storeFront = it }
            DebugDump.writeText("region-last.txt", "url=$url st=${resp.code}\n${body.take(600)}")
            val location = resp.header("Location")
            if (resp.code in 300..399 && !location.isNullOrBlank() && hops < 8) {
                // Клиент Http.apple редиректы не ходит сам, а вход в Apple ID без них не собрать.
                return@use call("GET", resolveUrl(url, location), null, hops + 1)
            }
            Hop(resp.code, body)
        }
    }

    private fun get(url: String) = call("GET", url, null)

    private fun post(url: String, payload: String) = call("POST", url, payload)

    private fun cookieHeader(commerceOnly: Boolean): String? {
        val parts = cookies.entries
            .filter { !commerceOnly || it.key.lowercase() in COMMERCE_COOKIES }
            .map { "${it.key}=${it.value}" }
            .sorted()
        return parts.takeIf { it.isNotEmpty() }?.joinToString("; ")
    }

    private fun storeSetCookies(lines: List<String>) {
        for (line in lines) {
            for (part in splitPacked(line)) {
                val first = part.substringBefore(';').trim()
                val idx = first.indexOf('=')
                if (idx <= 0) continue
                val name = first.substring(0, idx).trim()
                val value = first.substring(idx + 1).trim()
                // Пустое значение — это просьба стереть куку. Мы её пропускаем: Apple
                // присылает такую следом за свежим токеном и иначе гасит собственный вход.
                if (name.isNotEmpty() && value.isNotEmpty()) cookies[name] = value
            }
        }
    }

    // ------------------------------------------------------------ вход в магазин

    /**
     * Открывает платёжную часть аккаунта. Одних кук входа мало — Apple выдаёт
     * отдельное разрешение на работу с оплатой, и без него оплата отвечает отказом.
     */
    fun open() {
        var reached = false
        for (attempt in 0..1) {
            primeAccountPage()
            if (get("https://appleid.apple.com/account/manage/gs/ws/token").ok) {
                val gs = post(AUTH_GS, JSONObject().put("webAuthorizationFlowContext", "apps").toString())
                if (gs.ok) {
                    ingestGsBody(gs.body)
                    get("$BUY/account/web/infoRefresh")
                    val info = get("$BUY/account/web/info")
                    if (info.ok && !looksLikeAuthDialog(info.body)) {
                        reached = true
                        break
                    }
                }
            }
            if (attempt == 0) {
                refreshAccountCookies()
                Thread.sleep(500)
            }
        }
        if (!reached) error(SESSION_MESSAGE)
    }

    private fun primeAccountPage() {
        val headers = linkedMapOf(
            "User-Agent" to UA,
            "Accept" to "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8",
            "Accept-Language" to "ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7",
            "Referer" to "https://account.apple.com/",
        )
        cookieHeader(false)?.let { headers["Cookie"] = it }
        val builder = Request.Builder()
            .url("https://account.apple.com/account/manage/section/information")
        headers.forEach { (k, v) -> builder.header(k, v) }
        runCatching {
            Http.apple.newCall(builder.get().build()).execute().use { resp ->
                resp.body?.bytes()
                storeSetCookies(resp.headers("Set-Cookie"))
            }
        }
    }

    private fun refreshAccountCookies() {
        runCatching {
            primeAccountPage()
            get("https://account.apple.com/bootstrap/portal")
            get("https://appleid.apple.com/account/manage")
            get("$BUY/account/web/infoRefresh")
            get("$BUY/account/web/info")
        }
    }

    private fun ingestGsBody(text: String) {
        runCatching {
            val o = JSONObject(text)
            // itspod приходит телом, а не кукой, но дальше требуется именно как кука.
            o.optString("itspod").takeIf { it.isNotBlank() }?.let { cookies["itspod"] = it }
        }
    }

    // ------------------------------------------------------------------ запросы

    /** Трёхбуквенный код страны аккаунта прямо сейчас. */
    fun currentCountry(): String {
        val fields = get("$BUY/account/v1/editAccountFields/get?context=changeCountry")
        if (fields.status == 401 || fields.status == 403 || looksLikeAuthDialog(fields.body)) {
            error(SESSION_MESSAGE)
        }
        countryOf(fields.body)?.let { return it }
        val info = get("$BUY/account/web/info")
        if (info.status == 401 || info.status == 403 || looksLikeAuthDialog(info.body)) {
            error(SESSION_MESSAGE)
        }
        return countryOf(info.body) ?: error("Не удалось узнать текущую страну аккаунта.")
    }

    /**
     * Полный текущий адрес — то, что надо будет вернуть обратно. Если Apple отдала
     * его не полностью, менять страну нельзя: вернуть человека будет некуда.
     */
    fun currentAddress(names: (String) -> String): RegionAddress {
        val fields = get("$BUY/account/v1/editAccountFields/get?context=changeCountry")
        if (fields.status == 401 || fields.status == 403 || looksLikeAuthDialog(fields.body)) {
            error(SESSION_MESSAGE)
        }
        val root = runCatching { JSONObject(fields.body) }.getOrNull()
            ?: error("Apple не вернул текущий платёжный адрес. Повторите попытку.")
        val address = addressInfo(root) ?: root
        val code = (
            address.optString("addressOfficialCountryCode").trim().ifEmpty {
                address.optString("iso3CountryCode").trim()
            }.ifEmpty { countryOf(fields.body).orEmpty() }
            ).uppercase()
        val street = address.optString("addressOfficialLineFirst").trim()
        val city = address.optString("addressOfficialCity").trim()
        val postal = address.optString("addressOfficialPostalCode").trim()
        if (code.length != 3 || street.isEmpty() || city.isEmpty() || postal.isEmpty()) {
            error(
                "Apple не вернул полный текущий адрес. Смена страны отменена, " +
                    "страна аккаунта не изменилась.",
            )
        }
        return RegionAddress(
            countryCode = code,
            countryName = names(code),
            street = street,
            city = city,
            postal = postal,
            state = address.optString("addressOfficialStateProvince").trim().takeIf { it.isNotEmpty() },
            county = address.optString("addressOfficialCounty").trim().takeIf { it.isNotEmpty() },
            phoneArea = address.optString("phoneOfficeAreaCode").trim().ifEmpty { "000" },
            phoneNumber = address.optString("phoneOfficeNumber").trim().ifEmpty { "00000000" },
        )
    }

    /**
     * Переносит аккаунт на страну адреса. Возвращает код страны, которая стоит после
     * попытки: успешный ответ Apple ещё ничего не значит, она отвечает «готово» и
     * на адрес, который потом сама же отбрасывает.
     */
    fun changeTo(address: RegionAddress): String {
        val cc = address.countryCode.uppercase()

        val current = get("$BUY/account/v1/editAccountFields/get?context=changeCountry")
        if (current.status == 401 || current.status == 403 || looksLikeAuthDialog(current.body)) {
            error(SESSION_MESSAGE)
        }
        val before = countryOf(current.body).orEmpty()
        if (before.equals(cc, true)) return cc

        val (first, last) = billingNames(current.body)

        // Порядок шагов Apple не проверяет по одному, но пропуск согласия с условиями
        // роняет сохранение адреса, поэтому идём как страница смены страны.
        get("$BUY/account/v1/editAccountFields/get?context=changeCountry&iso3CountryCode=$cc&isVideoEverywhere=true")
        get("$BUY/account/v2/changeCountry/validate?iso3CountryCode=$cc")

        val terms = get("$AMP/account/v1/compliance/terms/info?termsType=AMS&cc=$cc&context=changeCountry")
        if (terms.status == 401 || terms.status == 403) error(SESSION_MESSAGE)
        val versionId = versionIdOf(terms.body) ?: FALLBACK_TERMS_VERSION
        get("$AMP/account/v1/compliance/terms/all?cc=$cc&context=changeCountry")
        val consent = post(
            "$AMP/account/v1/compliance/terms/updateConsent?termsType=AMS&cc=$cc&context=changeCountry",
            JSONObject().put("hasConsented", true).put("versionId", versionId).toString(),
        )
        if (consent.status == 401 || consent.status == 403) error(SESSION_MESSAGE)

        val payload = JSONObject()
            .put("paymentMethodType", "None")
            .put("billingFirstName", first)
            .put("billingLastName", last)
            .put("addressOfficialLineFirst", address.street)
            .put("addressOfficialCity", address.city)
            .put("addressOfficialPostalCode", address.postal)
            .put("iso3CountryCode", cc)
            .put("addressOfficialCountryCode", cc)
            .put("phoneOfficeAreaCode", address.phoneArea)
            .put("phoneOfficeNumber", address.phoneNumber)
        address.state?.trim()?.takeIf { it.isNotEmpty() }
            ?.let { payload.put("addressOfficialStateProvince", it) }
        address.county?.trim()?.takeIf { it.isNotEmpty() }
            ?.let { payload.put("addressOfficialCounty", it) }

        var lastError = "Не удалось сменить страну. Подождите минуту и попробуйте снова."
        for (attempt in 0..1) {
            if (attempt > 0) Thread.sleep(1600)
            val saved = post("$BUY/account/v1/nonstackable/paymentInfos/edit?", payload.toString())
            if (saved.status == 401 || saved.status == 403 || looksLikeAuthDialog(saved.body)) {
                error(SESSION_MESSAGE)
            }
            if (saved.status >= 400 || appleRejected(saved.body)) {
                lastError = humanAppleError(saved.body)
                continue
            }
            break
        }

        val after = runCatching { currentCountry() }.getOrDefault("")
        if (after.equals(cc, true)) return cc
        if (after.isNotEmpty() && !after.equals(before, true)) return after
        error(lastError)
    }

    /**
     * Подтверждает обновлённые условия Apple. Без этого установка приложений
     * иногда упирается в требование «примите условия» без объяснений.
     */
    fun acceptMediaTerms() {
        val cc = runCatching { currentCountry() }.getOrDefault("RUS")
        val info = get("$AMP/account/v1/compliance/terms/info?termsType=AMS&cc=$cc")
        val versionId = versionIdOf(info.body) ?: FALLBACK_MEDIA_VERSION
        post(
            "$AMP/account/v1/compliance/terms/updateConsent?termsType=AMS&cc=$cc",
            JSONObject().put("hasConsented", true).put("versionId", versionId).toString(),
        )
    }

    private fun billingNames(fieldsJson: String): Pair<String, String> {
        val root = runCatching { JSONObject(fieldsJson) }.getOrNull()
        val addr = root?.let { addressInfo(it) }
        val info = root
            ?.optJSONObject("data")?.optJSONObject("attributes")
            ?.optJSONObject("accountInfo")?.optJSONObject("accountInfo")
        val first = addr?.optString("billingFirstName")?.trim()?.ifEmpty { null }
            ?: info?.optString("firstName")?.trim().orEmpty()
        val last = addr?.optString("billingLastName")?.trim()?.ifEmpty { null }
            ?: info?.optString("lastName")?.trim().orEmpty()
        return latinName(first, "John") to latinName(last, "Smith")
    }

    private fun addressInfo(root: JSONObject): JSONObject? = root
        .optJSONObject("data")?.optJSONObject("attributes")
        ?.optJSONObject("accountInfo")?.optJSONObject("addressInfo")

    companion object {
        private const val BUY = "https://buy.apps.apple.com"
        private const val AMP = "https://amp-account.apps.apple.com"
        private const val AUTH_GS = "https://auth.apps.apple.com/auth/v1/gs"
        private const val RUS_FRONT = "143469-1,8"
        private const val USA_FRONT = "143441-1,8"
        private const val FALLBACK_TERMS_VERSION = 289469
        private const val FALLBACK_MEDIA_VERSION = 289404
        private const val UA =
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36"

        /** Разрешение веб-плеера Apple: платёжные хосты без него отвечают отказом. */
        private const val AMP_BEARER =
            "Bearer eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjU1VDNUMjM4UTQifQ" +
                ".eyJpc3MiOiJBTVBXZWJQbGF5IiwiaWF0IjoxNzgyODY0MjQyLCJleHAiOjE3OTg0MTYyNDIsInJvb3RfaHR0cHNfb3JpZ2luIjpbImFwcGxlLmNvbSIsIm16c3RhdGljLmNvbSJdfQ" +
                ".NWIMDTslEElpGnFqe_in3257-gsnrSGONKs1fK6sPHkVyrOoBRqUoOtk25GtsT3urf3r-tvlPOGF6OLAB0_bzw"

        internal const val SESSION_MESSAGE =
            "Вход в Apple ID устарел. Откройте «Аккаунт», войдите заново и повторите смену страны."

        private val JSON_MEDIA = "application/json".toMediaType()

        /**
         * Платёжные хосты глохнут, если прислать им весь набор кук входа: часть из них
         * их же собственная страница туда не отправляет. Оставляем ровно те, что нужны.
         */
        private val COMMERCE_COOKIES = setOf(
            "acn01",
            "caw",
            "caw-at",
            "commerce-authorization-token",
            "commerce-authorization-token-lite",
            "commerce-ds-session-id",
            "commerce-on-web-token",
            "dslang",
            "geo",
            "itspod",
            "myacinfo",
            "site",
        )

        private fun storeFrontFor(site: String?): String? =
            when (site?.trim()?.uppercase()) {
                "USA", "US" -> USA_FRONT
                "RUS", "RU" -> RUS_FRONT
                else -> null
            }

        /**
         * Apple иногда пакует несколько кук в одну строку через запятую. Режем только
         * по тем запятым, за которыми начинается новое имя куки, — внутри дат запятые тоже есть.
         */
        private fun splitPacked(raw: String): List<String> {
            val out = mutableListOf<String>()
            var start = 0
            var i = 0
            while (i < raw.length) {
                if (raw[i] == ',') {
                    val rest = raw.substring(i + 1).trimStart()
                    val eq = rest.indexOf('=')
                    if (eq > 0) {
                        val name = rest.substring(0, eq).trim()
                        val plain = name.isNotEmpty() && name.all {
                            it.isLetterOrDigit() && it.code < 128 || it == '_' || it == '-'
                        }
                        if (plain && name.lowercase() !in COOKIE_ATTRS) {
                            out += raw.substring(start, i).trim()
                            start = i + 1
                        }
                    }
                }
                i++
            }
            raw.substring(start).trim().takeIf { it.isNotEmpty() }?.let { out += it }
            return out.ifEmpty { listOf(raw) }
        }

        private val COOKIE_ATTRS = setOf(
            "expires", "max-age", "path", "domain", "secure", "httponly", "samesite",
        )

        private fun resolveUrl(base: String, location: String): String = when {
            location.startsWith("http://") || location.startsWith("https://") -> location
            location.startsWith("//") -> "https:$location"
            location.startsWith("/") -> {
                val uri = java.net.URI(base)
                "${uri.scheme}://${uri.host}$location"
            }
            else -> location
        }

        /** Имя в платёжной форме Apple принимает только латиницей. */
        private fun latinName(raw: String, fallback: String): String {
            val t = raw.trim()
            if (t.isEmpty()) return fallback
            val plain = t.all {
                (it in 'a'..'z') || (it in 'A'..'Z') || (it in '0'..'9') ||
                    it == ' ' || it == '-' || it == '\''
            }
            return if (plain) t.take(32) else fallback
        }

        private fun countryOf(body: String): String? {
            for (key in listOf(
                "iTunesStoreFrontCountryCode",
                "storeFrontISO3A",
                "addressOfficialCountryCode",
            )) {
                val v = quotedValue(body, key) ?: continue
                if (v.length == 3 && v.all { it.isLetter() }) return v.uppercase()
            }
            return null
        }

        /**
         * Код страны лежит на разной глубине в зависимости от запроса, поэтому берём
         * его текстом по имени ключа, а не разбором всего дерева.
         */
        private fun quotedValue(body: String, key: String): String? {
            val at = body.indexOf("\"$key\"")
            if (at < 0) return null
            var rest = body.substring(at + key.length + 2)
            val colon = rest.indexOf(':')
            if (colon < 0) return null
            rest = rest.substring(colon + 1).trimStart()
            if (!rest.startsWith("\"")) return null
            rest = rest.substring(1)
            val end = rest.indexOf('"')
            return if (end < 0) null else rest.substring(0, end)
        }

        private fun versionIdOf(body: String): Int? {
            runCatching {
                val o = JSONObject(body)
                o.optInt("versionId", 0).takeIf { it > 0 }?.let { return it }
                o.optJSONObject("data")?.optJSONObject("attributes")
                    ?.optInt("versionId", 0)?.takeIf { it > 0 }?.let { return it }
            }
            val at = body.indexOf("\"versionId\"")
            if (at < 0) return null
            val rest = body.substring(at + 11).trimStart().removePrefix(":").trimStart()
            return rest.takeWhile { it.isDigit() }.toIntOrNull()
        }

        private fun looksLikeAuthDialog(body: String): Boolean {
            val l = body.lowercase()
            return "signin required" in l || "authrequired" in l || "\"kind\":\"authorization\"" in l
        }

        /**
         * Apple отвечает 200 и на неудачу, а причину кладёт внутрь тела на любой глубине.
         * Поэтому ответ приходится обходить целиком.
         */
        private fun appleRejected(body: String): Boolean {
            if (body.isBlank()) return false
            val value = runCatching { JSONObject(body) }.getOrNull()
                ?: return "\"status\":-1" in body
            return hasError(value)
        }

        private fun hasError(value: Any?): Boolean = when (value) {
            is JSONObject -> {
                val status = value.opt("status")
                val bad = (status is Number && status.toInt() < 0) ||
                    (value.opt("success") as? Boolean == false) ||
                    ERROR_KEYS.any { key ->
                        val entry = value.opt(key)
                        entry != null && entry != JSONObject.NULL &&
                            (entry !is JSONArray || entry.length() > 0)
                    }
                bad || value.keys().asSequence().any { hasError(value.opt(it)) }
            }
            is JSONArray -> (0 until value.length()).any { hasError(value.opt(it)) }
            else -> false
        }

        private val ERROR_KEYS = listOf(
            "errors", "serviceErrors", "validationErrors", "service_errors",
            "error", "errorMessage", "errorCode",
        )

        private val TEXT_KEYS = setOf(
            "message", "errormessage", "localizeddescription", "description",
            "detail", "reason", "code", "errorcode", "errortype", "statusmessage",
        )

        private fun collectText(value: Any?, into: MutableSet<String>) {
            when (value) {
                is JSONObject -> value.keys().forEach { key ->
                    val item = value.opt(key)
                    if (key.lowercase() in TEXT_KEYS && item is String && item.isNotBlank()) {
                        into += item.trim().lowercase()
                    }
                    collectText(item, into)
                }
                is JSONArray -> (0 until value.length()).forEach { collectText(value.opt(it), into) }
            }
        }

        /**
         * Причины отказа Apple присылает по-английски и техническим языком.
         * Переводим их в понятное человеку действие.
         */
        private fun humanAppleError(body: String): String {
            val texts = sortedSetOf<String>()
            runCatching { collectText(JSONObject(body), texts) }
            val hints = if (texts.isEmpty()) body.lowercase() else texts.joinToString(" ")
            for ((needles, message) in ERROR_HINTS) {
                if (needles.any { it in hints }) return message
            }
            return "Не удалось сменить страну. Подождите минуту и попробуйте снова."
        }

        private val ERROR_HINTS: List<Pair<List<String>, String>> = listOf(
            listOf(
                "account balance", "apple account balance", "credit balance", "store credit",
                "remaining balance", "remaining credit", "gift card balance", "остаток средств",
            ) to "Не удалось сменить страну: на балансе Apple ID остались средства. " +
                "Потратьте их и повторите. Если остатка не хватает ни на одну покупку, " +
                "обратитесь в поддержку Apple.",
            listOf("family sharing", "family group", "семейн") to
                "Не удалось сменить страну: Apple ID состоит в семейной группе. Выйдите из неё и повторите.",
            listOf("subscription", "apple one", "membership", "подписк") to
                "Не удалось сменить страну: сначала отмените действующие подписки и дождитесь окончания их периода.",
            listOf(
                "pre-order", "preorder", "pre order", "movie rental", "season pass",
                "pending purchase", "pending refund", "pending transaction", "rental", "refund",
                "предзаказ", "возврат", "прокат",
            ) to "Не удалось сменить страну: дождитесь завершения предзаказа, проката, покупки " +
                "или возврата средств и повторите.",
            listOf(
                "payment verification", "payment method", "payment information", "billing address",
                "credit card", "debit card", "payment required", "card required", "billing",
                "платеж", "карт",
            ) to "Для выбранной страны Apple просит подходящий способ оплаты и платёжный адрес. " +
                "Проверьте их в Apple ID и повторите.",
            listOf(
                "postal", "post code", "postcode", "zip code", "zip", "state/province",
                "address invalid", "индекс",
            ) to "Apple не принял адрес для выбранной страны. Выберите другой адрес и повторите.",
            listOf("terms", "consent", "agreement", "услови", "соглашен") to
                "Apple просит подтвердить условия для выбранной страны. Подтвердите их в Apple ID и повторите.",
            listOf(
                "try again", "later", "temporarily", "unavailable", "rate limit", "too many",
                "timeout", "network", "connection", "service error", "временно", "попробуйте позже",
            ) to "Сейчас Apple временно не принимает смену страны. Подождите пару минут и попробуйте снова.",
        )
    }
}
