package pw.appbank.signum.data.apple

import okhttp3.Request
import pw.appbank.signum.data.api.Http
import java.io.ByteArrayOutputStream
import java.util.zip.Inflater
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/**
 * Подходящая под телефон сборка: что ставить и почему она подошла.
 * Отдельный тип, потому что общий список версий минимальную систему не хранит,
 * а трогать общие модели тут нельзя.
 */
data class AppStoreVersionFit(
    val externalVersionId: String,
    val version: String,
    val minIos: String,
    val isLatest: Boolean,
)

/**
 * Минимальная система для одной сборки приложения из App Store.
 *
 * Магазин её не сообщает: в ответе про сборку есть номер версии, список
 * устройств и ограничения, но не система. Единственный источник — ключ
 * MinimumOSVersion в Info.plist внутри самой сборки. Скачивать её целиком
 * нельзя, это сотни мегабайт, поэтому забираем только нужные куски: размер,
 * хвост с оглавлением, само оглавление и одну запись. Около мегабайта на
 * сборку вместо всего файла.
 */
object IpaMinIos {
    /** Самый длинный возможный хвост архива вместе с комментарием. */
    private const val TAIL = 65_557L

    /** Оглавление больше этого читать не станем, чтобы не съесть всю память. */
    private const val MAX_CHUNK = 32L * 1024 * 1024

    private const val UNREADABLE = "Не удалось узнать, какая система нужна этой версии"

    fun read(url: String, userAgent: String): String {
        val probe = Request.Builder()
            .url(url)
            .header("User-Agent", userAgent)
            .header("Accept", "*/*")
            .header("Range", "bytes=0-0")
            .get()
            .build()
        val size = Http.cdn.newCall(probe).execute().use { resp ->
            if (resp.code != 206) error(UNREADABLE)
            resp.body?.bytes()
            // Полный размер файла сервер сообщает только в этом заголовке.
            resp.header("Content-Range")?.substringAfterLast('/')?.trim()?.toLongOrNull()
        } ?: error(UNREADABLE)
        if (size < 100) error(UNREADABLE)
        return readFrom(size) { from, to -> fetchRange(url, userAgent, from, to) }
    }

    /**
     * Разбор архива в отрыве от сети: `fetch` отдаёт байты с `from` по `to`
     * включительно. На этом же входе работает самопроверка ниже.
     */
    internal fun readFrom(size: Long, fetch: (Long, Long) -> ByteArray): String {
        val tail = fetch(maxOf(0L, size - TAIL), size - 1)
        val eocd = findEocd(tail) ?: error(UNREADABLE)
        val dirSize = le32(tail, eocd + 12) ?: error(UNREADABLE)
        val dirAt = le32(tail, eocd + 16) ?: error(UNREADABLE)
        // 0xFFFFFFFF означает архив нового формата на терабайты; сборки такими
        // не бывают, и разбирать его ради одного ключа незачем.
        if (dirSize <= 0L || dirSize > MAX_CHUNK || dirAt == 0xFFFFFFFFL || dirAt + dirSize > size) {
            error(UNREADABLE)
        }
        val dir = fetch(dirAt, dirAt + dirSize - 1)

        var at = 0
        while (at + 46 <= dir.size && matches(dir, at, 0x50, 0x4b, 0x01, 0x02)) {
            val nameLen = le16(dir, at + 28) ?: error(UNREADABLE)
            val extraLen = le16(dir, at + 30) ?: error(UNREADABLE)
            val commentLen = le16(dir, at + 32) ?: error(UNREADABLE)
            val nameEnd = at + 46 + nameLen
            if (nameEnd > dir.size) error(UNREADABLE)
            val name = String(dir, at + 46, nameLen, Charsets.UTF_8)
            if (isMainInfoPlist(name)) {
                val method = le16(dir, at + 10) ?: error(UNREADABLE)
                val packed = le32(dir, at + 20) ?: error(UNREADABLE)
                val local = le32(dir, at + 42) ?: error(UNREADABLE)
                if (packed <= 0L || packed > MAX_CHUNK || local >= size) error(UNREADABLE)
                return readEntry(size, fetch, local, packed, method)
            }
            at = nameEnd + extraLen + commentLen
        }
        error(UNREADABLE)
    }

    private fun readEntry(
        size: Long,
        fetch: (Long, Long) -> ByteArray,
        localAt: Long,
        packed: Long,
        method: Int,
    ): String {
        // В начале записи имя и служебные поля повторяются со своими длинами,
        // поэтому берём с запасом и уже потом отсчитываем настоящее начало.
        val end = minOf(localAt + 30 + 4096 + packed, size) - 1
        val raw = fetch(localAt, end)
        if (raw.size < 30 || !matches(raw, 0, 0x50, 0x4b, 0x03, 0x04)) error(UNREADABLE)
        val start = 30 + (le16(raw, 26) ?: error(UNREADABLE)) + (le16(raw, 28) ?: error(UNREADABLE))
        val stop = start + packed.toInt()
        if (start < 0 || stop > raw.size) error(UNREADABLE)
        val data = raw.copyOfRange(start, stop)
        val plist = when (method) {
            0 -> data
            8 -> inflateRaw(data)
            else -> error(UNREADABLE)
        }
        // Info.plist в сборках бывает и двоичный, и текстовый — общий разбор
        // умеет оба, но его сообщения про повторный вход тут были бы не к месту.
        val dict = runCatching { Plist.parse(plist) }.getOrElse { error(UNREADABLE) }
        return dict.str("MinimumOSVersion") ?: error(UNREADABLE)
    }

    /**
     * Только Payload/<Имя>.app/Info.plist, ровно два слэша: у встроенных
     * библиотек и языковых папок свои Info.plist, и по ним ответ был бы чужой.
     */
    internal fun isMainInfoPlist(name: String): Boolean {
        val clean = name.replace('\\', '/')
        return clean.startsWith("Payload/") &&
            clean.endsWith(".app/Info.plist") &&
            clean.count { it == '/' } == 2
    }

    private fun fetchRange(url: String, userAgent: String, from: Long, to: Long): ByteArray {
        val req = Request.Builder()
            .url(url)
            .header("User-Agent", userAgent)
            .header("Accept", "*/*")
            .header("Range", "bytes=$from-$to")
            .get()
            .build()
        return Http.cdn.newCall(req).execute().use { resp ->
            if (resp.code != 206) error(UNREADABLE)
            resp.body?.bytes() ?: ByteArray(0)
        }
    }

    private fun findEocd(tail: ByteArray): Int? {
        for (at in tail.size - 22 downTo 0) {
            if (matches(tail, at, 0x50, 0x4b, 0x05, 0x06)) return at
        }
        return null
    }

    private fun matches(buf: ByteArray, at: Int, vararg sig: Int): Boolean {
        if (at < 0 || at + sig.size > buf.size) return false
        return sig.indices.all { (buf[at + it].toInt() and 0xFF) == sig[it] }
    }

    private fun le16(buf: ByteArray, at: Int): Int? {
        if (at < 0 || at + 2 > buf.size) return null
        return (buf[at].toInt() and 0xFF) or ((buf[at + 1].toInt() and 0xFF) shl 8)
    }

    private fun le32(buf: ByteArray, at: Int): Long? {
        if (at < 0 || at + 4 > buf.size) return null
        return (buf[at].toLong() and 0xFF) or
            ((buf[at + 1].toLong() and 0xFF) shl 8) or
            ((buf[at + 2].toLong() and 0xFF) shl 16) or
            ((buf[at + 3].toLong() and 0xFF) shl 24)
    }

    private fun inflateRaw(data: ByteArray): ByteArray {
        val inflater = Inflater(true)
        inflater.setInput(data)
        val out = ByteArrayOutputStream(maxOf(data.size * 4, 8192))
        val buf = ByteArray(16 * 1024)
        try {
            while (!inflater.finished()) {
                val n = inflater.inflate(buf)
                if (n == 0) {
                    if (inflater.needsInput() || inflater.needsDictionary()) break
                } else {
                    out.write(buf, 0, n)
                }
            }
        } catch (e: Exception) {
            error(UNREADABLE)
        } finally {
            inflater.end()
        }
        if (out.size() == 0) error(UNREADABLE)
        return out.toByteArray()
    }

    /**
     * Самопроверка разбора без сети: собираем в памяти маленький архив с таким
     * же расположением файлов, как в сборке, и читаем его теми же кусками.
     * Падает ровно тогда, когда ломается разбор оглавления или выбор файла.
     */
    fun selfCheck(): Boolean {
        check(isMainInfoPlist("Payload/VKClient.app/Info.plist"))
        check(!isMainInfoPlist("Payload/VKClient.app/Frameworks/Shared.framework/Info.plist"))
        check(!isMainInfoPlist("Payload/VKClient.app/Base.lproj/Info.plist"))
        check(!isMainInfoPlist("Info.plist"))

        val plist = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n" +
            "<plist version=\"1.0\"><dict>" +
            "<key>MinimumOSVersion</key><string>12.4</string>" +
            "</dict></plist>\n"
        val zip = ByteArrayOutputStream()
        ZipOutputStream(zip).use { out ->
            // Чужой Info.plist идёт первым: если выбор файла сломается, ответ
            // будет из него, и проверка это заметит.
            out.putNextEntry(ZipEntry("Payload/Demo.app/Frameworks/X.framework/Info.plist"))
            out.write(("<plist version=\"1.0\"><dict>" +
                "<key>MinimumOSVersion</key><string>9.0</string></dict></plist>").toByteArray())
            out.closeEntry()
            out.putNextEntry(ZipEntry("Payload/Demo.app/Info.plist"))
            out.write(plist.toByteArray())
            out.closeEntry()
        }
        val bytes = zip.toByteArray()
        val value = readFrom(bytes.size.toLong()) { from, to ->
            bytes.copyOfRange(from.toInt(), minOf(to.toInt() + 1, bytes.size))
        }
        check(value == "12.4") { "самопроверка чтения системы дала $value" }
        return true
    }
}
