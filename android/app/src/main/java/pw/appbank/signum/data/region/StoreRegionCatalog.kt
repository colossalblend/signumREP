package pw.appbank.signum.data.region

import org.json.JSONArray
import org.json.JSONObject
import pw.appbank.signum.data.settings.SettingsStore

/** Страна магазина, которую человек может выбрать в списке. */
data class StoreRegion(
    val code: String,
    val name: String,
)

/**
 * Платёжный адрес для выбранной страны. Apple меняет страну магазина только
 * вместе с платёжным адресом — отдельной кнопки «сменить страну» у неё нет,
 * поэтому адрес нужен всегда.
 */
data class RegionAddress(
    val countryCode: String,
    val countryName: String,
    val street: String,
    val city: String,
    val postal: String,
    val state: String? = null,
    val county: String? = null,
    val phoneArea: String,
    val phoneNumber: String,
)

/** Итог операции: где аккаунт находится сейчас и что сказать человеку. */
data class RegionStatus(
    val code: String,
    val name: String,
    val message: String,
)

/**
 * Таблица стран и адресов лежит в assets/regions.json, а не в коде: это чистые
 * данные, их правят без пересборки логики, и .kt не распухает на полсотни витрин.
 */
internal class StoreRegionCatalog(private val store: SettingsStore) {

    private class Entry(
        val code: String,
        val name: String,
        val cardOnly: Boolean,
        val addresses: List<RegionAddress>,
    )

    private val entries: List<Entry> by lazy { parseAsset() }

    /** Запасные названия для стран, которых нет в таблице адресов (их не выбрать, но показать надо). */
    private val extraNames: Map<String, String> by lazy { parseNames() }

    private var raw: JSONObject? = null

    private fun document(): JSONObject {
        raw?.let { return it }
        val text = store.openAsset("regions.json").use { it.readBytes().toString(Charsets.UTF_8) }
        val parsed = JSONObject(text)
        raw = parsed
        return parsed
    }

    private fun parseAsset(): List<Entry> = runCatching {
        val list = document().optJSONArray("regions") ?: JSONArray()
        val out = ArrayList<Entry>(list.length())
        for (i in 0 until list.length()) {
            val o = list.optJSONObject(i) ?: continue
            val code = o.optString("code").trim().uppercase()
            val name = o.optString("name").trim()
            if (code.length != 3 || name.isEmpty()) continue
            val addrArray = o.optJSONArray("addresses") ?: JSONArray()
            val addresses = ArrayList<RegionAddress>(addrArray.length())
            for (j in 0 until addrArray.length()) {
                val a = addrArray.optJSONObject(j) ?: continue
                addresses += RegionAddress(
                    countryCode = code,
                    countryName = name,
                    street = a.optString("street"),
                    city = a.optString("city"),
                    postal = a.optString("postal"),
                    state = a.optString("state").trim().takeIf { it.isNotEmpty() },
                    county = a.optString("county").trim().takeIf { it.isNotEmpty() },
                    phoneArea = a.optString("phoneArea"),
                    phoneNumber = a.optString("phoneNumber"),
                )
            }
            if (addresses.isEmpty()) continue
            out += Entry(code, name, o.optBoolean("cardOnly", false), addresses)
        }
        out
    }.getOrElse { emptyList<Entry>() }

    private fun parseNames(): Map<String, String> = runCatching {
        val o = document().optJSONObject("names") ?: return@runCatching emptyMap<String, String>()
        o.keys().asSequence().associate { it.uppercase() to o.optString(it) }
    }.getOrElse { emptyMap<String, String>() }

    private fun entry(iso3: String): Entry? {
        val code = iso3.trim().uppercase()
        return entries.firstOrNull { it.code == code }
    }

    /** Только страны, где Apple разрешает счёт без банковской карты. */
    fun regions(): List<StoreRegion> =
        entries.filterNot { it.cardOnly }.map { StoreRegion(it.code, it.name) }

    fun name(iso3: String): String {
        val code = iso3.trim().uppercase()
        return entry(code)?.name ?: extraNames[code] ?: code
    }

    /** Проверенные адреса страны. Если Apple не принял один — можно предложить соседний. */
    fun addresses(iso3: String): List<RegionAddress> {
        if (entries.isEmpty()) error("Не удалось загрузить список стран. Переустановите Signum.")
        val found = entry(iso3)
            ?: error("Эту страну пока нельзя выбрать в Signum.")
        if (found.cardOnly) {
            error("Для этой страны Apple просит банковскую карту. Выберите другую.")
        }
        return found.addresses
    }

    /**
     * Адрес выбирается случайно, чтобы у разных людей не оказался один и тот же:
     * Apple подозрительно относится к пачке аккаунтов с одинаковым адресом.
     */
    fun pick(iso3: String): RegionAddress = addresses(iso3).random()
}
