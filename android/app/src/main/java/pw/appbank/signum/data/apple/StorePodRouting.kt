package pw.appbank.signum.data.apple

import java.io.IOException
import java.net.URI
import java.net.UnknownHostException

/** Android adaptation of ipatool 1.1.8's stale-pod DNS and redirect handling. */
object StorePodRouting {
    const val UPSTREAM_VERSION = "1.1.8"
    private val podHost = Regex("""p(\d+)-buy\.itunes\.apple\.com""", RegexOption.IGNORE_CASE)

    data class Reply<T>(val status: Int, val value: T, val location: String? = null, val pod: String? = null)

    fun validPod(value: String?): String? = value?.trim()?.takeIf {
        it.length in 1..5 && it.all { c -> c in '0'..'9' }
    }

    fun podFromUrl(url: String): String? {
        val uri = URI(url)
        return validPod(podHost.matchEntire(uri.host.orEmpty())?.groupValues?.get(1))
    }

    fun withoutPod(url: String): String? {
        val uri = URI(url)
        if (podFromUrl(url) == null || !uri.scheme.equals("https", true)) return null
        return "https://buy.itunes.apple.com" + uri.rawPath.orEmpty() +
            (uri.rawQuery?.let { "?$it" } ?: "")
    }

    fun redirectUrl(base: String, location: String): String {
        val loc = location.trim()
        val resolved = if (loc.startsWith("?")) base.substringBefore('#').substringBefore('?') + loc
            else URI(base).resolve(loc).toASCIIString()
        validateUrl(resolved)
        return resolved
    }

    private fun validateUrl(url: String) {
        val uri = URI(url)
        val host = uri.host.orEmpty().lowercase()
        require(uri.scheme.equals("https", true) && uri.userInfo == null &&
            (uri.port == -1 || uri.port == 443) && (host == "apple.com" || host.endsWith(".apple.com"))) {
            "Apple вернула некорректный адрес сервера App Store."
        }
    }

    fun <T> execute(
        startUrl: String,
        send: (String) -> Reply<T>,
        onPod: (String) -> Unit,
        resolveRedirect: (String, String) -> String = ::redirectUrl,
    ): T {
        var url = startUrl
        var usedDnsFallback = false
        repeat(6) {
            validateUrl(url)
            val reply = try {
                send(url)
            } catch (e: UnknownHostException) {
                val fallback = if (!usedDnsFallback) withoutPod(url) else null
                if (fallback == null) throw e
                usedDnsFallback = true
                url = fallback
                return@repeat
            }
            val explicitPod = validPod(reply.pod)
            if (reply.status in listOf(301, 302, 303, 307, 308)) {
                val location = reply.location?.takeIf { it.isNotBlank() }
                    ?: throw IOException("Apple не указала адрес следующего сервера App Store.")
                // Follow Apple's complete Location, including handshake query
                // parameters such as mzcprh. Rebuilding from pod loses them.
                val next = resolveRedirect(url, location)
                validateUrl(next)
                (explicitPod ?: podFromUrl(next))?.let(onPod)
                url = next
            } else {
                (explicitPod ?: podFromUrl(url))?.let(onPod)
                return reply.value
            }
        }
        throw IOException("App Store слишком много раз перенаправил запрос. Повторите позже.")
    }
}
