package pw.appbank.signum.data.apple

import android.util.Base64
import android.util.Log
import pw.appbank.signum.data.settings.SettingsStore
import java.io.File
import java.security.MessageDigest

/**
 * Produces the FairPlay SAP signature Apple requires on protected App Store
 * actions (login `authenticate`, purchase, download) via the bundled native
 * helper `libsapsigner.so` (a Go + Unicorn emulator shipped in jniLibs).
 *
 * The helper reads the request body on stdin and writes the raw signature on
 * stdout; [headerValue] base64-encodes it for `X-Apple-ActionSignature`.
 *
 * The emulator needs five Apple FairPlay artifacts. They are shipped in
 * `assets/sap/` and extracted once into a writable cache dir, which is handed to
 * the helper through the `SAPSIGNER_CACHE` environment variable.
 */
class SapSigner(
    private val store: SettingsStore,
) {
    private val filesDir: File = store.filesDir()
    private val nativeLibDir: File? = store.nativeLibDir().let(::File)
    private val cacheDir: File = File(filesDir, "sap-cache")

    fun useAccountIdentity(email: String?, configurator: Boolean) {
        if (!configurator || email.isNullOrBlank()) return
        val digest = MessageDigest.getInstance("SHA-256").digest(email.trim().lowercase().toByteArray())
        val hex = digest.take(6).joinToString("") { "%02x".format(it) }
        val mac = (hex.substring(0, 2).toInt(16) and 0xFC or 0x02).toString(16).padStart(2, '0') + hex.substring(2)
        runCatching { File(filesDir, "sap-mac-${email.trim().lowercase().hashCode()}.txt").writeText(mac) }
        accountMac = mac
    }

    private var accountMac: String? = null

    fun available(): Boolean = resolveHelper() != null

    fun sign(data: ByteArray): ByteArray {
        val helper = resolveHelper()
            ?: error("Нет SAP-подписи. Переустановите Signum с компонентом sapsigner.")
        ensureArtifacts()
        val pb = ProcessBuilder(helper.absolutePath)
        pb.environment().apply {
            put("SAPSIGNER_CACHE", cacheDir.absolutePath)
            put("TMPDIR", cacheDir.absolutePath)
            put("HOME", filesDir.absolutePath)
            // Android's untrusted_app SELinux domain blocks netlink route sockets.
            // The helper's guid.Get() calls net.Interfaces() (netlink) for a MAC and
            // panics with "netlinkrib: permission denied"; hand it a stable id so it
            // never enumerates interfaces.
            put("SAPSIGNER_MAC", accountMac ?: deviceMac())
            // Same reason for DNS: force the cgo resolver (Bionic getaddrinfo),
            // which skips Go's RFC6724 sorting (also netlink-backed).
            put("GODEBUG", "netdns=cgo")
        }
        val proc = pb.start()
        // Drain stderr on a separate thread so a chatty helper cannot fill the
        // pipe buffer and deadlock against our stdout read.
        var stderrBytes = ByteArray(0)
        val errThread = Thread { stderrBytes = runCatching { proc.errorStream.readBytes() }.getOrDefault(ByteArray(0)) }
        errThread.start()
        proc.outputStream.use { out ->
            out.write(data)
            out.flush()
        }
        val stdout = proc.inputStream.readBytes()
        val code = proc.waitFor()
        errThread.join()
        if (code != 0 || stdout.isEmpty()) {
            val err = stderrBytes.toString(Charsets.UTF_8).trim()
            error(if (err.isNotEmpty()) err else "Не удалось подписать запрос App Store (SAP).")
        }
        return stdout
    }

    fun headerValue(data: ByteArray): String =
        Base64.encodeToString(sign(data), Base64.NO_WRAP)

    /** Stable 12-hex-char (6-byte) id for the helper's FairPlay hwInfo. */
    private fun deviceMac(): String {
        accountMac?.let { return it }
        val file = File(filesDir, "sap-mac.txt")
        runCatching {
            val existing = file.readText().trim()
            if (existing.length == 12 && existing.all { it.isDigit() || it in 'a'..'f' || it in 'A'..'F' }) {
                return existing.lowercase()
            }
        }
        val bytes = ByteArray(6).also { java.security.SecureRandom().nextBytes(it) }
        bytes[0] = (bytes[0].toInt() and 0xFE or 0x02).toByte() // locally-administered, unicast
        val hex = bytes.joinToString("") { "%02x".format(it) }
        runCatching { file.writeText(hex) }
        return hex
    }

    /** Extract the FairPlay artifacts from assets on first use. */
    private fun ensureArtifacts() {
        synchronized(artifactLock) {
            if (!cacheDir.exists()) cacheDir.mkdirs()
            for (name in ARTIFACTS) {
                val target = File(cacheDir, name)
                if (target.isFile && target.length() > 0) continue
                runCatching {
                    val tmp = File(cacheDir, "$name.tmp")
                    // AssetManager transparently inflates the (APK-compressed) asset.
                    store.openAsset("sap/$name").use { input ->
                        tmp.outputStream().use { output -> input.copyTo(output) }
                    }
                    if (!tmp.renameTo(target)) {
                        tmp.copyTo(target, overwrite = true)
                        tmp.delete()
                    }
                }.onFailure {
                    // Non-fatal: the helper falls back to fetching from Apple's CDN.
                    Log.w("SignumStore", "sap artifact $name not extracted: ${it.message}")
                }
            }
        }
    }

    private fun resolveHelper(): File? {
        val names = listOf("libsapsigner.so", "sapsigner")
        val dirs = listOfNotNull(nativeLibDir, File(filesDir, "sap"), filesDir)
        for (dir in dirs) {
            for (name in names) {
                val candidate = File(dir, name)
                if (candidate.isFile && candidate.length() > 0) {
                    if (!candidate.canExecute()) {
                        candidate.setExecutable(true, false)
                    }
                    if (candidate.canExecute()) return candidate
                }
            }
        }
        return null
    }

    private companion object {
        val ARTIFACTS = listOf("CommerceKit", "storeagent", "CommerceCore", "CoreFP", "CoreFP.icxs")
        val artifactLock = Any()
    }
}
