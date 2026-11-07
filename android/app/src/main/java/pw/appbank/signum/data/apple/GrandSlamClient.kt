package pw.appbank.signum.data.apple

import com.dd.plist.NSDictionary
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import pw.appbank.signum.data.api.Http

class GrandSlamClient {
    private var urls: NSDictionary? = null

    fun ensureBag() {
        if (urls != null) return
        val req = Request.Builder()
            .url(URL_BAG)
            .headers(baseHeaders())
            .get()
            .build()
        Http.apple.newCall(req).execute().use { resp ->
            val body = resp.body?.bytes() ?: error("URL bag пуст")
            pw.appbank.signum.data.api.DebugDump.write("gsa-urlbag.bin", body)
            if (!resp.isSuccessful) {
                error("Не удалось связаться с Apple (${resp.code})")
            }
            val dict = Plist.parse(body)
            urls = dict.dict("urls") ?: dict
        }
    }

    fun url(key: String): String {
        ensureBag()
        return urls?.str(key) ?: when (key) {
            "gsService" -> "https://gsa.apple.com/grandslam/GsService2"
            "trustedDeviceSecondaryAuth" -> "https://gsa.apple.com/auth/verify/trusteddevice"
            "validateCode" -> "https://gsa.apple.com/auth/verify/trusteddevice/securitycode"
            "midStartProvisioning" -> "https://gsa.apple.com/grandslam/GsService2/midStartProvisioning"
            "midFinishProvisioning" -> "https://gsa.apple.com/grandslam/GsService2/midFinishProvisioning"
            else -> error("URL bag: нет $key")
        }
    }

    fun plistRequest(
        url: String,
        body: NSDictionary,
        extraHeaders: Map<String, String> = emptyMap(),
        sms: Boolean = false,
    ): NSDictionary {
        val builder = Request.Builder().url(url)
        baseHeaders(sms).toMultimap().forEach { (k, v) -> builder.header(k, v.first()) }
        extraHeaders.forEach { (k, v) -> builder.header(k, v) }
        val payload = Plist.xml(body)
        val media = if (sms) "application/json" else "text/x-xml-plist"
        builder.post(payload.toRequestBody(media.toMediaType()))
        Http.apple.newCall(builder.build()).execute().use { resp ->
            val bytes = resp.body?.bytes() ?: ByteArray(0)
            pw.appbank.signum.data.api.DebugDump.write("gsa-last.bin", bytes)
            pw.appbank.signum.data.api.DebugDump.write("gsa-last-req.xml", payload)
            val dict = runCatching { Plist.parse(bytes) }.getOrElse { e ->
                if (!resp.isSuccessful) {
                    error("Apple отклонил запрос (${resp.code})")
                }
                throw e
            }
            val response = dict.dict("Response") ?: dict
            checkStatus(response)
            if (!resp.isSuccessful) {
                error("Apple отклонил запрос (${resp.code})")
            }
            return response
        }
    }

    fun get(url: String, extraHeaders: Map<String, String> = emptyMap()): Pair<Int, ByteArray> {
        val builder = Request.Builder().url(url).get()
        baseHeaders().toMultimap().forEach { (k, v) -> builder.header(k, v.first()) }
        extraHeaders.forEach { (k, v) -> builder.header(k, v) }
        Http.apple.newCall(builder.build()).execute().use { resp ->
            return resp.code to (resp.body?.bytes() ?: ByteArray(0))
        }
    }

    fun putJson(url: String, json: String, extraHeaders: Map<String, String>): Pair<Int, String> {
        val builder = Request.Builder()
            .url(url)
            .put(json.toRequestBody("application/json".toMediaType()))
        baseHeaders(sms = true).toMultimap().forEach { (k, v) -> builder.header(k, v.first()) }
        extraHeaders.forEach { (k, v) -> builder.header(k, v) }
        Http.apple.newCall(builder.build()).execute().use { resp ->
            return resp.code to (resp.body?.string().orEmpty())
        }
    }

    fun postJson(url: String, json: String, extraHeaders: Map<String, String>): Pair<Int, String> {
        val builder = Request.Builder()
            .url(url)
            .post(json.toRequestBody("application/json".toMediaType()))
        baseHeaders(sms = true).toMultimap().forEach { (k, v) -> builder.header(k, v.first()) }
        extraHeaders.forEach { (k, v) -> builder.header(k, v) }
        Http.apple.newCall(builder.build()).execute().use { resp ->
            return resp.code to (resp.body?.string().orEmpty())
        }
    }

    fun checkStatus(dict: NSDictionary) {
        val status = dict.dict("Status") ?: dict
        val ec = status.int("ec") ?: 0
        if (ec != 0) {
            val em = status.str("em") ?: "Unknown error"
            throw AppleAuthException(ec.toLong(), em)
        }
    }

    fun baseHeaders(sms: Boolean = false): okhttp3.Headers {
        val b = okhttp3.Headers.Builder()
        if (sms) {
            b.add("Content-Type", "application/json")
            b.add("Accept", "application/json")
        } else {
            b.add("Content-Type", "text/x-xml-plist")
            b.add("Accept", "text/x-xml-plist")
        }
        b.add("X-Mme-Client-Info", AnisetteClient.CLIENT_INFO)
        b.add("User-Agent", AnisetteClient.USER_AGENT)
        b.add("X-Xcode-Version", "27.0 (27A5218g)")
        b.add("X-Apple-App-Info", "com.apple.gs.xcode.auth")
        return b.build()
    }

    companion object {
        private const val URL_BAG = "https://gsa.apple.com/grandslam/GsService2/lookup"
    }
}

class AppleAuthException(val code: Long, override val message: String) : Exception(message)
