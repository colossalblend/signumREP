package pw.appbank.signum.data.api

import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.net.ServerSocket
import java.security.MessageDigest
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

class UpdateDownloaderTest {
    private val payload = "0123456789".toByteArray()
    private val checksum = MessageDigest.getInstance("SHA-256").digest(payload).joinToString("") { "%02x".format(it) }

    @Test fun resumesInterruptedBodyAndRestartsIfRangeIgnored() {
        for (supportsRange in listOf(true, false)) {
            ServerSocket(0).use { server ->
                server.soTimeout = 5000
                var secondRequest = ""
                val worker = thread {
                    repeat(2) { attempt ->
                        server.accept().use { socket ->
                            socket.soTimeout = 5000
                            val reader = socket.getInputStream().bufferedReader()
                            val request = buildString {
                                while (true) {
                                    val line = reader.readLine() ?: break
                                    if (line.isEmpty()) break
                                    appendLine(line)
                                }
                            }
                            val output = socket.getOutputStream()
                            if (attempt == 0) {
                                output.write("HTTP/1.1 200 OK\r\nContent-Length: 10\r\nETag: \"test\"\r\nConnection: close\r\n\r\n0123".toByteArray())
                            } else {
                                secondRequest = request
                                output.write((if (supportsRange) "HTTP/1.1 206 Partial Content\r\nContent-Length: 6\r\nContent-Range: bytes 4-9/10\r\nConnection: close\r\n\r\n456789"
                                else "HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789").toByteArray())
                            }
                            output.flush()
                        }
                    }
                }
                val directory = java.nio.file.Files.createTempDirectory("signum-update-test").toFile()
                try {
                    val target = File(directory, "update.apk")
                    var percent = 0f
                    val client = OkHttpClient.Builder().readTimeout(2, TimeUnit.SECONDS).build()
                    UpdateDownloader.download(client, "http://127.0.0.1:${server.localPort}/update.apk".toHttpUrl(), target, checksum) { percent = it }
                    worker.join(5000)
                    assertFalse(worker.isAlive)
                    assertTrue(secondRequest.lowercase().contains("range: bytes=4-"))
                    assertArrayEquals(payload, target.readBytes())
                    assertEquals(100f, percent, 0.01f)
                    assertFalse(File(directory, "update.apk.part").exists())
                } finally { directory.deleteRecursively() }
            }
        }
    }

    @Test fun rejectsCorruptUpdateBeforeInstallation() {
        ServerSocket(0).use { server ->
            val worker = thread {
                server.accept().use { socket ->
                    val reader = socket.getInputStream().bufferedReader()
                    while (!reader.readLine().isNullOrEmpty()) { }
                    socket.getOutputStream().write("HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789".toByteArray())
                }
            }
            val directory = java.nio.file.Files.createTempDirectory("signum-update-corrupt").toFile()
            try {
                val target = File(directory, "update.apk")
                try {
                    UpdateDownloader.download(OkHttpClient(), "http://127.0.0.1:${server.localPort}/update.apk".toHttpUrl(), target, "0".repeat(64)) { }
                    fail("Corrupt update accepted")
                } catch (_: IllegalStateException) { }
                assertFalse(target.exists())
                assertFalse(File(directory, "update.apk.part").exists())
                worker.join(5000)
            } finally { directory.deleteRecursively() }
        }
    }
}
