package pw.appbank.signum.data.api

import okhttp3.Authenticator
import okhttp3.Credentials
import okhttp3.OkHttpClient
import okhttp3.Route
import java.net.InetSocketAddress
import java.net.Proxy
import pw.appbank.signum.data.models.AppSettings

/**
 * Прокси для обращений к Apple.
 *
 * Нужен там, где Apple недоступна напрямую. На телефоне его не было вовсе:
 * все клиенты создавались без прокси, и человеку оставалось только включать
 * VPN на весь телефон.
 *
 * Клиенты OkHttp создаются один раз при запуске, поэтому настройка не может
 * просто «примениться» — её надо принять и пересобрать клиентов. Этим
 * занимается [apply].
 */
object ProxyConfig {

    @Volatile
    private var current: Settings? = null

    data class Settings(
        val kind: String,
        val host: String,
        val port: Int,
        val user: String,
        val password: String,
    )

    /** Запомнить настройку. Возвращает true, если она изменилась. */
    fun apply(settings: AppSettings): Boolean {
        val next = if (!settings.proxyEnabled ||
            settings.proxyHost.isBlank() ||
            settings.proxyPort <= 0
        ) {
            null
        } else {
            Settings(
                kind = settings.proxyKind.trim().lowercase(),
                host = settings.proxyHost.trim(),
                port = settings.proxyPort,
                user = settings.proxyUser.trim(),
                password = settings.proxyPassword,
            )
        }
        if (next == current) return false
        current = next
        return true
    }

    /**
     * Надеть прокси на клиента.
     *
     * SOCKS выбираем типом [Proxy.Type.SOCKS] — тогда имя узла разрешает сам
     * прокси, а не телефон. Это важно: ради обхода блокировки по имени прокси
     * обычно и включают, а разрешение имени на телефоне свело бы смысл на нет.
     */
    fun wrap(builder: OkHttpClient.Builder): OkHttpClient.Builder {
        val settings = current ?: return builder
        val type = if (settings.kind.startsWith("http")) Proxy.Type.HTTP else Proxy.Type.SOCKS
        builder.proxy(Proxy(type, InetSocketAddress.createUnresolved(settings.host, settings.port)))

        if (settings.user.isNotEmpty()) {
            val credential = Credentials.basic(settings.user, settings.password)
            // Пароль спрашивают оба: обычный прокси и туннель для HTTPS.
            val authenticator = Authenticator { _: Route?, response ->
                if (response.request.header("Proxy-Authorization") != null) {
                    null      // уже пробовали, второй раз незачем
                } else {
                    response.request.newBuilder()
                        .header("Proxy-Authorization", credential)
                        .build()
                }
            }
            builder.proxyAuthenticator(authenticator)
            if (type == Proxy.Type.SOCKS) {
                // SOCKS5 берёт имя и пароль из системного механизма Java.
                java.net.Authenticator.setDefault(object : java.net.Authenticator() {
                    override fun getPasswordAuthentication() =
                        java.net.PasswordAuthentication(
                            settings.user,
                            settings.password.toCharArray(),
                        )
                })
            }
        }
        return builder
    }

    /** Включён ли прокси сейчас — для подписи в интерфейсе. */
    fun isActive(): Boolean = current != null
}
