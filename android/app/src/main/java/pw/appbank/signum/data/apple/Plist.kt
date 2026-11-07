package pw.appbank.signum.data.apple

import com.dd.plist.BinaryPropertyListParser
import com.dd.plist.NSArray
import com.dd.plist.NSData
import com.dd.plist.NSDictionary
import com.dd.plist.NSNumber
import com.dd.plist.NSObject
import com.dd.plist.NSString
import com.dd.plist.PropertyListParser
import com.dd.plist.XMLPropertyListParser
import java.io.ByteArrayInputStream

object Plist {
    fun parse(bytes: ByteArray): NSDictionary {
        val obj = parseObject(stripPreamble(bytes))
        return when (obj) {
            is NSDictionary -> obj
            else -> error("Ожидался plist-словарь")
        }
    }

    fun parseObject(bytes: ByteArray): NSObject {
        if (bytes.size >= 8 && bytes.copyOfRange(0, 6).toString(Charsets.US_ASCII) == "bplist") {
            return BinaryPropertyListParser.parse(bytes)
        }
        val raw = runCatching { bytes.toString(Charsets.UTF_8) }.getOrDefault("")
        val xml = normalizeXmlPlist(raw)
        if (xml != null) {
            return try {
                XMLPropertyListParser.parse(ByteArrayInputStream(xml.toByteArray()))
            } catch (e: Exception) {
                pw.appbank.signum.data.api.DebugDump.logFail("SignumPlist", bytes, e)
                error("Apple вернул неожиданный ответ. Повторите вход.")
            }
        }
        return try {
            PropertyListParser.parse(ByteArrayInputStream(bytes))
        } catch (e: Exception) {
            pw.appbank.signum.data.api.DebugDump.logFail("SignumPlist", bytes, e)
            if (raw.contains("<html", ignoreCase = true) || raw.contains("DOCTYPE html", ignoreCase = true)) {
                error("Apple вернул HTML вместо данных. Проверьте интернет и повторите вход.")
            }
            error("Apple вернул неожиданный ответ. Повторите вход.")
        }
    }

                                                                                  
    private fun normalizeXmlPlist(text: String): String? {
        var t = text.trim()
        if (t.isEmpty()) return null
        t = t.replace(Regex("<!DOCTYPE[^>]*>", RegexOption.IGNORE_CASE), "")
        val plistIdx = t.indexOf("<plist", ignoreCase = true)
        if (plistIdx >= 0) {
            return "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n" + t.substring(plistIdx)
        }
        val dictIdx = t.indexOf("<dict", ignoreCase = true)
        if (dictIdx >= 0) {
            val dictXml = t.substring(dictIdx)
            return """<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
$dictXml
</plist>
"""
        }
        return null
    }

    private fun stripPreamble(bytes: ByteArray): ByteArray {
        var start = 0
        if (bytes.size >= 3 &&
            bytes[0] == 0xEF.toByte() &&
            bytes[1] == 0xBB.toByte() &&
            bytes[2] == 0xBF.toByte()
        ) {
            start = 3
        }
        while (start < bytes.size) {
            val b = bytes[start].toInt() and 0xFF
            if (b == ' '.code || b == '\n'.code || b == '\r'.code || b == '\t'.code) start++ else break
        }
        return if (start == 0) bytes else bytes.copyOfRange(start, bytes.size)
    }

    fun parseText(text: String): NSDictionary = parse(text.toByteArray())

    fun xml(dict: NSDictionary): ByteArray = dict.toXMLPropertyList().toByteArray()

    fun dict(vararg pairs: Pair<String, Any?>): NSDictionary {
        val d = NSDictionary()
        for ((k, v) in pairs) {
            if (v != null) d.put(k, wrap(v))
        }
        return d
    }

    fun wrap(value: Any): NSObject = when (value) {
        is NSObject -> value
        is String -> NSString(value)
        is Boolean -> NSNumber(value)
        is Int -> NSNumber(value)
        is Long -> NSNumber(value)
        is Double -> NSNumber(value)
        is ByteArray -> NSData(value)
        is Map<*, *> -> {
            val d = NSDictionary()
            value.forEach { (k, v) ->
                if (k != null && v != null) d.put(k.toString(), wrap(v))
            }
            d
        }
        is List<*> -> NSArray(*value.mapNotNull { it?.let(::wrap) }.toTypedArray())
        is Array<*> -> NSArray(*value.mapNotNull { it?.let(::wrap) }.toTypedArray())
        else -> NSString(value.toString())
    }
}

fun NSDictionary.str(key: String): String? {
    val v = objectForKey(key) ?: return null
    return when (v) {
        is NSString -> v.content.trim().takeIf { it.isNotEmpty() }
        is NSData -> null
        else -> v.toString().trim().takeIf { it.isNotEmpty() }
    }
}

fun NSDictionary.data(key: String): ByteArray? {
    val v = objectForKey(key) ?: return null
    return when (v) {
        is NSData -> v.bytes()
        is NSString -> android.util.Base64.decode(v.content, android.util.Base64.DEFAULT)
        else -> null
    }
}

fun NSDictionary.anyBytes(key: String): Any? {
    val d = data(key)
    if (d != null) return d
    return str(key)
}

fun NSDictionary.int(key: String): Int? {
    val v = objectForKey(key) ?: return null
    return when (v) {
        is NSNumber -> v.intValue()
        is NSString -> v.content.toIntOrNull()
        else -> null
    }
}

fun NSDictionary.dict(key: String): NSDictionary? = objectForKey(key) as? NSDictionary

fun NSDictionary.arr(key: String): NSArray? = objectForKey(key) as? NSArray
