package pw.appbank.signum.data.apple

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import pw.appbank.signum.data.api.DiagLog
import pw.appbank.signum.data.api.Http

/**
 * Альтернативный способ установки — через временный Apple ID банка.
 *
 * Банк выдаёт по SMS логин и пароль от своего Apple ID (живут около получаса)
 * и ссылку на установку. На ПК это работает давно; на Android кнопка была, но
 * за ней стояло окно «сейчас этот способ недоступен» — то есть заглушка.
 *
 * На телефоне выходит даже проще, чем на ПК: там ссылку приходится передавать
 * на iPhone через QR и свой мини-сервер, а здесь iPhone уже подключён кабелем,
 * и файл ставится тем же путём, что и всё остальное.
 */
object BankOtp {

    enum class Bank(val key: String, val displayName: String, val source: String) {
        Sovcombank("sovcombank", "Совкомбанк", "sovcom"),
        Rocketbank("rocketbank", "Рокетбанк", "rocket"),
        ;

        companion object {
            fun parse(raw: String): Bank? {
                val v = raw.trim().lowercase()
                return entries.firstOrNull { it.key == v }
            }
        }
    }

    /** Тройка адресов одного узла банка. Internal: Session её показывает наружу. */
    internal data class Host(val sendSms: String, val validate: String, val credentials: String)

    internal data class Config(
        val origin: String,
        val refererSend: String,
        val refererValidate: String,
        val refererCreds: String,
        val hosts: List<Host>,
    )

    internal fun config(bank: Bank): Config = when (bank) {
        Bank.Sovcombank -> Config(
            origin = "https://manual.sovcombank.ru",
            refererSend = "https://manual.sovcombank.ru/manual",
            refererValidate = "https://manual.sovcombank.ru/manual/verify",
            refererCreds = "https://manual.sovcombank.ru/manual/manual",
            hosts = listOf(
                Host(
                    "https://manual.sovcombank.ru/rest/auth/phone/sendsmscode",
                    "https://manual.sovcombank.ru/rest/auth/phone/validate",
                    "https://manual.sovcombank.ru/rest/api/v2/constants/auth/iphone/installation/credentials",
                ),
            ),
        )
        Bank.Rocketbank -> Config(
            origin = "https://rocketbank.ru",
            refererSend = "https://rocketbank.ru/ios/manual",
            refererValidate = "https://rocketbank.ru/ios/manual/verify",
            refererCreds = "https://rocketbank.ru/ios/manual/verify",
            // Два узла: первый отвечает быстрее, второй остаётся запасным.
            hosts = listOf(
                Host(
                    "https://api-app.sovcombank.ru/v1/phone/sendsmscode",
                    "https://api-app.sovcombank.ru/v1/phone/validate",
                    "https://api-app.sovcombank.ru/v2/constants/auth/iphone/installation/credentials?soft=rocket",
                ),
                Host(
                    "https://rocketbank.ru/ios/manual/rest/auth/phone/sendsmscode",
                    "https://rocketbank.ru/ios/manual/rest/auth/phone/validate",
                    "https://rocketbank.ru/ios/manual/rest/api/v2/constants/auth/iphone/installation/credentials?soft=rocket",
                ),
            ),
        )
    }

    private const val UA =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36"

    /**
     * Сессия банка. Создать её можно только через sendSms: собранная вручную
     * не имеет ни узла, ни токена, и все следующие шаги на ней падают.
     */
    class Session internal constructor(
        val bank: Bank,
        val phone: String,
        internal val host: Host,
        internal var smsToken: String? = null,
        internal var authToken: String? = null,
        internal var tokenWithData: String? = null,
    )

    data class Credentials(
        val login: String,
        val password: String,
        val link: String?,
        val bundleId: String?,
        val appStoreId: String?,
    )

    /** Телефон к виду 79991234567. Банк другого не понимает. */
    fun normalizePhone(raw: String): String {
        var digits = raw.filter { it.isDigit() }
        if (digits.length == 11 && digits.startsWith("8")) digits = "7" + digits.substring(1)
        if (digits.length == 10) digits = "7$digits"
        require(digits.length == 11 && digits.startsWith("7")) {
            "Введите номер телефона в виде +7 999 123-45-67"
        }
        return digits
    }

    fun formatPhoneDisplay(phone: String): String =
        if (phone.length == 11) {
            "+7 ${phone.substring(1, 4)} ${phone.substring(4, 7)}-${phone.substring(7, 9)}-${phone.substring(9)}"
        } else {
            phone
        }

    /** Шаг 1: попросить банк прислать SMS. */
    fun sendSms(bank: Bank, phoneRaw: String): Pair<Session, String> {
        val phone = normalizePhone(phoneRaw)
        val cfg = config(bank)
        val body = JSONObject().put("phone", phone).put("source", bank.source).toString()
        var lastMessage = ""
        for (host in cfg.hosts) {
            val (code, json, raw) = post(
                host.sendSms,
                body,
                mapOf("Origin" to cfg.origin, "Referer" to cfg.refererSend),
            )
            lastMessage = message(json) ?: raw.take(200)
            val token = json?.optString("token")?.trim().orEmpty()
            if (code in 200..299 && token.isNotEmpty()) {
                DiagLog.note("банк", "SMS отправлена, банк ${bank.key}")
                return Session(bank, phone, host, smsToken = token) to
                    (message(json) ?: "Код отправлен на ${formatPhoneDisplay(phone)}")
            }
        }
        error(lastMessage.ifBlank { "Банк не принял номер. Проверьте телефон и повторите." })
    }

    /** Шаг 2: подтвердить код из SMS. */
    fun validate(session: Session, codeRaw: String): String {
        val token = session.smsToken ?: error("Сначала отправьте код по SMS.")
        val code = codeRaw.filter { it.isDigit() }
        require(code.length >= 4) { "Введите код из SMS." }
        val cfg = config(session.bank)

        val body = JSONObject().put("code", code).put("token", token).apply {
            if (session.bank == Bank.Sovcombank) put("phone", session.phone)
            else put("source", session.bank.source)
        }.toString()

        val headers = buildMap {
            put("Origin", cfg.origin)
            put("Referer", cfg.refererValidate)
            // Рокет ждёт уже полученный токен в заголовке, Совком — нет.
            if (session.bank == Bank.Rocketbank) put("authorization", token)
        }

        val (code2, json, raw) = post(session.host.validate, body, headers)
        if (code2 !in 200..299) {
            error(message(json) ?: "Код не принят. Запросите новый и повторите. ${raw.take(120)}")
        }
        // tokenWithData важнее остальных: с ним банк отдаёт учётку сразу.
        var twd: String? = null
        for (key in listOf("tokenWithData", "token", "accessToken")) {
            val v = json?.optString(key)?.trim().orEmpty()
            if (v.isNotEmpty()) {
                twd = v
                if (key == "tokenWithData") break
            }
        }
        session.tokenWithData = twd
        session.authToken = if (session.bank == Bank.Rocketbank) token else (twd ?: token)
        return session.authToken.orEmpty()
    }

    /** Шаг 3: забрать логин, пароль и ссылку на установку. */
    fun credentials(session: Session): Credentials {
        val cfg = config(session.bank)
        // Какой из токенов подойдёт, банк решает сам — перебираем по порядку,
        // который на ПК проверен для каждого банка отдельно.
        val candidates = when (session.bank) {
            Bank.Rocketbank -> listOf(session.smsToken, session.authToken, session.tokenWithData)
            Bank.Sovcombank -> listOf(session.authToken, session.tokenWithData, session.smsToken)
        }.mapNotNull { it?.trim()?.takeIf { t -> t.isNotEmpty() } }.distinct()
        if (candidates.isEmpty()) error("Нет доступа после SMS. Отправьте код заново.")

        var lastMessage = ""
        for (token in candidates) {
            val (code, json, raw) = get(
                session.host.credentials,
                mapOf(
                    "authorization" to token,
                    "Origin" to cfg.origin,
                    "Referer" to cfg.refererCreds,
                ),
            )
            lastMessage = message(json) ?: raw.take(200)
            if (code != 200 || json == null) continue
            val login = json.optString("login").trim()
            val password = json.optString("password").trim()
            if (login.isEmpty() || password.isEmpty()) continue
            val link = json.optString("link").trim().takeIf { it.isNotEmpty() }
            val bundleId = listOf("bundleId", "bundle_id", "bundleIdentifier")
                .firstNotNullOfOrNull { json.optString(it).trim().takeIf { v -> v.isNotEmpty() } }
            DiagLog.note("банк", "учётка получена, банк ${session.bank.key}")
            return Credentials(login, password, link, bundleId, adamFromLink(link))
        }
        error(lastMessage.ifBlank { "Банк не отдал учётную запись. Попробуйте позже." })
    }

    /** Номер приложения из ссылки вида …/id123456789. */
    private fun adamFromLink(link: String?): String? {
        val l = link ?: return null
        return Regex("""[/?&]id(\d{5,12})""").find(l)?.groupValues?.get(1)
    }

    private fun message(json: JSONObject?): String? {
        val j = json ?: return null
        for (key in listOf("message", "errorMessage", "description", "error")) {
            val v = j.optString(key).trim()
            if (v.isNotEmpty()) return v
        }
        return null
    }

    private fun post(url: String, body: String, headers: Map<String, String>) =
        call(Request.Builder().url(url).post(body.toRequestBody(JSON)), headers)

    private fun get(url: String, headers: Map<String, String>) =
        call(Request.Builder().url(url).get(), headers)

    private val JSON = "application/json; charset=utf-8".toMediaType()

    private fun call(
        builder: Request.Builder,
        headers: Map<String, String>,
    ): Triple<Int, JSONObject?, String> {
        builder.header("User-Agent", UA).header("Accept", "application/json, text/plain, */*")
        headers.forEach { (k, v) -> builder.header(k, v) }
        return Http.apple.newCall(builder.build()).execute().use { resp ->
            val raw = resp.body?.string().orEmpty()
            val json = runCatching { JSONObject(raw) }.getOrNull()
            Triple(resp.code, json, raw)
        }
    }
}
