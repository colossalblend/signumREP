package pw.appbank.signum.data.device

import pw.appbank.signum.data.apple.str
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

class AfcClient(private val stream: UsbMux.MuxStream) {
    private var packet = 0L

    fun makeDir(path: String) {
        send(AFC_OP_MAKE_DIR, cString(path))
        val (op, data) = recv()
        if (op == AFC_OP_STATUS) {
            val status = statusCode(data)
            if (status != 0L && status != AFC_E_OBJECT_EXISTS) {
                error("Не удалось подготовить папку на iPhone")
            }
        }
    }

    fun upload(local: File, remote: String, onProgress: (Float, String) -> Unit = { _, _ -> }) {
        val handle = fileOpen(remote, AFC_FOPEN_WR)
        val total = local.length().coerceAtLeast(1)
        pw.appbank.signum.data.api.DebugDump.pair(
            "afc open $remote handle=$handle size=$total chunk=$AFC_WRITE_CHUNK (saved=$AFC_WRITE_CHUNK_SAVED)",
        )
        fun report(bytes: Long) {
            val pct = (bytes.toFloat() / total) * 100f
            val have = bytes / (1024 * 1024)
            val all = total / (1024 * 1024)
            onProgress(pct, "Копируем файл… ${pct.toInt()}% · $have / $all МБ")
        }
        report(0)
        var sent = 0L
        var lastLog = 0L
        local.inputStream().use { input ->
            val buf = ByteArray(AFC_WRITE_CHUNK)
            while (true) {
                val n = input.read(buf)
                if (n <= 0) break
                report(sent)
                fileWrite(handle, if (n == buf.size) buf else buf.copyOf(n))
                sent += n
                report(sent)
                if (sent - lastLog >= 1L * 1024L * 1024L || sent == total) {
                    lastLog = sent
                    pw.appbank.signum.data.api.DebugDump.pair("afc wrote $sent/$total")
                }
            }
        }
        fileClose(handle)
        pw.appbank.signum.data.api.DebugDump.pair("afc close $remote ok")
    }

    private fun fileOpen(path: String, mode: Long): Long {
        val pathBytes = cString(path)
        val payload = ByteBuffer.allocate(8 + pathBytes.size).order(ByteOrder.LITTLE_ENDIAN)
        payload.putLong(mode)
        payload.put(pathBytes)
        send(AFC_OP_FILE_OPEN, payload.array())
        val (op, data) = recv()
        if (op != AFC_OP_FILE_OPEN_RESULT || data.size < 8) {
            error("Не удалось открыть файл на iPhone")
        }
        return ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN).long
    }

    private fun fileWrite(handle: Long, chunk: ByteArray) {
        val handleBuf = ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(handle).array()
        send(AFC_OP_WRITE, handleBuf, chunk)
        val (op, data) = recv()
        if (op == AFC_OP_STATUS) {
            val status = statusCode(data)
            if (status != 0L) error("Не удалось записать файл на iPhone")
        }
    }

    private fun fileClose(handle: Long) {
        val payload = ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(handle).array()
        send(AFC_OP_FILE_CLOSE, payload)
        recv()
    }

    private fun send(op: Long, headerPayload: ByteArray, body: ByteArray = ByteArray(0)) {
        packet += 1
        val thisLen = 40L + headerPayload.size
        val entire = thisLen + body.size
        val header = ByteBuffer.allocate(40).order(ByteOrder.LITTLE_ENDIAN)
        header.putLong(AFC_MAGIC)
        header.putLong(entire)
        header.putLong(thisLen)
        header.putLong(packet)
        header.putLong(op)
        stream.write(header.array() + headerPayload + body)
    }

    private fun recv(): Pair<Long, ByteArray> {
        val hdr = stream.readExact(40, 30_000)
        val buf = ByteBuffer.wrap(hdr).order(ByteOrder.LITTLE_ENDIAN)
        val magic = buf.long
        val entireLen = buf.long
        val thisLen = buf.long
        buf.long
        val op = buf.long
        if (magic != AFC_MAGIC) {
            pw.appbank.signum.data.api.DebugDump.pair(
                "afc bad magic=${java.lang.Long.toHexString(magic)} entire=$entireLen this=$thisLen op=$op",
            )
        }
        val headerRest = (thisLen - 40).toInt().coerceAtLeast(0)
        val bodyLen = (entireLen - thisLen).toInt().coerceAtLeast(0)
        val extra = if (headerRest > 0) stream.readExact(headerRest, 30_000) else ByteArray(0)
        val body = if (bodyLen > 0) stream.readExact(bodyLen, 30_000) else ByteArray(0)
        return op to (extra + body)
    }

    private fun statusCode(data: ByteArray): Long =
        if (data.size >= 8) ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN).long else -1L

    private fun cString(path: String): ByteArray = path.toByteArray() + 0

    companion object {
        const val AFC_MAGIC = 0x4141504C36414643L
        const val AFC_OP_STATUS = 1L
        const val AFC_OP_DATA = 2L
        const val AFC_OP_MAKE_DIR = 9L
        const val AFC_OP_FILE_OPEN = 13L
        const val AFC_OP_FILE_OPEN_RESULT = 14L
        const val AFC_OP_WRITE = 16L
        const val AFC_OP_FILE_CLOSE = 20L
        const val AFC_FOPEN_WR = 4L
        const val AFC_E_OBJECT_EXISTS = 16L
        const val AFC_WRITE_CHUNK_SAVED = 8 * 1024
        const val AFC_WRITE_CHUNK = 256 * 1024
    }
}

class IpaInstaller(
    private val devices: DeviceService,
) {
    fun install(ipa: File, onProgress: (Float, String) -> Unit) {
        if (!ipa.exists() || ipa.length() < 1000) error("Файл приложения не найден")
        val remote = "/PublicStaging/${ipa.name}"
        var copied = false
        devices.withDevice({ msg -> onProgress(72f, msg) }) { lock, mux ->
            val info = lock.readInfo()
            onProgress(72f, "Сессия с ${info.deviceName ?: "iPhone"}…")
            lock.startSession(info.udid, mux)
            val afcPort = lock.startService("com.apple.afc")
            pw.appbank.signum.data.api.DebugDump.pair("afc port=$afcPort ipa=${ipa.length()} copied=$copied")
            val afc = AfcClient(mux.connect(afcPort))
            runCatching { afc.makeDir("/PublicStaging") }
                .onFailure { pw.appbank.signum.data.api.DebugDump.pair("afc mkdir ${it.message}") }
            if (copied) {
                onProgress(90f, "Файл уже на iPhone, ставим…")
                pw.appbank.signum.data.api.DebugDump.pair("afc skip re-upload $remote")
            } else {
                afc.upload(ipa, remote) { p, msg ->
                    onProgress(20f + p * 0.70f, msg)
                }
                copied = true
            }
            onProgress(92f, "Устанавливаем…")
            val instPort = lock.startService("com.apple.mobile.installation_proxy")
            pw.appbank.signum.data.api.DebugDump.pair("inst port=$instPort")
            val inst = ServicePlist(mux.connect(instPort))
            inst.send(
                mapOf(
                    "Command" to "Install",
                    "PackagePath" to remote,
                    "ClientOptions" to emptyMap<String, Any>(),
                ),
            )
            // iPhone сам говорит, как идёт установка. Раньше молчание устройства
            // просто продолжало цикл: 80 попыток по 30 секунд — до сорока минут
            // с неизменной надписью «Устанавливаем…», и человек не понимал,
            // идёт что-то или всё зависло.
            val started = System.currentTimeMillis()
            var silence = 0
            var lastStatus = "Устанавливаем…"
            while (System.currentTimeMillis() - started < INSTALL_TIMEOUT_MS) {
                val dict = runCatching { inst.recv(RECV_TIMEOUT_MS) }.onFailure {
                    pw.appbank.signum.data.api.DebugDump.pair("inst recv ${it.message}")
                }.getOrNull()
                if (dict == null) {
                    silence += 1
                    if (silence >= MAX_SILENT_ROUNDS) {
                        error("iPhone перестал отвечать во время установки. Отключите и подключите кабель, разблокируйте телефон и повторите.")
                    }
                    // Показываем, что ждём именно ответа телефона, и сколько уже.
                    val waited = (System.currentTimeMillis() - started) / 1000
                    onProgress(92f, "$lastStatus ждём ответа iPhone, $waited с")
                    continue
                }
                silence = 0
                val status = dict.str("Status")
                val err = dict.str("Error")
                val pct = dict.objectForKey("PercentComplete")?.toString()?.toFloatOrNull()
                if (err != null) error("iPhone отклонил установку: $err")
                if (status != null) lastStatus = translateInstallStatus(status)
                if (pct != null) onProgress(92f + pct * 0.08f, "$lastStatus ${pct.toInt()}%")
                else onProgress(92f, lastStatus)
                if (status == "Complete") {
                    onProgress(100f, "Установлено")
                    return@withDevice
                }
            }
            error("Установка на iPhone не завершилась за ${INSTALL_TIMEOUT_MS / 60000} минут. Разблокируйте телефон и повторите.")
        }
    }

    private val RECV_TIMEOUT_MS = 10_000
    private val MAX_SILENT_ROUNDS = 6
    private val INSTALL_TIMEOUT_MS = 6 * 60_000L

    /** Статусы installation_proxy приходят по-английски; показываем по-русски. */
    private fun translateInstallStatus(status: String): String = when (status) {
        "CreatingStagingDirectory" -> "Готовим место на iPhone…"
        "ExtractingPackage" -> "Распаковываем на iPhone…"
        "InspectingPackage" -> "Проверяем пакет…"
        "PreflightingApplication" -> "Проверяем приложение…"
        "VerifyingApplication" -> "Проверяем подпись…"
        "CreatingContainer" -> "Создаём контейнер…"
        "InstallingEmbeddedProfile" -> "Ставим профиль…"
        "InstallingApplication" -> "Устанавливаем…"
        "PostflightingApplication" -> "Завершаем…"
        "SandboxingApplication" -> "Настраиваем доступы…"
        "GeneratingApplicationMap" -> "Почти готово…"
        "Complete" -> "Установлено"
        else -> "Устанавливаем…"
    }
}