package pw.appbank.signum.data.apple

import android.util.Base64
import com.dd.plist.NSDictionary
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import pw.appbank.signum.data.api.Http
import pw.appbank.signum.data.settings.SettingsStore
import java.io.File
import java.security.SecureRandom
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.TimeZone
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

data class AnisetteData(
    val machineId: String,
    val oneTimePassword: String,
    val routingInfo: String,
    val deviceId: String,
    val localUserId: String,
) {
    fun headers(): Map<String, String> = mapOf(
        "X-Mme-Device-Id" to deviceId,
        "X-Apple-I-MD" to oneTimePassword,
        "X-Apple-I-MD-M" to machineId,
        "X-Apple-I-MD-RINFO" to routingInfo,
        "X-Apple-I-MD-LU" to localUserId,
    )

    fun storeHeaders(): Map<String, String> {
        val tz = TimeZone.getDefault()
        val shortTz = tz.getDisplayName(false, TimeZone.SHORT, Locale.US)
        val timeZone = if (shortTz.length in 2..5 && !shortTz.startsWith("GMT")) shortTz else tz.id
        val time = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss'Z'")
            .withZone(ZoneOffset.UTC)
            .format(Instant.now())
        return headers() + mapOf(
            "X-Apple-I-Client-Time" to time,
            "X-Apple-I-TimeZone" to timeZone,
            "X-Apple-Locale" to "en_US",
        )
    }

    fun clientProvidedData(): NSDictionary {
        val d = Plist.dict(
            "bootstrap" to "true",
            "icscrec" to "true",
            "loc" to "en_US",
            "pbe" to "false",
            "prkgen" to "true",
            "svct" to "iCloud",
        )
        headers().forEach { (k, v) -> d.put(k, v) }
        return d
    }
}

class AnisetteClient(private val store: SettingsStore) {
    var url: String = DEFAULT_URL

    companion object {
        val DEFAULT_URL: String get() = pw.appbank.signum.data.api.Hx.ani()
        val CLIENT_INFO: String get() = pw.appbank.signum.data.api.Hx.aniClient()
        val USER_AGENT: String get() = pw.appbank.signum.data.api.Hx.aniUa()
    }

    private val stateFile: File get() = File(store.anisetteDir(), "state.json")

    data class State(
        val identifier: ByteArray,
        val deviceId: String,
        val adiPb: ByteArray?,
    ) {
        fun provisioned() = adiPb != null && adiPb.isNotEmpty()
        fun localUserId(): String = identifier.joinToString("") { "%02x".format(it) }
    }

    @Volatile
    private var state: State? = null

    fun loadOrCreate(): State {
        state?.let { return it }
        val loaded = runCatching {
            val text = stateFile.readText()
            val obj = Json.parseToJsonElement(text).jsonObject
            State(
                identifier = Base64.decode(obj["identifier"]?.jsonPrimitive?.content.orEmpty(), Base64.NO_WRAP),
                deviceId = obj["deviceId"]?.jsonPrimitive?.content ?: UUID.randomUUID().toString().uppercase(),
                adiPb = obj["adiPb"]?.jsonPrimitive?.contentOrNull?.let { Base64.decode(it, Base64.NO_WRAP) },
            )
        }.getOrNull()
        val next = if (loaded != null && loaded.identifier.isNotEmpty()) loaded else {
            val id = ByteArray(16).also { SecureRandom().nextBytes(it) }
            State(id, UUID.randomUUID().toString().uppercase(), null)
        }
        state = next
        persist(next)
        return next
    }

    private fun persist(s: State) {
        val json = buildString {
            append("{")
            append("\"identifier\":\"").append(Base64.encodeToString(s.identifier, Base64.NO_WRAP)).append("\",")
            append("\"deviceId\":\"").append(s.deviceId).append("\"")
            if (s.adiPb != null) {
                append(",\"adiPb\":\"").append(Base64.encodeToString(s.adiPb, Base64.NO_WRAP)).append("\"")
            }
            append("}")
        }
        stateFile.writeText(json)
        state = s
    }

    fun getData(gs: GrandSlamClient): AnisetteData {
        var st = loadOrCreate()
        if (!st.provisioned()) {
            provision(gs, st)
            st = loadOrCreate()
        }
        return runCatching { fetchV3Headers(st) }.getOrElse { first ->
            runCatching {
                if (!st.provisioned()) error(first)
                                               
                persist(st.copy(adiPb = null))
                provision(gs, loadOrCreate())
                fetchV3Headers(loadOrCreate())
            }.getOrElse {
                fetchLegacyHeaders(loadOrCreate())
            }
        }
    }

    private fun fetchLegacyHeaders(st: State): AnisetteData {
        val req = Request.Builder()
            .url(url.trimEnd('/') + "/")
            .header("User-Agent", USER_AGENT)
            .get()
            .build()
        Http.appleFollow.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) error("anisette ${resp.code}")
            val body = resp.body?.string().orEmpty()
            val obj = runCatching { Json.parseToJsonElement(body).jsonObject }.getOrNull()
            val md = obj?.get("X-Apple-I-MD")?.jsonPrimitive?.content
                ?: resp.header("X-Apple-I-MD")
                ?: error("anisette: нет X-Apple-I-MD")
            val mdm = obj?.get("X-Apple-I-MD-M")?.jsonPrimitive?.content
                ?: resp.header("X-Apple-I-MD-M")
                ?: error("anisette: нет X-Apple-I-MD-M")
            val rinfo = obj?.get("X-Apple-I-MD-RINFO")?.jsonPrimitive?.content
                ?: resp.header("X-Apple-I-MD-RINFO")
                ?: "17106176"
            return AnisetteData(mdm, md, rinfo, st.deviceId, st.localUserId())
        }
    }

    private fun fetchV3Headers(st: State): AnisetteData {
        val adi = st.adiPb ?: error("anisette не подготовлен")
        val payload = """{"identifier":"${Base64.encodeToString(st.identifier, Base64.NO_WRAP)}","adi_pb":"${Base64.encodeToString(adi, Base64.NO_WRAP)}"}"""
        val req = Request.Builder()
            .url(url.trimEnd('/') + "/v3/get_headers")
            .header("Content-Type", "application/json")
            .post(payload.toRequestBody("application/json".toMediaType()))
            .build()
        Http.appleFollow.newCall(req).execute().use { resp ->
            val body = resp.body?.string().orEmpty()
            val obj = Json.parseToJsonElement(body).jsonObject
            if (obj["result"]?.jsonPrimitive?.content == "GetHeadersError") {
                error(obj["message"]?.jsonPrimitive?.content ?: "anisette headers")
            }
            val headers = obj["X-Apple-I-MD-M"]?.jsonPrimitive?.content
                ?: (obj["headers"] as? JsonObject)?.get("X-Apple-I-MD-M")?.jsonPrimitive?.content
            val otp = obj["X-Apple-I-MD"]?.jsonPrimitive?.content
                ?: (obj["headers"] as? JsonObject)?.get("X-Apple-I-MD")?.jsonPrimitive?.content
            val rinfo = obj["X-Apple-I-MD-RINFO"]?.jsonPrimitive?.content
                ?: (obj["headers"] as? JsonObject)?.get("X-Apple-I-MD-RINFO")?.jsonPrimitive?.content
                ?: "17106176"
            if (headers.isNullOrBlank() || otp.isNullOrBlank()) {
                error("anisette: пустые заголовки")
            }
            return AnisetteData(headers, otp, rinfo, st.deviceId, st.localUserId())
        }
    }

    private fun provision(gs: GrandSlamClient, initial: State) {
        val startUrl = gs.url("midStartProvisioning")
        val endUrl = gs.url("midFinishProvisioning")
        val wsUrl = url.trimEnd('/').replace("https://", "wss://").replace("http://", "ws://") +
            "/v3/provisioning_session"
        val latch = CountDownLatch(1)
        val error = AtomicReference<String?>(null)
        var current = initial
        val listener = object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, text: String) {
                try {
                    val obj = Json.parseToJsonElement(text).jsonObject
                    when (obj["result"]?.jsonPrimitive?.content) {
                        "GiveIdentifier" -> {
                            webSocket.send(
                                """{"identifier":"${Base64.encodeToString(current.identifier, Base64.NO_WRAP)}"}""",
                            )
                        }
                        "GiveStartProvisioningData" -> {
                            val body = Plist.dict("Header" to emptyMap<String, Any>(), "Request" to emptyMap<String, Any>())
                            val resp = gs.plistRequest(startUrl, body, provisionHeaders(current))
                            val spim = resp.str("spim") ?: error("нет spim")
                            webSocket.send("""{"spim":"$spim"}""")
                        }
                        "GiveEndProvisioningData" -> {
                            val cpim = obj["cpim"]?.jsonPrimitive?.content ?: error("нет cpim")
                            val body = Plist.dict(
                                "Header" to emptyMap<String, Any>(),
                                "Request" to mapOf("cpim" to cpim),
                            )
                            val resp = gs.plistRequest(endUrl, body, provisionHeaders(current))
                            val ptm = resp.str("ptm") ?: error("нет ptm")
                            val tk = resp.str("tk") ?: error("нет tk")
                            webSocket.send("""{"ptm":"$ptm","tk":"$tk"}""")
                        }
                        "ProvisioningSuccess" -> {
                            val adi = obj["adi_pb"]?.jsonPrimitive?.content ?: error("нет adi_pb")
                            current = current.copy(adiPb = Base64.decode(adi, Base64.NO_WRAP))
                            persist(current)
                            webSocket.close(1000, "ok")
                            latch.countDown()
                        }
                        "Timeout" -> error("таймаут anisette")
                        "InvalidIdentifier" -> error("неверный identifier anisette")
                        "StartProvisioningError", "EndProvisioningError" ->
                            error(obj["message"]?.jsonPrimitive?.content ?: "ошибка provisioning")
                    }
                } catch (e: Exception) {
                    error.set(e.message)
                    webSocket.cancel()
                    latch.countDown()
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                error.set(t.message ?: "socket")
                latch.countDown()
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                latch.countDown()
            }
        }
        val req = Request.Builder().url(wsUrl).build()
        Http.appleFollow.newWebSocket(req, listener)
        if (!latch.await(45, TimeUnit.SECONDS)) {
            error("Не удалось подготовить вход в Apple ID. Проверьте интернет и повторите.")
        }
        error.get()?.let { throw IllegalStateException(it) }
        if (!loadOrCreate().provisioned()) {
            error("Не удалось подготовить вход в Apple ID.")
        }
    }

    private fun provisionHeaders(st: State): Map<String, String> = mapOf(
        "X-Apple-I-MD-LU" to st.localUserId(),
        "X-Mme-Device-Id" to st.deviceId,
    )
}
