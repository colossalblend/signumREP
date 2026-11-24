package pw.appbank.signum.data.device

import android.graphics.Bitmap
import android.graphics.Color
import pw.appbank.signum.data.apple.Plist
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.NetworkInterface
import java.net.ServerSocket
import java.net.Socket
import java.net.URLDecoder
import java.net.URLEncoder
import java.util.concurrent.atomic.AtomicReference

/**
 * Wi-Fi сервер установки: Signum поднимает локальный HTTP-сервер и раздаёт
 * страницу + OTA-manifest + IPA. iPhone открывает ссылку (или сканирует QR),
 * жмёт «Установить» — iOS ставит приложение через itms-services.
 */
class InstallServer {
    data class Serving(
        val ipa: File,
        val title: String,
        val bundleId: String,
        val version: String,
        val port: Int,
        val url: String,
        val startedAt: Long = System.currentTimeMillis(),
    ) {
        val manifestUrl: String get() = "$url/manifest.plist"
        val installUrl: String
            get() = "itms-services://?action=install-manifest&url=" +
                URLEncoder.encode(manifestUrl, "UTF-8")
    }

    private val serving = AtomicReference<Serving?>(null)
    private var serverSocket: ServerSocket? = null

    val current: Serving? get() = serving.get()

    @Synchronized
    fun start(ipa: File, title: String, bundleId: String, version: String): Serving {
        stop()
        val ip = bestLanIp()
            ?: error("Android не подключён к Wi-Fi (локальный IP не найден). Подключитесь к одной сети с iPhone.")
        val ss = ServerSocket(0, 8, InetAddress.getByName("0.0.0.0"))
        val s = Serving(ipa, title.ifBlank { "Приложение" }, bundleId, version, ss.localPort, "http://$ip:${ss.localPort}")
        serverSocket = ss
        serving.set(s)
        Thread({ acceptLoop(ss) }, "signum-install-server").start()
        return s
    }

    @Synchronized
    fun stop() {
        runCatching { serverSocket?.close() }
        serverSocket = null
        serving.set(null)
    }

    private fun acceptLoop(ss: ServerSocket) {
        while (true) {
            val client = try {
                ss.accept()
            } catch (e: java.io.IOException) {
                return // сервер остановлен
            }
            Thread({ handle(client) }, "signum-install-conn").start()
        }
    }

    private fun handle(socket: Socket) {
        try {
            socket.soTimeout = 15_000
            val input = socket.getInputStream()
            val request = readRequestLine(input) ?: return socket.close().let { Unit }
            val path = request.substringBefore('?').let { URLDecoder.decode(it, "UTF-8") }
            val rangeHeader = readHeader(input, "Range")
            val out = socket.getOutputStream()
            when (path) {
                "/", "/index.html" -> servePage(out)
                "/manifest.plist" -> serveManifest(out)
                "/qr.png" -> serveQr(out)
                "/app.ipa" -> serveIpa(out, rangeHeader)
                else -> respond(out, 404, "text/plain; charset=utf-8", "not found".toByteArray())
            }
            out.flush()
        } catch (e: java.io.IOException) {
            // iPhone оборвал соединение (ушёл в сон, отменил загрузку) — это норма.
        } finally {
            runCatching { socket.close() }
        }
    }

    private fun readRequestLine(input: InputStream): String? {
        val sb = StringBuilder()
        while (sb.length < 8192) {
            val b = input.read()
            if (b < 0) break
            if (b == '\n'.code) break
            sb.append(b.toChar())
        }
        val line = sb.toString().trim()
        return line.takeIf { it.startsWith("GET ", true) }
    }

    private fun readHeader(input: InputStream, name: String): String? {
        // Тянем заголовки до пустой строки (после request line).
        val sb = StringBuilder()
        while (sb.length < 16384) {
            val b = input.read()
            if (b < 0 || b == '\n'.code) {
                val line = sb.toString().trim()
                if (line.isEmpty()) return null
                val idx = line.indexOf(':')
                if (idx > 0 && line.substring(0, idx).equals(name, true)) {
                    return line.substring(idx + 1).trim()
                }
                sb.setLength(0)
                continue
            }
            sb.append(b.toChar())
        }
        return null
    }

    private fun servePage(out: OutputStream) {
        val s = serving.get() ?: return respond(out, 503, "text/plain; charset=utf-8", "server stopped".toByteArray())
        val sizeMb = s.ipa.length() / 1_048_576.0
        val html = landingPage(
            title = escapeHtml(s.title),
            version = escapeHtml(s.version),
            size = String.format("%.1f МБ", sizeMb),
            qrDataUri = qrPngDataUri(s.url),
            installUrl = s.installUrl,
            pageUrl = s.url,
        )
        respond(out, 200, "text/html; charset=utf-8", html.toByteArray(Charsets.UTF_8))
    }

    private fun serveManifest(out: OutputStream) {
        val s = serving.get() ?: return respond(out, 503, "text/plain; charset=utf-8", "server stopped".toByteArray())
        val root = Plist.dict(
            "items" to listOf(
                mapOf(
                    "assets" to listOf(
                        mapOf(
                            "kind" to "software-package",
                            "url" to "${s.url}/app.ipa",
                        ),
                    ),
                    "metadata" to mapOf(
                        "bundle-identifier" to s.bundleId,
                        "bundle-version" to s.version,
                        "kind" to "software",
                        "title" to s.title,
                    ),
                ),
            ),
        )
        respond(out, 200, "application/xml", Plist.xml(root))
    }

    private fun serveQr(out: OutputStream) {
        val s = serving.get() ?: return respond(out, 503, "text/plain; charset=utf-8", "server stopped".toByteArray())
        respond(out, 200, "image/png", qrPngBytes(s.url))
    }

    private fun serveIpa(out: OutputStream, rangeHeader: String?) {
        val s = serving.get() ?: return respond(out, 503, "text/plain; charset=utf-8", "server stopped".toByteArray())
        val len = s.ipa.length()
        var from = 0L
        var to = len - 1
        var partial = false
        if (rangeHeader != null) {
            val m = Regex("bytes=(\\d*)-(\\d*)").find(rangeHeader)
            if (m != null) {
                partial = true
                from = m.groupValues[1].toLongOrNull() ?: 0L
                to = m.groupValues[2].toLongOrNull()?.coerceAtMost(len - 1) ?: (len - 1)
            }
        }
        if (from >= len || from > to) {
            respond(out, 416, "text/plain; charset=utf-8", "range not satisfiable".toByteArray(), extra = "Content-Range: bytes */$len\r\n")
            return
        }
        val bodyLen = to - from + 1
        val head = buildString {
            append("HTTP/1.1 ${if (partial) "206 Partial Content" else "200 OK"}\r\n")
            append("Content-Type: application/octet-stream\r\n")
            append("Content-Length: $bodyLen\r\n")
            append("Content-Range: bytes $from-$to/$len\r\n")
            append("Accept-Ranges: bytes\r\n")
            append("Cache-Control: no-store\r\n")
            append("Connection: close\r\n\r\n")
        }
        out.write(head.toByteArray(Charsets.ISO_8859_1))
        s.ipa.inputStream().use { fs ->
            var skip = from
            while (skip > 0) {
                val n = fs.skip(skip)
                if (n <= 0) break
                skip -= n
            }
            val buf = ByteArray(64 * 1024)
            var left = bodyLen
            while (left > 0) {
                val want = minOf(buf.size.toLong(), left).toInt()
                val n = fs.read(buf, 0, want)
                if (n <= 0) break
                out.write(buf, 0, n)
                left -= n
            }
        }
    }

    private fun respond(out: OutputStream, status: Int, contentType: String, body: ByteArray, extra: String = "") {
        val reason = when (status) {
            200 -> "OK"
            206 -> "Partial Content"
            404 -> "Not Found"
            416 -> "Range Not Satisfiable"
            503 -> "Service Unavailable"
            else -> "Status"
        }
        val head = buildString {
            append("HTTP/1.1 $status $reason\r\n")
            append("Content-Type: $contentType\r\n")
            append("Content-Length: ${body.size}\r\n")
            append("Cache-Control: no-store\r\n")
            if (extra.isNotEmpty()) append(extra)
            append("Connection: close\r\n\r\n")
        }
        out.write(head.toByteArray(Charsets.ISO_8859_1))
        out.write(body)
    }

    private fun qrPngBytes(text: String): ByteArray {
        val hints = mapOf(EncodeHintType.MARGIN to 1, EncodeHintType.CHARACTER_SET to "UTF-8")
        val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 512, 512, hints)
        val bmp = Bitmap.createBitmap(matrix.width, matrix.height, Bitmap.Config.ARGB_8888)
        for (y in 0 until matrix.height) {
            for (x in 0 until matrix.width) {
                bmp.setPixel(x, y, if (matrix[x, y]) Color.BLACK else Color.WHITE)
            }
        }
        val bos = ByteArrayOutputStream()
        bmp.compress(Bitmap.CompressFormat.PNG, 100, bos)
        return bos.toByteArray()
    }

    private fun qrPngDataUri(text: String): String {
        val png = qrPngBytes(text)
        return "data:image/png;base64," + android.util.Base64.encodeToString(png, android.util.Base64.NO_WRAP)
    }

    private fun escapeHtml(s: String): String =
        s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")

    private fun bestLanIp(): String? {
        val candidates = mutableListOf<String>()
        runCatching {
            NetworkInterface.getNetworkInterfaces().toList().forEach { ni ->
                if (!ni.isUp || ni.isLoopback) return@forEach
                ni.inetAddresses.toList().forEach { addr ->
                    if (addr is java.net.Inet4Address && addr.isSiteLocalAddress) {
                        candidates += addr.hostAddress ?: return@forEach
                    }
                }
            }
        }
        return candidates.firstOrNull()
    }

    private fun landingPage(
        title: String,
        version: String,
        size: String,
        qrDataUri: String,
        installUrl: String,
        pageUrl: String,
    ): String = """
<!DOCTYPE html>
<html lang="ru">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>Signum · Установка</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; -webkit-tap-highlight-color: transparent; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Display', 'Segoe UI', sans-serif;
    min-height: 100vh;
    background: #0a0d14;
    background-image:
      radial-gradient(60% 50% at 15% 0%, rgba(64,120,255,.22), transparent 70%),
      radial-gradient(50% 40% at 90% 100%, rgba(120,64,255,.18), transparent 70%);
    color: #eef1f8;
    display: flex; align-items: center; justify-content: center;
    padding: 28px 18px;
  }
  .card {
    width: 100%; max-width: 420px;
    background: rgba(20, 25, 38, .82);
    border: 1px solid rgba(255,255,255,.09);
    border-radius: 26px;
    padding: 30px 26px 26px;
    box-shadow: 0 24px 80px rgba(0,0,0,.55);
    backdrop-filter: blur(14px);
  }
  .brand { display: flex; align-items: center; gap: 8px; justify-content: center; margin-bottom: 22px; }
  .brand .dot {
    width: 10px; height: 10px; border-radius: 50%;
    background: linear-gradient(135deg, #4f8cff, #9a6bff);
    box-shadow: 0 0 12px rgba(79,140,255,.8);
  }
  .brand span { font-size: 13px; letter-spacing: .3em; text-transform: uppercase; color: #8b93a7; }
  .app-icon {
    width: 84px; height: 84px; margin: 0 auto 16px;
    border-radius: 22px;
    background: linear-gradient(135deg, #4f8cff 0%, #9a6bff 100%);
    display: flex; align-items: center; justify-content: center;
    font-size: 34px; font-weight: 700; color: #fff;
    box-shadow: 0 10px 34px rgba(79,140,255,.45);
  }
  h1 { font-size: 22px; text-align: center; font-weight: 700; }
  .meta { text-align: center; color: #8b93a7; font-size: 14px; margin-top: 6px; }
  .meta b { color: #c6cdde; font-weight: 500; }
  .install {
    display: block; width: 100%; margin: 24px 0 10px;
    padding: 17px 0; border: 0; border-radius: 16px;
    font-size: 17px; font-weight: 600; text-decoration: none; text-align: center;
    color: #fff;
    background: linear-gradient(135deg, #3d7bff, #8b5bff);
    box-shadow: 0 12px 34px rgba(61,123,255,.38);
    transition: transform .12s ease, box-shadow .12s ease;
  }
  .install:active { transform: scale(.975); box-shadow: 0 6px 18px rgba(61,123,255,.30); }
  .qr-wrap { text-align: center; margin: 22px 0 8px; }
  .qr-wrap img {
    width: 190px; height: 190px;
    border-radius: 18px;
    background: #fff; padding: 9px;
    box-shadow: 0 8px 28px rgba(0,0,0,.4);
  }
  .qr-hint { text-align: center; font-size: 12.5px; color: #737b90; line-height: 1.55; }
  .link {
    margin-top: 16px; padding: 12px 14px;
    background: rgba(255,255,255,.05); border: 1px solid rgba(255,255,255,.07);
    border-radius: 12px; font-size: 13px; color: #aab3c8;
    text-align: center; word-break: break-all;
  }
  .foot { margin-top: 18px; text-align: center; font-size: 11.5px; color: #566079; line-height: 1.6; }
</style>
</head>
<body>
  <div class="card">
    <div class="brand"><div class="dot"></div><span>Signum</span></div>
    <div class="app-icon">${title.take(1).uppercase()}</div>
    <h1>$title</h1>
    <div class="meta">версия <b>$version</b> · <b>$size</b></div>
    <a class="install" href="$installUrl">Установить на iPhone</a>
    <div class="qr-wrap"><img src="$qrDataUri" alt="QR"></div>
    <div class="qr-hint">Наведите камеру iPhone на код,<br>или откройте ссылку в Safari на этом телефоне</div>
    <div class="link">$pageUrl</div>
    <div class="foot">Телефон и устройство с Signum должны быть<br>в одной Wi-Fi сети. Не закрывайте Signum до конца установки.</div>
  </div>
</body>
</html>
"""
}
