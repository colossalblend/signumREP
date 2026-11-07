package pw.appbank.signum.data.api

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import org.json.JSONObject
import pw.appbank.signum.data.models.APP_BUILD
import pw.appbank.signum.data.models.APP_VERSION
import pw.appbank.signum.data.models.DEFAULT_API
import java.util.concurrent.ConcurrentHashMap

object AutoLog {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val recent = ConcurrentHashMap<String, Long>()
    private const val DEDUP_MS = 15 * 60 * 1000L
    private const val MAX_PER_HOUR = 30

    fun report(source: String, message: String, detail: String? = null) {
        val shown = redact(message).trim()
        if (skip(shown)) return
        val extra = detail?.let(::redact).orEmpty().let { text ->
            if (text.isBlank() || text == shown) "" else text.take(4000)
        }
        // Headline is an English code, matching the desktop clients: it reads
        // the same for everyone and stays identical across occurrences, so the
        // hourly digest groups the fault instead of listing it N times.
        val (code, english) = classify("$shown\n$extra")
        val headline = "[$code] $english"
        val now = System.currentTimeMillis()
        recent.entries.removeIf { now - it.value > 60 * 60 * 1000L }
        val key = "$source|$code"
        val previous = recent[key]
        if (previous != null && now - previous < DEDUP_MS) return
        if (recent.size >= MAX_PER_HOUR) return
        recent[key] = now
        val log = buildString {
            if (extra.isNotBlank()) append("raw:\n").append(extra).append("\n\n")
            append("ui-ru:\n").append(shown)
        }
        scope.launch {
            runCatching { post(source, headline.take(800), log.take(4000)) }
        }
    }

    /** Stable English label for a fault, matched on raw output and Russian UI text. */
    fun classify(text: String): Pair<String, String> {
        val t = text.lowercase()
        fun has(vararg needles: String) = needles.any { it in t }
        return when {
            has("could not be found in the keyring", "errkeynotfound") ->
                "KEYRING_ITEM_MISSING" to "the saved Apple ID session is missing from the local store"
            has("keyunwrap", "integrity check failed", "illegal base64") ->
                "KEYRING_DECRYPT_FAILED" to "the stored session cannot be decrypted with the current key"
            has("not logged in", "вход в apple id не сохранился") ->
                "NOT_LOGGED_IN" to "no active Apple ID session"
            has("password token", "устарела") ->
                "PASSWORD_TOKEN_EXPIRED" to "the App Store passwordToken expired: a fresh sign-in is required"
            has("sapsigner", "sap helper", "actionsignature") ->
                "SAP_SIGNER_FAILED" to "the SAP signer failed to produce X-Apple-ActionSignature"
            has("anisette", "x-apple-i-md") ->
                "ANISETTE_UNAVAILABLE" to "anisette headers could not be produced"
            has("неверный пароль", "bad login") ->
                "BAD_PASSWORD" to "Apple rejected the Apple ID password"
            has("empty songlist", "нет в покупках", "нет доступных версий") ->
                "NOT_PURCHASED" to "this build is not in the Apple ID purchase history"
            has("account not in store", "accountnotinstore") ->
                "STOREFRONT_MISMATCH" to "the Apple ID is not attached to this storefront"
            has("iphone не найден", "device not found", "no device") ->
                "DEVICE_NOT_FOUND" to "no iPhone/iPad reachable over USB"
            has("не удалось установить доверие", "pairing", "trust") ->
                "DEVICE_TRUST_FAILED" to "pairing failed: the Trust prompt was not confirmed"
            has("applicationverificationfailed", "при проверке") ->
                "APP_VERIFICATION_FAILED" to "the device refused the signed app"
            has("timeout", "timed out", "не ответил вовремя") ->
                "NETWORK_TIMEOUT" to "the request timed out"
            has("unable to resolve host", "connection reset", "не удалось подключиться") ->
                "NETWORK_UNREACHABLE" to "the service could not be reached"
            has("не удалось выполнить поиск") ->
                "SEARCH_FAILED" to "the App Store search request failed"
            else -> "UNCLASSIFIED" to "no matching rule — see the raw output below"
        }
    }

    private fun skip(message: String): Boolean {
        if (message.isBlank()) return true
        val lower = message.lowercase()
        if ("отменено" in lower || "cancelled" in lower) return true
        if ("signum_2fa_required" in lower) return true
        if ("введите код" in lower || ("нужен код" in lower && "iphone" in lower)) return true
        return false
    }

    private fun redact(input: String): String {
        var text = input
        text = text.replace(Regex("(?i)[a-z0-9._%+\\-]+@[a-z0-9.\\-]+\\.[a-z]{2,}"), "[email]")
        text = text.replace(Regex("(?i)(bearer\\s+|token[=:]\\s*|passwordtoken[=:]\\s*|cookie[=:]\\s*)[^\\s,&]+"), "$1[hidden]")
        text = text.replace(Regex("\\b\\d{6}\\b"), "[code]")
        text = text.replace(Regex("(?i)([A-Z]:\\\\Users\\\\)[^\\\\]+"), "$1[user]")
        text = text.replace(Regex("(/(?:Users|home)/)[^/\\s]+"), "$1[user]")
        text = text.replace(Regex("\\b[0-9a-fA-F]{40}\\b"), "[id]")
        return text
    }

    private fun post(source: String, message: String, log: String) {
        val url = "${DEFAULT_API.trimEnd('/')}/logs.php".toHttpUrlOrNull() ?: return
        if (url.host != Hx.apiHost()) return
        val body = JSONObject()
            .put("kind", "auto")
            .put("source", source.take(32))
            .put("platform", "android")
            .put("version", APP_VERSION)
            .put("build", APP_BUILD)
            .put("message", message)
            .put("log", log)
            .toString()
        val req = Request.Builder()
            .url(url)
            .post(body.toRequestBody("application/json; charset=utf-8".toMediaType()))
            .header("Accept", "application/json")
            .build()
        Http.api.newCall(req).execute().close()
    }
}
