package pw.appbank.signum.data.api

import android.util.Log
import java.io.File

object DebugDump {
    @Volatile
    var dir: File? = null

    fun write(name: String, bytes: ByteArray) {
        val folder = dir ?: return
        runCatching {
            folder.mkdirs()
            File(folder, name).writeBytes(bytes)
        }
    }

    fun writeText(name: String, text: String) = write(name, text.toByteArray())

    fun appendText(name: String, text: String) {
        val folder = dir ?: return
        runCatching {
            folder.mkdirs()
            File(folder, name).appendText(text + "\n")
        }
    }

    fun pair(text: String) {
        Log.e("SignumPair", text)
        appendText("usb-tls.txt", "${System.currentTimeMillis()} $text")
    }

    fun hexPreview(bytes: ByteArray, n: Int = 48): String =
        bytes.take(n).joinToString("") { "%02x".format(it) }

    fun logFail(tag: String, bytes: ByteArray, e: Throwable? = null) {
        val text = runCatching { bytes.toString(Charsets.UTF_8) }.getOrDefault("")
        val preview = text.replace("\n", " ").take(200)
        Log.e(tag, "fail ${bytes.size}b hex=${hexPreview(bytes)} text=$preview", e)
        write("last-fail.bin", bytes)
        writeText("last-fail.txt", "size=${bytes.size}\nhex=${hexPreview(bytes, 80)}\n\n$preview")
    }
}
