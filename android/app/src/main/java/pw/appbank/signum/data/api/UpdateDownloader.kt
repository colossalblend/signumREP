package pw.appbank.signum.data.api

import okhttp3.HttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.security.MessageDigest

internal object UpdateDownloader {
    fun download(client: OkHttpClient, url: HttpUrl, target: File, sha256: String?, progress: (Float) -> Unit): File {
        val expected = sha256?.trim()?.lowercase().orEmpty()
        require(expected.isEmpty() || expected.matches(Regex("[a-f0-9]{64}"))) { "Обновление не прошло проверку целостности." }
        val partial = File(target.parentFile, "${target.name}.part")
        if (expected.isEmpty()) partial.delete()
        var etag: String? = null
        var lastError: IOException? = null
        repeat(3) {
            try {
                var done = partial.length()
                val request = Request.Builder().url(url).header("Accept-Encoding", "identity")
                    .header("Cache-Control", "no-cache").apply {
                        if (done > 0) {
                            header("Range", "bytes=$done-")
                            etag?.let { header("If-Range", it) }
                        }
                    }.build()
                client.newCall(request).execute().use { response ->
                    check(response.request.url.host == url.host) { "Не удалось проверить адрес обновления." }
                    if (response.code == 416 && expected.isNotEmpty()
                        && response.header("Content-Range") == "bytes */$done") {
                        return finish(partial, target, expected, progress)
                    }
                    if (!response.isSuccessful) throw IOException("Не удалось скачать обновление (${response.code}).")
                    val body = response.body ?: throw IOException("Пустой ответ сервера.")
                    val resumed = response.code == 206
                    val range = response.header("Content-Range")?.let { Regex("bytes (\\d+)-(\\d+)/(\\d+)").matchEntire(it) }
                    if (resumed && (range == null || range.groupValues[1].toLongOrNull() != done)) {
                        partial.delete()
                        throw IOException("Не удалось продолжить загрузку обновления.")
                    }
                    if (!resumed) done = 0
                    val total = if (resumed) range!!.groupValues[3].toLong() else body.contentLength()
                    etag = response.header("ETag")
                    FileOutputStream(partial, resumed).use { output ->
                        body.byteStream().use { input ->
                            val buffer = ByteArray(64 * 1024)
                            while (true) {
                                val read = input.read(buffer)
                                if (read < 0) break
                                output.write(buffer, 0, read)
                                done += read
                                if (total > 0) progress((done * 100f / total).coerceAtMost(99f))
                            }
                        }
                        output.flush()
                        output.fd.sync()
                    }
                    if (done == 0L || (total > 0 && done != total)) throw IOException("Загрузка обновления прервалась. Пробуем продолжить…")
                    return finish(partial, target, expected, progress)
                }
            } catch (error: IOException) {
                lastError = error
            }
        }
        throw lastError ?: IOException("Не удалось скачать обновление. Попробуйте ещё раз.")
    }

    private fun finish(partial: File, target: File, expected: String, progress: (Float) -> Unit): File {
        if (expected.isNotEmpty()) {
            val digest = MessageDigest.getInstance("SHA-256")
            partial.inputStream().use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    digest.update(buffer, 0, read)
                }
            }
            val actual = digest.digest().joinToString("") { "%02x".format(it) }
            if (actual != expected) {
                partial.delete()
                error("Обновление скачалось повреждённым. Повторите загрузку.")
            }
        }
        target.delete()
        check(partial.renameTo(target)) { "Не удалось сохранить обновление. Повторите загрузку." }
        progress(100f)
        return target
    }
}
