package pw.appbank.signum.data.api

import okhttp3.CertificatePinner
import okhttp3.OkHttpClient
import okhttp3.Protocol
import java.util.concurrent.TimeUnit

object Http {
    private val apiHost: String get() = Hx.apiHost()

    /**
     * Строка в запись для поддержки на каждый запрос.
     *
     * Тела не читаем: у ответа оно одноразовое, и вычитать его здесь значит
     * отобрать у вызывающего кода. Берём длину из заголовка и первые байты
     * только у своего API — у Apple там подписи и токены.
     */
    private val diagInterceptor = okhttp3.Interceptor { chain ->
        val req = chain.request()
        if (!DiagLog.isActive()) return@Interceptor chain.proceed(req)
        val started = System.currentTimeMillis()
        try {
            val resp = chain.proceed(req)
            val len = resp.header("Content-Length")?.toLongOrNull()
                ?: resp.body?.contentLength()?.takeIf { it >= 0 } ?: -1L
            DiagLog.http(req.method, req.url.toString(), resp.code, len, null)
            resp
        } catch (e: Throwable) {
            DiagLog.httpFail(req.method, req.url.toString(), e.message ?: e.javaClass.simpleName)
            throw e
        } finally {
            val ms = System.currentTimeMillis() - started
            if (ms > 10_000) DiagLog.note("http", "медленно ${ms} мс ${req.url}")
        }
    }

    // Клиенты пересоздаются при смене прокси: OkHttp не позволяет изменить
    // прокси у готового клиента, а настройку человек меняет на ходу.
    fun rebuild() {
        apple = buildApple()
        appleFollow = apple.newBuilder().followRedirects(true).followSslRedirects(true).build()
        api = buildApi()
        itunes = buildItunes()
        itunesNoFollow = itunes.newBuilder().followRedirects(false).followSslRedirects(false).build()
        cdn = buildCdn()
    }

    private fun buildApple(): OkHttpClient = ProxyConfig.wrap(OkHttpClient.Builder()
        .addInterceptor(diagInterceptor)
        .connectTimeout(20, TimeUnit.SECONDS)
        .readTimeout(45, TimeUnit.SECONDS)
        .writeTimeout(45, TimeUnit.SECONDS)
        .followRedirects(false)
        .followSslRedirects(false)
        .protocols(listOf(Protocol.HTTP_1_1))
        .sslSocketFactory(AppleTrust.socketFactory, AppleTrust.trustManager))
        .build()

    @Volatile var apple: OkHttpClient = buildApple()
        private set

    @Volatile var appleFollow: OkHttpClient =
        apple.newBuilder().followRedirects(true).followSslRedirects(true).build()
        private set

    private fun buildApi(): OkHttpClient = ProxyConfig.wrap(OkHttpClient.Builder()
        .addInterceptor(diagInterceptor)
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(25, TimeUnit.SECONDS)
        .certificatePinner(
            CertificatePinner.Builder()
                // leaf SPKI (YR1-issued, until 2026-11-25)
                .add(apiHost, "sha256/FkWuDT/bdBonQrGCRPQJ75lLXXWYP2znbMTSiS+2Nzs=")
                // previous leaf SPKI
                .add(apiHost, "sha256/WBjB4nAkcd+KG3ls1/yjNqq/Qxq3y2k9CJTQmScjd+A=")
                // Let's Encrypt YR1
                .add(apiHost, "sha256/LoMHBotttiDko50Gi13uXW71eIy7LAttI+rYT8wXF4w=")
                // Let's Encrypt YR2
                .add(apiHost, "sha256/nWN7PSep5XDQdge5zK24CnCRXHr3KvzhKEGxsdqCX9E=")
                // Let's Encrypt E5 / E6 / R10 / R11
                .add(apiHost, "sha256/NYbU7PBwV4y9J67c4guWTki8FJ+uudrXL0a4V4aRcrg=")
                .add(apiHost, "sha256/0Bbh/jEZSKymTy3kTOhsmlHKBB32EDu1KojrP3YfV9c=")
                .add(apiHost, "sha256/K7rZOrXHknnsEhUH8nLL4MZkejquUuIvOIr6tCa0rbo=")
                .add(apiHost, "sha256/bdrBhpj38ffhxpubzkINl0rG+UyossdhcBYj+Zx2fcc=")
                .build(),
        ))
        .build()

    @Volatile var api: OkHttpClient = buildApi()
        private set

    private fun buildItunes(): OkHttpClient = ProxyConfig.wrap(OkHttpClient.Builder()
        .addInterceptor(diagInterceptor)
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(40, TimeUnit.SECONDS)
        .followRedirects(true)
        .protocols(listOf(Protocol.HTTP_1_1))
        .sslSocketFactory(AppleTrust.socketFactory, AppleTrust.trustManager))
        .build()

    @Volatile var itunes: OkHttpClient = buildItunes()
        private set

    @Volatile var itunesNoFollow: OkHttpClient =
        itunes.newBuilder().followRedirects(false).followSslRedirects(false).build()
        private set

                                                                                                 
    private fun buildCdn(): OkHttpClient = ProxyConfig.wrap(OkHttpClient.Builder()
        .addInterceptor(diagInterceptor)
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(15, TimeUnit.SECONDS)
        .writeTimeout(60, TimeUnit.SECONDS)
        .followRedirects(true)
        .followSslRedirects(true)
        .retryOnConnectionFailure(true)
        .protocols(listOf(Protocol.HTTP_1_1))
        .addInterceptor { chain ->
            val req = chain.request().newBuilder()
                .header("Accept-Encoding", "identity")
                .build()
            chain.proceed(req)
        })
        .build()

    @Volatile var cdn: OkHttpClient = buildCdn()
        private set
}
