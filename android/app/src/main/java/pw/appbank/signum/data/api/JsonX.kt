package pw.appbank.signum.data.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

val SignumJson = Json {
    ignoreUnknownKeys = true
    isLenient = true
    coerceInputValues = true
    explicitNulls = false
}

fun JsonElement.obj(): JsonObject? = this as? JsonObject
fun JsonElement.arr(): JsonArray? = this as? JsonArray
fun JsonObject.str(key: String): String? = anyStr(key)

fun JsonObject.anyStr(vararg keys: String): String? {
    for (key in keys) {
        val p = this[key] as? JsonPrimitive ?: continue
        val raw = p.content.trim()
        if (raw.isNotEmpty() && !raw.equals("null", true) && raw != "NONE") return raw
    }
    return null
}

fun JsonObject.bool(key: String, default: Boolean = false): Boolean = anyBool(key, default)

fun JsonObject.anyBool(key: String, default: Boolean = false): Boolean {
    val p = this[key] as? JsonPrimitive ?: return default
    p.booleanOrNull?.let { return it }
    p.intOrNull?.let { return it != 0 }
    return when (p.content.trim().lowercase()) {
        "1", "true", "yes", "active", "available" -> true
        "0", "false", "no" -> false
        else -> default
    }
}

fun JsonObject.int(key: String): Int? {
    val p = this[key] as? JsonPrimitive ?: return null
    return p.intOrNull ?: p.content.trim().toIntOrNull()
}

fun JsonObject.strList(key: String): List<String> {
    val arr = arr(key) ?: return emptyList()
    return arr.mapNotNull { el ->
        val p = el as? JsonPrimitive ?: return@mapNotNull null
        p.content.trim().takeIf { it.isNotEmpty() }
    }
}

fun JsonObject.arr(key: String): JsonArray? = this[key] as? JsonArray

fun JsonObject.obj(key: String): JsonObject? = this[key] as? JsonObject

fun JsonPrimitive.asText(): String = contentOrNull.orEmpty()
