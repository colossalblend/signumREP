package pw.appbank.signum.data.apple

import com.dd.plist.NSDictionary
import org.bouncycastle.crypto.engines.AESEngine
import org.bouncycastle.crypto.modes.CBCBlockCipher
import org.bouncycastle.crypto.modes.GCMBlockCipher
import org.bouncycastle.crypto.params.AEADParameters
import org.bouncycastle.crypto.params.KeyParameter
import org.bouncycastle.crypto.params.ParametersWithIV
import org.bouncycastle.crypto.paddings.PKCS7Padding
import org.bouncycastle.crypto.paddings.PaddedBufferedBlockCipher
import pw.appbank.signum.data.models.LoginResult
import pw.appbank.signum.data.settings.SettingsStore
import java.security.MessageDigest
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.TimeZone
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

enum class LoginState {
    LoggedIn,
    NeedsDevice2FA,
    NeedsDevice2FAVerification,
    NeedsSms2FA,
    NeedsSms2FAVerification,
    NeedsLogin,
}

class AppleAccount(
    private val store: SettingsStore,
    private val anisette: AnisetteClient,
    private val gs: GrandSlamClient = GrandSlamClient(),
) {
    var email: String = ""
        private set
    var loginState: LoginState = LoginState.NeedsLogin
        private set
    var spd: NSDictionary? = null
        private set
    private var password: String = ""
    private var smsId: Int = 1

    fun login(emailRaw: String, passwordRaw: String, twoFactorCode: String?): LoginResult {
        email = emailRaw.trim().lowercase()
        if (passwordRaw.isNotEmpty()) password = passwordRaw
        if (email.isEmpty() || password.isEmpty()) {
            error("Введите Apple ID и пароль")
        }
        store.storePassword(email, password)
        store.markAccountAdded(email)

        val code = twoFactorCode?.filter { it.isDigit() }.orEmpty()
        if (code.length >= 6 && loginState == LoginState.NeedsDevice2FAVerification) {
            loginState = verifyDevice2fa(code)
        } else if (code.length >= 6 && loginState == LoginState.NeedsSms2FAVerification) {
            loginState = verifySms2fa(code, smsId)
        } else {
            loginState = loginInner()
        }

        var attempts = 0
        while (attempts++ < 8) {
            when (loginState) {
                LoginState.LoggedIn -> {
                    persistSession()
                    store.setActiveEmail(email)
                    return LoginResult(
                        email = email,
                        message = "Apple ID добавлен и выбран для установок",
                    )
                }
                LoginState.NeedsDevice2FA -> {
                    sendTrustedDevice2fa()
                    loginState = LoginState.NeedsDevice2FAVerification
                    return LoginResult(
                        email = email,
                        needs2fa = true,
                        message = "Введите 6-значный код с iPhone. Если push не пришёл — «Запросить новый код».",
                    )
                }
                LoginState.NeedsDevice2FAVerification -> {
                    if (code.length >= 6) {
                        loginState = verifyDevice2fa(code)
                    } else {
                        return LoginResult(
                            email = email,
                            needs2fa = true,
                            message = "Введите 6-значный код с iPhone.",
                        )
                    }
                }
                LoginState.NeedsSms2FA -> {
                    sendSms2fa(smsId)
                    loginState = LoginState.NeedsSms2FAVerification
                    return LoginResult(
                        email = email,
                        needs2fa = true,
                        message = "Apple отправил SMS. Введите 6-значный код.",
                    )
                }
                LoginState.NeedsSms2FAVerification -> {
                    if (code.length >= 6) {
                        loginState = verifySms2fa(code, smsId)
                    } else {
                        return LoginResult(
                            email = email,
                            needs2fa = true,
                            message = "Введите 6-значный код из SMS.",
                        )
                    }
                }
                LoginState.NeedsLogin -> loginState = loginInner()
            }
        }
        return LoginResult(email = email, needs2fa = true, message = "Не удалось завершить вход. Повторите.")
    }

    fun resendCode(): LoginResult {
        loginState = LoginState.NeedsLogin
        return login(email, password, null)
    }

    fun logout() {
        if (email.isNotEmpty()) store.removeAccount(email)
        email = ""
        password = ""
        spd = null
        loginState = LoginState.NeedsLogin
    }

    fun pet(): String? {
        val t = spd?.dict("t")?.dict("com.apple.gs.idms.pet") ?: return null
        return t.str("token")
    }

    fun refreshPetIfPossible(): Boolean {
        if (email.isBlank()) restoreIfPossible()
        if (password.isBlank()) {
            password = store.loadPassword(email).orEmpty()
        }
        if (email.isBlank() || password.isBlank()) return false
        val previous = loginState
        return runCatching {
            val state = loginInner()
            if (state == LoginState.LoggedIn) {
                loginState = LoginState.LoggedIn
                persistSession()
                !pet().isNullOrBlank()
            } else {
                loginState = previous
                false
            }
        }.getOrDefault(false)
    }

    fun dsid(): String? = spd?.str("adsid") ?: spd?.str("dsid")

    fun gsIdmsToken(): String? = spd?.str("GsIdmsToken")

    fun sessionKey(): ByteArray? = spd?.data("sk")

    fun restoreIfPossible(): Boolean {
        val active = store.activeEmail() ?: return false
        email = active
        password = store.loadPassword(active).orEmpty()
        val blob = store.loadSessionBlob(active) ?: return false
        return runCatching {
            spd = Plist.parse(android.util.Base64.decode(blob, android.util.Base64.NO_WRAP))
            loginState = LoginState.LoggedIn
            true
        }.getOrDefault(false)
    }

    private fun persistSession() {
        val dict = spd ?: return
        val xml = Plist.xml(dict)
        store.storeSessionBlob(email, android.util.Base64.encodeToString(xml, android.util.Base64.NO_WRAP))
    }

    private fun loginInner(): LoginState {
        val ani = anisette.getData(gs)
        val cpd = ani.clientProvidedData()
        val srp = SrpClient(email.toByteArray())
        val req1 = Plist.dict(
            "Header" to mapOf("Version" to "1.0.1"),
            "Request" to mapOf(
                "A2k" to srp.aPub,
                "cpd" to cpd,
                "o" to "init",
                "ps" to listOf("s2k", "s2k_fo"),
                "u" to email,
            ),
        )
        val response = gs.plistRequest(gs.url("gsService"), req1, ani.headers())
        val salt = response.data("s") ?: error("Apple не подтвердил вход. Повторите попытку.")
        val bPub = response.data("B") ?: error("Apple не подтвердил вход. Повторите попытку.")
        val iters = response.int("i") ?: 20000
        val c = response.str("c") ?: error("Apple не подтвердил вход. Повторите попытку.")
        val protocol = response.str("sp") ?: "s2k"
        val derived = derivePassword(password, salt, iters, protocol)
        val verifier = srp.processReply(derived, salt, bPub)
        val req2 = Plist.dict(
            "Header" to mapOf("Version" to "1.0.1"),
            "Request" to mapOf(
                "M1" to verifier.proof,
                "c" to c,
                "cpd" to cpd,
                "o" to "complete",
                "u" to email,
            ),
        )
        val response2 = try {
            gs.plistRequest(gs.url("gsService"), req2, ani.headers())
        } catch (e: AppleAuthException) {
            val msg = e.message
            if (msg.contains("password", ignoreCase = true) || e.code == -20751L || e.code == -20101L) {
                error("Apple не принял пароль. Проверьте раскладку и повторите. Если пароль верный — подождите минуту и войдите снова.")
            }
            throw e
        }
        val m2 = response2.data("M2") ?: error("Apple не подтвердил вход. Повторите попытку.")
        verifier.verifyServer(m2)
        val spdEnc = response2.data("spd") ?: error("Apple не подтвердил вход. Повторите попытку.")
        spd = parseSpd(spdEnc, verifier)
        val status = response2.dict("Status")
        val au = status?.str("au")
        return when (au) {
            "trustedDeviceSecondaryAuth" -> LoginState.NeedsDevice2FA
            "secondaryAuth" -> LoginState.NeedsSms2FA
            "repair", null, "" -> LoginState.LoggedIn
            else -> LoginState.LoggedIn
        }
    }

    private fun sendTrustedDevice2fa() {
        val ani = anisette.getData(gs)
        val url = gs.url("trustedDeviceSecondaryAuth")
        val (code, body) = gs.get(url, build2faHeaders(ani))
        val preview = runCatching { body.toString(Charsets.UTF_8) }.getOrDefault("")
            .replace(Regex("[A-Za-z0-9+/=]{24,}"), "***").replace('\n', ' ').take(300)
        android.util.Log.i("SignumLogin", "trustedDevice2fa GET status=$code host=${url.substringAfter("://").substringBefore("/")} body=$preview")
        if (code !in 200..299 && code != 409) {
            error("Apple не отправил код на iPhone (HTTP $code). Проверьте интернет и нажмите «Запросить новый код».")
        }
    }

    private fun verifyDevice2fa(code: String): LoginState {
        val ani = anisette.getData(gs)
        // ipatool sends the code in a JSON body (POST), not in a security-code
        // header. The header form gets rejected with "invalid Trust Key (-45003)".
        val (status, body) = gs.postJson(
            gs.url("validateCode"),
            """{"securityCode":{"code":"$code"}}""",
            build2faHeaders(ani),
        )
        if (status in 200..299) {
            loginState = LoginState.NeedsLogin
            return loginInner()
        }
        if (body.contains("-21669") || body.contains("Incorrect", true)) {
            return LoginState.NeedsDevice2FAVerification
        }
        loginState = LoginState.NeedsLogin
        return loginInner()
    }

    private fun sendSms2fa(id: Int) {
        val ani = anisette.getData(gs)
        gs.putJson(
            "https://gsa.apple.com/auth/verify/phone",
            """{"phoneNumber":{"id":$id},"mode":"sms"}""",
            build2faHeaders(ani),
        )
    }

    private fun verifySms2fa(code: String, id: Int): LoginState {
        val ani = anisette.getData(gs)
        val (status, body) = gs.postJson(
            "https://gsa.apple.com/auth/verify/phone/securitycode",
            """{"phoneNumber":{"id":$id},"securityCode":{"code":"$code"},"mode":"sms"}""",
            build2faHeaders(ani),
        )
        if (status in 200..299) {
            return loginInner()
        }
        if (body.contains("-21669")) return LoginState.NeedsSms2FAVerification
        return loginInner()
    }

    private fun build2faHeaders(ani: AnisetteData): Map<String, String> {
        val identity = spd?.str("adsid") ?: email
        val token = spd?.str("GsIdmsToken") ?: ""
        val basic = android.util.Base64.encodeToString(
            "$identity:$token".toByteArray(),
            android.util.Base64.NO_WRAP,
        )
        // Same set as ipatool's apply2FAHeaders: without time/locale headers the
        // trusted-device call is rejected with "invalid Trust Key (-45003)".
        val tz = TimeZone.getDefault()
        val shortTz = tz.getDisplayName(false, TimeZone.SHORT, Locale.US)
        val timeZone = if (shortTz.length in 2..5 && !shortTz.startsWith("GMT")) shortTz else tz.id
        val clientTime = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss'Z'")
            .withZone(ZoneOffset.UTC)
            .format(Instant.now())
        return ani.headers() + mapOf(
            "X-Apple-Identity-Token" to basic,
            "X-Apple-App-Info" to "com.apple.gs.xcode.auth",
            "X-Xcode-Version" to "16.0 (16A242d)",
            "X-Mme-Client-Info" to AnisetteClient.CLIENT_INFO,
            "X-Apple-I-Client-Time" to clientTime,
            "X-Apple-Locale" to "en_US",
            "X-Apple-I-TimeZone" to timeZone,
        )
    }

    private fun derivePassword(password: String, salt: ByteArray, iters: Int, protocol: String): ByteArray {
        val hashed = MessageDigest.getInstance("SHA-256").digest(password.toByteArray())
        val material = if (protocol == "s2k_fo") {
            hashed.joinToString("") { "%02x".format(it) }.toByteArray()
        } else hashed
        return pbkdf2(material, salt, iters)
    }

    private fun pbkdf2(password: ByteArray, salt: ByteArray, iters: Int): ByteArray {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(password, "HmacSHA256"))
        val hashLen = 32
        val block = ByteArray(salt.size + 4)
        System.arraycopy(salt, 0, block, 0, salt.size)
        block[salt.size + 3] = 1
        var u = mac.doFinal(block)
        val out = u.copyOf()
        repeat(iters - 1) {
            u = mac.doFinal(u)
            for (i in out.indices) out[i] = (out[i].toInt() xor u[i].toInt()).toByte()
        }
        return out
    }

    private fun parseSpd(spdEnc: ByteArray, verifier: SrpClient.Verifier): NSDictionary {
        val candidates = listOf(verifier.key, verifier.sessionKey)
        var last: Exception? = null
        for (material in candidates) {
            try {
                val dec = decryptCbc(material, spdEnc)
                return Plist.parse(dec)
            } catch (e: Exception) {
                last = e
            }
        }
        throw last ?: IllegalStateException("Не удалось прочитать ответ Apple после входа. Повторите.")
    }

    private fun decryptCbc(sessionKey: ByteArray, data: ByteArray): ByteArray {
        val key = hmacSha256(sessionKey, "extra data key:".toByteArray())
        val ivFull = hmacSha256(sessionKey, "extra data iv:".toByteArray())
        val iv = ivFull.copyOf(16)
        val engine = PaddedBufferedBlockCipher(CBCBlockCipher.newInstance(AESEngine.newInstance()), PKCS7Padding())
        engine.init(false, ParametersWithIV(KeyParameter(key), iv))
        val out = ByteArray(engine.getOutputSize(data.size))
        var n = engine.processBytes(data, 0, data.size, out, 0)
        n += engine.doFinal(out, n)
        return out.copyOf(n)
    }

    companion object {
        fun hmacSha256(key: ByteArray, data: ByteArray): ByteArray {
            val mac = Mac.getInstance("HmacSHA256")
            mac.init(SecretKeySpec(key, "HmacSHA256"))
            return mac.doFinal(data)
        }

        fun decryptGcmToken(data: ByteArray, key: ByteArray): ByteArray {
            if (data.size < 3 + 16 + 16) error("token too short")
            require(data.copyOfRange(0, 3).contentEquals("XYZ".toByteArray()))
            val iv = data.copyOfRange(3, 19)
            val ct = data.copyOfRange(19, data.size)
            val gcm = GCMBlockCipher.newInstance(AESEngine.newInstance())
            gcm.init(false, AEADParameters(KeyParameter(key), 128, iv))
            val out = ByteArray(gcm.getOutputSize(ct.size))
            var n = gcm.processBytes(ct, 0, ct.size, out, 0)
            n += gcm.doFinal(out, n)
            return out.copyOf(n)
        }
    }
}
