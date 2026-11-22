package pw.appbank.signum.data.device

import com.dd.plist.NSDictionary
import com.dd.plist.NSNumber
import com.dd.plist.NSObject
import org.bouncycastle.asn1.ASN1Primitive
import org.bouncycastle.asn1.pkcs.RSAPublicKey
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.asn1.x509.BasicConstraints
import org.bouncycastle.asn1.x509.Extension
import org.bouncycastle.asn1.x509.KeyUsage
import org.bouncycastle.cert.jcajce.JcaX509ExtensionUtils
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder
import pw.appbank.signum.data.apple.Plist
import pw.appbank.signum.data.apple.data
import pw.appbank.signum.data.apple.dict
import pw.appbank.signum.data.apple.str
import pw.appbank.signum.data.models.DeviceInfo
import pw.appbank.signum.data.models.InstalledApp
import pw.appbank.signum.data.settings.SettingsStore
import java.io.ByteArrayInputStream
import java.math.BigInteger
import java.nio.ByteBuffer
import java.security.KeyFactory
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.PrivateKey
import java.security.PublicKey
import java.security.SecureRandom
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.spec.PKCS8EncodedKeySpec
import java.security.spec.RSAPublicKeySpec
import java.security.spec.X509EncodedKeySpec
import java.util.Date
import java.util.UUID
import javax.net.ssl.SSLContext
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager

class LockdownClient(
    private val store: SettingsStore,
    private var stream: UsbMux.MuxStream,
) {
    private var sessionId: String? = null
    private var ssl = false
    private var tls: TlsPipe? = null

    fun queryType(): String {
        val r = request(mapOf("Request" to "QueryType"))
        val type = r.str("Type") ?: r.str("Request") ?: "com.apple.mobile.lockdown"
        android.util.Log.i("SignumPair", "queryType=$type")
        return type
    }

    fun getValue(domain: String? = null, key: String? = null): NSDictionary {
        val fields = mutableMapOf<String, Any?>("Request" to "GetValue")
        if (domain != null) fields["Domain"] = domain
        if (key != null) fields["Key"] = key
        return request(fields)
    }

    fun readInfo(): DeviceInfo {
        val name = stringValue("DeviceName")
        val udid = stringValue("UniqueDeviceID") ?: error("Не удалось определить iPhone")
        val product = stringValue("ProductType")
        val version = stringValue("ProductVersion")
        val battery = runCatching {
            getValue("com.apple.mobile.battery", "BatteryCurrentCapacity")
                .objectForKey("Value")
                ?.toString()
                ?.toIntOrNull()
        }.getOrNull()
        return DeviceInfo(
            udid = udid,
            productType = product,
            productVersion = version,
            deviceName = name,
            battery = battery,
            connection = "usb",
            paired = pairingFile(udid).exists(),
        )
    }

    fun pair(
        deviceNameHint: String = "Android",
        force: Boolean = false,
        onStatus: (String) -> Unit = {},
    ): DeviceInfo {
        val info = readInfo()
        val existing = pairingFile(info.udid)
        if (existing.exists() && !force) {
            android.util.Log.i("SignumPair", "using saved pairing ${info.udid} (no ValidatePair)")
            return info.copy(paired = true)
        }
        if (force && existing.exists()) {
            val old = runCatching { Plist.parse(existing.readBytes()) }.getOrNull()
            val oldHost = old?.str("HostID")
            if (!oldHost.isNullOrBlank()) {
                runCatching {
                    request(mapOf("Request" to "Unpair", "PairRecord" to mapOf("HostID" to oldHost)))
                }
            }
            existing.delete()
        }
        val hostId = UUID.randomUUID().toString().uppercase()
        val buid = loadOrCreateBuid()
        val rootKeys = generateKeys()
        val hostKeys = generateKeys()
        val rootCert = selfSigned(rootKeys, "Signum Root CA", ca = true)
        val hostCert = signedCert(rootKeys, rootCert, hostKeys.public, "Signum Host", ca = false)
        val devicePub = readDevicePublicKey()
        val deviceCert = signedCert(rootKeys, rootCert, devicePub, "Device", ca = false)
        val wifi = runCatching { stringValue("WiFiAddress") }.getOrNull()
        android.util.Log.i("SignumPair", "pair start udid=${info.udid} wifi=$wifi pem=true")
        val pairRecord = mutableMapOf<String, Any?>(
            "DeviceCertificate" to pemCert(deviceCert),
            "HostCertificate" to pemCert(hostCert),
            "RootCertificate" to pemCert(rootCert),
            "HostID" to hostId,
            "SystemBUID" to buid,
        )
        if (!wifi.isNullOrBlank()) pairRecord["WiFiMACAddress"] = wifi
        pairRecord["HostPrivateKey"] = pemKey(hostKeys.private)
        repeat(15) { attempt ->
            onStatus("Разблокируйте iPhone и нажмите «Доверять этому компьютеру»… (${attempt + 1}/15)")
            val resp = request(
                mapOf(
                    "Request" to "Pair",
                    "PairRecord" to pairRecord,
                    "ProtocolVersion" to "2",
                    "PairingOptions" to mapOf("ExtendedPairingErrors" to true),
                ),
            )
            val err = resp.str("Error")
            android.util.Log.i("SignumPair", "try=${attempt + 1} err=$err")
            if (err == null) {
                val escrow = resp.objectForKey("EscrowBag")
                val fromDevice = resp.objectForKey("DeviceCertificate")
                val record = Plist.dict(
                    "HostID" to hostId,
                    "SystemBUID" to buid,
                    "HostCertificate" to pemCert(hostCert),
                    "HostPrivateKey" to pemKey(hostKeys.private),
                    "RootCertificate" to pemCert(rootCert),
                    "RootPrivateKey" to pemKey(rootKeys.private),
                    "DeviceCertificate" to ((fromDevice as? com.dd.plist.NSData)?.bytes() ?: deviceCert?.let(::pemCert)),
                    "EscrowBag" to (escrow as? com.dd.plist.NSData)?.bytes(),
                    "DeviceName" to (info.deviceName ?: deviceNameHint),
                    "WiFiMACAddress" to wifi,
                )
                existing.parentFile?.mkdirs()
                existing.writeBytes(Plist.xml(record))
                return info.copy(paired = true)
            }
            when {
                err.contains("UserDenied", true) ->
                    error("На iPhone нажмите «Доверять» и введите код-пароль.")
                err.contains("PasswordProtected", true) ||
                    err.contains("PairingDialogResponsePending", true) ||
                    err.contains("PairingDialog", true) ->
                    Thread.sleep(2_000)
                else -> {
                    if (attempt < 2) Thread.sleep(1_000) else error("Не удалось установить доверие. Разблокируйте iPhone и нажмите «Доверять».")
                }
            }
        }
        error(
            "На iPhone не появилось «Доверять». " +
                "Разблокируйте телефон, оставьте экран включённым и нажмите «Доверять» ещё раз.",
        )
    }

    fun startSession(udid: String, mux: UsbMux? = null) {
        if (mux != null) reconnect(mux)
        try {
            doStartSession(udid)
        } catch (e: Exception) {
            if (mux == null) throw e
            val msg = e.message.orEmpty()
            val pairBroken = msg.contains("InvalidHostID", true) ||
                msg.contains("InvalidPair", true) ||
                (msg.contains("Сессия lockdown") && !msg.contains("matching", true))
            if (!pairBroken) {
                android.util.Log.w("SignumPair", "StartSession/TLS failed, keep pairing: $msg")
                throw e
            }
            android.util.Log.w("SignumPair", "StartSession failed, re-pair: $msg")
            reconnect(mux)
            pair(force = true)
            reconnect(mux)
            doStartSession(udid)
        }
    }

    fun reconnect(mux: UsbMux) {
        tls = null
        ssl = false
        sessionId = null
        runCatching { stream.close() }
        stream = mux.connect(UsbMux.LOCKDOWN_PORT)
        queryType()
    }

    private fun doStartSession(udid: String) {
        val rec = Plist.parse(pairingFile(udid).readBytes())
        val hostId = rec.str("HostID") ?: error("Не удалось прочитать данные доверия")
        val buid = rec.str("SystemBUID") ?: error("Не удалось прочитать данные доверия")
        pw.appbank.signum.data.api.DebugDump.pair("StartSession host=$hostId buid=$buid")
        val resp = request(
            mapOf(
                "Request" to "StartSession",
                "HostID" to hostId,
                "SystemBUID" to buid,
            ),
        )
        val err = resp.str("Error")
        val enableSsl = resp.objectForKey("EnableSessionSSL")
        sessionId = resp.str("SessionID")
        pw.appbank.signum.data.api.DebugDump.pair(
            "StartSession err=$err ssl=$enableSsl sid=$sessionId keys=${resp.allKeys()?.contentToString()}",
        )
        if (err != null) error("Не удалось начать сессию с iPhone")
        if (enableSsl?.toString() == "true" || (enableSsl as? com.dd.plist.NSNumber)?.boolValue() == true) {
            ssl = true
            tls = TlsPipe.start(stream, pairingFile(udid))
            val probe = runCatching { stringValue("ProductVersion") }
            pw.appbank.signum.data.api.DebugDump.pair(
                "tls probe ProductVersion=${probe.getOrNull()} err=${probe.exceptionOrNull()?.message}",
            )
            probe.exceptionOrNull()?.let { throw it }
        }
    }

    fun listApps(): List<InstalledApp> {
        val svc = startService("com.apple.mobile.installation_proxy")
        val conn = serviceRequest(
            svc,
            mapOf(
                    "Command" to "Browse",
                    "ClientOptions" to mapOf(
                    "ApplicationType" to "Any",
                    "ReturnAttributes" to listOf("CFBundleIdentifier", "CFBundleDisplayName", "CFBundleName", "CFBundleShortVersionString", "CFBundleVersion", "IsPlaceholder", "iTunesMetadata", "iTunesStoreItemIdentifier"),
                ),
            ),
        )
        val apps = mutableListOf<InstalledApp>()
        val list = conn.objectForKey("CurrentList") as? com.dd.plist.NSArray
        if (list != null) {
            for (i in 0 until list.count()) {
                val d = list.objectAtIndex(i) as? NSDictionary ?: continue
                val bid = d.str("CFBundleIdentifier") ?: continue
                apps += InstalledApp(
                    bundleId = bid,
                    name = d.str("CFBundleDisplayName") ?: d.str("CFBundleName"),
                    version = d.str("CFBundleShortVersionString"),
                    appStoreId = d.str("iTunesStoreItemIdentifier") ?: d.dict("iTunesMetadata")?.str("itemId")
                        ?: d.data("iTunesMetadata")?.let(::itemIdFromMetadata),
                    isPlaceholder = placeholderFlag(d.objectForKey("IsPlaceholder")),
                )
            }
        }
        return apps
    }

    fun startService(name: String): Int {
        val resp = request(mapOf("Request" to "StartService", "Service" to name))
        val err = resp.str("Error")
        if (err != null) error("Не удалось подключиться к iPhone")
        return resp.objectForKey("Port")?.toString()?.toIntOrNull()
            ?: error("Не удалось подключиться к iPhone")
    }

    private fun validatePair(udid: String) {
        val rec = Plist.parse(pairingFile(udid).readBytes())
        val record = mutableMapOf<String, Any?>()
        for (key in listOf("HostID", "SystemBUID", "HostCertificate", "RootCertificate", "DeviceCertificate")) {
            rec.data(key)?.let { record[key] = it } ?: rec.str(key)?.let { record[key] = it }
        }
        if (record["HostID"] == null) return
        val resp = request(mapOf("Request" to "ValidatePair", "PairRecord" to record))
        val err = resp.str("Error")
        if (!err.isNullOrBlank()) android.util.Log.w("SignumPair", "ValidatePair $err")
        else android.util.Log.i("SignumPair", "ValidatePair ok")
    }

    private fun stringValue(key: String): String? {
        val r = getValue(key = key)
        return r.str("Value") ?: r.str(key)
    }

    private fun request(fields: Map<String, Any?>): NSDictionary {
        val merged = linkedMapOf<String, Any?>("Label" to "Signum")
        sessionId?.let { merged["SessionID"] = it }
        merged.putAll(fields)
        val dict = Plist.dict(*merged.entries.map { it.key to it.value }.toTypedArray())
        val xml = String(Plist.xml(dict), Charsets.UTF_8)
            .replace(Regex("<!DOCTYPE[^>]*>\\s*", RegexOption.IGNORE_CASE), "")
            .toByteArray()
        val header = ByteBuffer.allocate(4).putInt(xml.size).array()
        writeAll(header + xml)
        val lenBytes = readAll(4, 12_000)
        val len = ByteBuffer.wrap(lenBytes).int
        if (len <= 0 || len > 8_000_000) error("Повреждённый ответ iPhone")
        val payload = readAll(len, 20_000)
        return Plist.parse(payload)
    }

    private fun writeAll(data: ByteArray) {
        (tls ?: RawPipe(stream)).write(data)
    }

    private fun readAll(count: Int, timeoutMs: Int): ByteArray {
        return (tls ?: RawPipe(stream)).readExact(count, timeoutMs)
    }

    private fun serviceRequest(port: Int, fields: Map<String, Any?>): NSDictionary {
                                                                                     
        return request(fields)
    }

    fun pairingFile(udid: String) = java.io.File(store.pairingDir(), "$udid.plist")

    private fun loadOrCreateBuid(): String {
        val file = java.io.File(store.filesDir(), "system_buid.txt")
        if (file.exists()) {
            val old = file.readText().trim()
            if (old.isNotEmpty()) return old
        }
        val created = UUID.randomUUID().toString().uppercase()
        file.writeText(created)
        return created
    }

    private fun pemCert(cert: X509Certificate): ByteArray = pemBlock("CERTIFICATE", cert.encoded)

    private fun pemKey(key: PrivateKey): ByteArray = pemBlock("PRIVATE KEY", key.encoded)

    private fun pemBlock(type: String, der: ByteArray): ByteArray {
        val b64 = android.util.Base64.encodeToString(der, android.util.Base64.NO_WRAP)
        val lines = b64.chunked(64).joinToString("\n")
        return "-----BEGIN $type-----\n$lines\n-----END $type-----\n".toByteArray()
    }

    private fun generateKeys(): KeyPair {
        val g = KeyPairGenerator.getInstance("RSA")
        g.initialize(2048)
        return g.generateKeyPair()
    }

    private fun readDevicePublicKey(): PublicKey {
        val raw = getValue(key = "DevicePublicKey")
        val bytes = raw.data("Value")
            ?: raw.str("Value")?.toByteArray(Charsets.US_ASCII)
            ?: error("Не удалось получить данные iPhone")
        val head = bytes.take(24).toByteArray().toString(Charsets.US_ASCII)
        android.util.Log.i("SignumPair", "DevicePublicKey size=${bytes.size} head=$head")
        return parseRsaPublic(bytes)
    }

    private fun parseRsaPublic(bytes: ByteArray): PublicKey {
        val text = runCatching { bytes.toString(Charsets.US_ASCII) }.getOrDefault("")
        val der = if (text.contains("BEGIN")) decodePem(text) else bytes
        runCatching {
            KeyFactory.getInstance("RSA").generatePublic(X509EncodedKeySpec(der))
        }.getOrNull()?.let { return it }
        val rsa = RSAPublicKey.getInstance(ASN1Primitive.fromByteArray(der))
        return KeyFactory.getInstance("RSA").generatePublic(RSAPublicKeySpec(rsa.modulus, rsa.publicExponent))
    }

    private fun decodePem(text: String): ByteArray {
        val b64 = text.lineSequence()
            .filter { it.isNotBlank() && !it.contains("BEGIN") && !it.contains("END") }
            .joinToString("")
        return android.util.Base64.decode(b64, android.util.Base64.DEFAULT)
    }

    private fun selfSigned(keys: KeyPair, cn: String, ca: Boolean): X509Certificate {
        return signedCert(keys, null, keys.public, cn, ca)
    }

    private fun signedCert(
        issuerKeys: KeyPair,
        issuerCert: X509Certificate?,
        subjectKey: PublicKey,
        cn: String,
        ca: Boolean,
    ): X509Certificate {
        val now = Date()
        val until = Date(now.time + 3650L * 24 * 3600 * 1000)
        val subject = X500Name("CN=$cn")
        val issuer = if (issuerCert != null) X500Name(issuerCert.subjectX500Principal.name) else subject
        val serial = BigInteger(64, SecureRandom()).abs().max(BigInteger.ONE)
        val builder = JcaX509v3CertificateBuilder(
            issuer,
            serial,
            now,
            until,
            subject,
            subjectKey,
        )
        builder.addExtension(Extension.basicConstraints, true, BasicConstraints(ca))
        if (!ca) {
            builder.addExtension(
                Extension.keyUsage,
                true,
                KeyUsage(KeyUsage.digitalSignature or KeyUsage.keyEncipherment),
            )
            val ski = JcaX509ExtensionUtils().createSubjectKeyIdentifier(subjectKey)
            builder.addExtension(Extension.subjectKeyIdentifier, false, ski)
        }
        val signer = JcaContentSignerBuilder("SHA256WithRSA").build(issuerKeys.private)
        return JcaX509CertificateConverter().getCertificate(builder.build(signer))
    }

    companion object {
        fun trustAll(): X509TrustManager = object : X509TrustManager {
            override fun checkClientTrusted(c: Array<X509Certificate>, a: String) {}
            override fun checkServerTrusted(c: Array<X509Certificate>, a: String) {}
            override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
        }

        fun trustAllContext(): SSLContext {
            val ctx = SSLContext.getInstance("TLS")
            ctx.init(null, arrayOf<TrustManager>(trustAll()), SecureRandom())
            return ctx
        }
    }
}

class DeviceService(
    private val context: android.content.Context,
    private val store: SettingsStore,
) {
    @Volatile
    private var claimed: ClaimedAppleUsb? = null

    @Volatile
    var onUsbChange: (() -> Unit)? = null

    @Volatile
    var hostActivity: android.app.Activity? = null

    @Volatile
    private var sessionBusy = false

    private val usbLock = Any()
    private val permissionWaiters = java.util.concurrent.CopyOnWriteArrayList<(Boolean) -> Unit>()

    fun notifyUsbChanged() {
        onUsbChange?.invoke()
    }

    fun onUsbIntent(action: String, device: android.hardware.usb.UsbDevice?, granted: Boolean) {
        if (action == USB_PERMISSION) {
            val waiters = permissionWaiters.toList()
            permissionWaiters.clear()
            waiters.forEach { it(granted) }
        }
        if (sessionBusy) return
        if (action == android.hardware.usb.UsbManager.ACTION_USB_DEVICE_ATTACHED &&
            device != null &&
            AppleUsb.looksLikeIphone(device) &&
            !AppleUsb.hasPermission(context, device)
        ) {
            runCatching { AppleUsb.requestPermission(hostActivity ?: context, device) }
        }
        onUsbChange?.invoke()
    }

    fun attached(): List<UsbPreview> =
        AppleUsb.listAppleDevices(context).map {
            UsbPreview(
                name = it.productName ?: "iPhone",
                serial = runCatching { it.serialNumber }.getOrNull(),
                deviceName = it.deviceName,
                hasPermission = AppleUsb.hasPermission(context, it),
            )
        }

    fun requestAccess(): String {
        val apple = AppleUsb.listAppleDevices(context)
        if (apple.isEmpty()) {
            val others = AppleUsb.allDevices(context)
            return if (others.isEmpty()) {
                "iPhone не виден. Подключите разблокированный iPhone кабелем USB‑C и повторите."
            } else {
                "iPhone не найден. Отключите другие USB-устройства и подключите iPhone напрямую."
            }
        }
        val first = apple.first()
        if (AppleUsb.hasPermission(context, first)) {
            return "USB уже разрешён: ${first.productName ?: "iPhone"}"
        }
        return try {
            AppleUsb.requestPermission(hostActivity ?: context, first)
            "Разрешите доступ к «${first.productName ?: "iPhone"}» в системном окне и не сворачивайте Signum."
        } catch (e: Exception) {
            "Не удалось запросить USB: ${e.message ?: e.javaClass.simpleName}"
        }
    }

    fun withDevice(
        onStatus: (String) -> Unit = {},
        forcePair: Boolean = false,
        block: (LockdownClient, UsbMux) -> Unit,
    ): DeviceInfo {
        synchronized(usbLock) {
            var devices = AppleUsb.listAppleDevices(context)
            if (devices.isEmpty()) {
                onStatus("Ждём iPhone по USB…")
                Thread.sleep(500)
                devices = AppleUsb.listAppleDevices(context)
            }
            if (devices.isEmpty()) {
                error("iPhone не найден. Подключите устройство разблокированным кабелем USB.")
            }
            val selectedUdid = store.load().selectedDeviceUdid
            var device = selectedUdid?.let { wanted ->
                devices.firstOrNull { dev -> runCatching { dev.serialNumber == wanted }.getOrDefault(false) }
            } ?: devices.first()
            if (!AppleUsb.hasPermission(context, device)) {
                onStatus("Разрешите доступ к USB на Android…")
                if (!waitForPermission(device)) {
                    error("Разрешите доступ к USB на Android и повторите. Не сворачивайте окно Signum.")
                }
                Thread.sleep(350)
                device = AppleUsb.resolve(context, device)
            }
            sessionBusy = true
            var last: Exception? = null
            try {
                repeat(4) { attempt ->
                    onStatus(if (attempt == 0) "Подключаем iPhone…" else "Связь сбросилась, подключаемся снова… (${attempt + 1}/4)")
                    val live = AppleUsb.resolve(context, device)
                    val claim = AppleUsb.claim(context, live)
                    claimed = claim
                    try {
                        val mux = UsbMux(claim)
                        onStatus("Связь с iPhone…")
                        Thread.sleep(300L + attempt * 200L)
                        mux.handshake()
                        val stream = mux.connect(UsbMux.LOCKDOWN_PORT)
                        Thread.sleep(80)
                        val lock = LockdownClient(store, stream)
                        lock.queryType()
                        val info = lock.pair(force = forcePair, onStatus = onStatus)
                        val selected = store.load().selectedDeviceUdid
                        if (selected.isNullOrBlank()) {
                            store.save(store.load().copy(selectedDeviceUdid = info.udid))
                        }
                        block(lock, mux)
                        return info
                    } catch (e: Exception) {
                        last = e
                        android.util.Log.w("SignumUsb", "session try=${attempt + 1}", e)
                        AppleUsb.release(claim)
                        claimed = null
                        if (!isUsbReset(e) || attempt == 3) throw e
                        Thread.sleep(500L * (attempt + 1))
                    }
                }
                throw last ?: IllegalStateException("Не удалось связаться с iPhone")
            } finally {
                claimed?.let { AppleUsb.release(it) }
                claimed = null
                sessionBusy = false
            }
        }
    }

    private fun isUsbReset(e: Exception): Boolean {
        if (e is UsbResetException) return true
        val msg = e.message.orEmpty()
        return msg.contains("сброс", true) || msg.contains("reset", true) ||
            msg.contains("DETACH", true) || msg.contains("timeout", true)
    }

    private fun waitForPermission(device: android.hardware.usb.UsbDevice): Boolean {
        if (AppleUsb.hasPermission(context, device)) return true
        val latch = java.util.concurrent.CountDownLatch(1)
        val granted = java.util.concurrent.atomic.AtomicBoolean(false)
        val cb: (Boolean) -> Unit = { ok ->
            granted.set(ok)
            latch.countDown()
        }
        permissionWaiters.add(cb)
        return try {
            AppleUsb.requestPermission(hostActivity ?: context, device)
            val arrived = latch.await(90, java.util.concurrent.TimeUnit.SECONDS)
            val now = AppleUsb.listAppleDevices(context).any { AppleUsb.hasPermission(context, it) }
            (arrived && granted.get()) || now
        } catch (e: Exception) {
            android.util.Log.e("SignumUsb", "requestPermission", e)
            false
        } finally {
            permissionWaiters.remove(cb)
        }
    }

    fun listDevices(): List<pw.appbank.signum.data.models.DeviceInfo> {
        return AppleUsb.listAppleDevices(context).map { dev ->
            val serial = runCatching { dev.serialNumber }.getOrNull()?.takeIf { it.isNotBlank() }
            val udid = serial ?: "usb-${dev.deviceId}"
            val pairingExists = serial != null && java.io.File(store.pairingDir(), "$serial.plist").exists()
            pw.appbank.signum.data.models.DeviceInfo(
                udid = udid,
                productType = AppleUsb.describe(dev),
                productVersion = runCatching {
                    val claim = AppleUsb.claim(context, dev)
                    try {
                        val mux = UsbMux(claim)
                        mux.handshake()
                        LockdownClient(store, mux.connect(UsbMux.LOCKDOWN_PORT)).readInfo().productVersion
                    } finally { AppleUsb.release(claim) }
                }.getOrNull(),
                deviceName = dev.productName ?: "iPhone",
                connection = "usb",
                paired = pairingExists,
            )
        }
    }

    fun trust(): String {
        var msg = "Доверие установлено"
        val info = withDevice(onStatus = { status -> msg = status }, forcePair = true) { _, _ -> }
        msg = "Доверие установлено: ${info.deviceName ?: "iPhone"}. Можно ставить приложения."
        return msg
    }

    fun installedApps(): List<InstalledApp> {
        var apps: List<InstalledApp> = emptyList()
        withDevice { lock, mux ->
            val info = lock.readInfo()
            lock.startSession(info.udid, mux)
            val port = lock.startService("com.apple.mobile.installation_proxy")
            val svc = mux.connect(port)
            val proxy = ServicePlist(svc)
            apps = proxy.browseApps()
        }
        return apps
    }

    data class UsbPreview(
        val name: String,
        val serial: String?,
        val deviceName: String,
        val hasPermission: Boolean,
    )
}

class ServicePlist(private val stream: UsbMux.MuxStream) {
    fun send(fields: Map<String, Any?>) {
        val xml = Plist.xml(Plist.dict(*fields.entries.map { it.key to it.value }.toTypedArray()))
        val header = ByteBuffer.allocate(4).putInt(xml.size).array()
        stream.write(header + xml)
    }

    fun recv(timeout: Int = 20_000): NSDictionary {
        val lenBytes = stream.readExact(4, timeout)
        val len = ByteBuffer.wrap(lenBytes).int
        val payload = stream.readExact(len, timeout)
        return Plist.parse(payload)
    }

    fun browseApps(): List<InstalledApp> {
        send(
            mapOf(
                    "Command" to "Browse",
                    "ClientOptions" to mapOf(
                    "ApplicationType" to "Any",
                    "ReturnAttributes" to listOf(
                        "CFBundleIdentifier",
                        "CFBundleName",
                        "CFBundleDisplayName",
                        "CFBundleShortVersionString",
                        "CFBundleVersion",
                        "IsPlaceholder",
                        "iTunesMetadata",
                        "iTunesStoreItemIdentifier",
                    ),
                ),
            ),
        )
        val apps = mutableListOf<InstalledApp>()
        repeat(40) {
            val dict = runCatching { recv(15_000) }.getOrNull() ?: return apps
            val status = dict.str("Status")
            val list = dict.objectForKey("CurrentList") as? com.dd.plist.NSArray
            if (list != null) {
                for (i in 0 until list.count()) {
                    val d = list.objectAtIndex(i) as? NSDictionary ?: continue
                    val bid = d.str("CFBundleIdentifier") ?: continue
                    apps += InstalledApp(
                        bundleId = bid,
                        name = d.str("CFBundleDisplayName"),
                        version = d.str("CFBundleShortVersionString"),
                        appStoreId = d.str("iTunesStoreItemIdentifier") ?: d.dict("iTunesMetadata")?.str("itemId")
                            ?: d.data("iTunesMetadata")?.let(::itemIdFromMetadata),
                        isPlaceholder = placeholderFlag(d.objectForKey("IsPlaceholder")),
                    )
                }
            }
            if (status == "Complete") return apps
        }
        return apps
    }
}

internal fun placeholderFlag(value: NSObject?): Boolean = when (value) {
    is NSNumber -> value.boolValue()
    else -> value?.toString()?.trim()?.let { it.equals("true", true) || it == "1" } == true
}

internal fun itemIdFromMetadata(bytes: ByteArray): String? = runCatching {
    val dict = Plist.parse(bytes)
    fun find(value: Any?): String? = when (value) {
        is NSDictionary -> sequenceOf("itemId", "iTunesStoreItemIdentifier", "trackId")
            .mapNotNull { key -> value.str(key)?.takeIf { id -> id.isNotEmpty() && id.all { it in '0'..'9' } } }
            .firstOrNull()
            ?: value.allKeys().asSequence().map { value.objectForKey(it) }
                .filter { it is NSDictionary || it is com.dd.plist.NSArray }
                .mapNotNull { find(it) }.firstOrNull()
        is com.dd.plist.NSArray -> (0 until value.count()).asSequence()
            .mapNotNull { find(value.objectAtIndex(it)) }.firstOrNull()
        else -> null
    }
    find(dict)
}.getOrNull()

private interface BytePipe {
    fun write(data: ByteArray)
    fun readExact(count: Int, timeoutMs: Int): ByteArray
}

private class RawPipe(private val stream: UsbMux.MuxStream) : BytePipe {
    override fun write(data: ByteArray) = stream.write(data)
    override fun readExact(count: Int, timeoutMs: Int) = stream.readExact(count, timeoutMs)
}

private class TlsPipe(private val tls: BcTlsPipe) : BytePipe {
    override fun write(data: ByteArray) = tls.write(data)
    override fun readExact(count: Int, timeoutMs: Int) = tls.readExact(count, timeoutMs)

    companion object {
        fun start(raw: UsbMux.MuxStream, pairing: java.io.File): TlsPipe {
            val rec = Plist.parse(pairing.readBytes())
            val hostCertDer = decodePemOrDer(
                rec.data("HostCertificate") ?: error("Не удалось прочитать данные доверия"),
                "CERTIFICATE",
            )
            val rootCertDer = rec.data("RootCertificate")?.let { decodePemOrDer(it, "CERTIFICATE") }
            val hostKey = keyFrom(rec.data("HostPrivateKey") ?: error("Не удалось прочитать данные доверия"))
            val hostCert = certFrom(rec.data("HostCertificate") ?: error("Не удалось прочитать данные доверия"))
            pw.appbank.signum.data.api.DebugDump.pair(
                "tls cert=${hostCert.subjectX500Principal} der=${hostCertDer.size}b key=${hostKey.algorithm}",
            )
            val proto = lockdownTlsConnect(raw, hostCertDer, hostKey, rootCertDer)
            return TlsPipe(BcTlsPipe(proto))
        }

        private fun certFrom(bytes: ByteArray): X509Certificate {
            val der = decodePemOrDer(bytes, "CERTIFICATE")
            return CertificateFactory.getInstance("X.509")
                .generateCertificate(ByteArrayInputStream(der)) as X509Certificate
        }

        private fun keyFrom(bytes: ByteArray): PrivateKey {
            val der = decodePemOrDer(bytes, "PRIVATE KEY")
            return KeyFactory.getInstance("RSA").generatePrivate(PKCS8EncodedKeySpec(der))
        }

        private fun decodePemOrDer(bytes: ByteArray, type: String): ByteArray {
            val text = runCatching { String(bytes, Charsets.US_ASCII) }.getOrDefault("")
            if (!text.contains("BEGIN")) return bytes
            val b64 = text.lineSequence()
                .filter { it.isNotBlank() && !it.contains("BEGIN") && !it.contains("END") }
                .joinToString("")
            return android.util.Base64.decode(b64, android.util.Base64.DEFAULT)
        }
    }
}
