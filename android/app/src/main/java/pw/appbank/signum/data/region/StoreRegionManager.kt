package pw.appbank.signum.data.region

import org.json.JSONObject
import pw.appbank.signum.data.settings.SettingsStore

/**
 * Смена страны магазина App Store.
 *
 * Главная опасность здесь не сеть, а Android: систему ничто не обязывает
 * дать процессу дожить до конца операции. Если приложение убьют между
 * «сменили на чужую страну» и «вернули свою», человек останется в чужой стране
 * и сам об этом не узнает. Поэтому исходный адрес записывается на диск ДО
 * переключения, а [recoverInterruptedRegionChange] чинит это при следующем запуске.
 *
 * Все методы ходят в сеть и блокируют поток — вызывать с Dispatchers.IO.
 */
class StoreRegionManager(private val store: SettingsStore) {

    private val catalog = StoreRegionCatalog(store)

    // ------------------------------------------------------------------ справочник

    /** Страны, куда можно перейти без банковской карты. */
    fun regions(): List<StoreRegion> = catalog.regions()

    /** Проверенные адреса страны — на случай, если Apple не примет первый. */
    fun addresses(iso3: String): List<RegionAddress> = catalog.addresses(iso3)

    fun countryName(iso3: String): String = catalog.name(iso3)

    // -------------------------------------------------------------------- операции

    /** Где аккаунт находится сейчас. */
    fun currentRegion(email: String): RegionStatus {
        val code = session(email).currentCountry()
        return RegionStatus(code, catalog.name(code), "Сейчас выбрана страна: ${catalog.name(code)}.")
    }

    /**
     * Меняет страну насовсем — так, как человек попросил на экране аккаунта.
     * Возвращать назад никто не будет, поэтому и запоминать нечего.
     * Адрес можно передать свой — например, если предыдущий Apple не принял.
     */
    fun switchTo(email: String, iso3: String, address: RegionAddress? = null): RegionStatus {
        val target = iso3.trim().uppercase()
        val wanted = address?.takeIf { it.countryCode.equals(target, true) } ?: catalog.pick(target)
        val code = session(email).changeTo(wanted)
        // Прошлый недоигранный возврат больше не нужен: человек сам выбрал, где остаться.
        store.deleteRegionSnapshot(email)
        return RegionStatus(code, catalog.name(code), "Страна магазина изменена на ${catalog.name(code)}.")
    }

    /**
     * Меняет страну на время — чтобы забрать приложение из чужого магазина и уйти обратно.
     * Обязательно завершать [restoreOriginal].
     */
    fun switchTemporarily(email: String, iso3: String, address: RegionAddress? = null): RegionStatus {
        val target = iso3.trim().uppercase()
        val wanted = address?.takeIf { it.countryCode.equals(target, true) } ?: catalog.pick(target)
        val session = session(email)
        val original = session.currentAddress(catalog::name)
        if (original.countryCode.equals(target, true)) {
            return RegionStatus(target, catalog.name(target), "Аккаунт уже в этой стране.")
        }
        // Записываем «куда возвращать» до запроса на смену, а не после: между запросом
        // и ответом процесс может не дожить, и тогда вернуть будет некуда.
        saveSnapshot(email, original, target)
        val code = session.changeTo(wanted)
        return RegionStatus(code, catalog.name(code), "Страна магазина временно изменена на ${catalog.name(code)}.")
    }

    /** Исходная страна, если смена ещё не откачена. */
    fun pendingRestore(email: String): RegionAddress? = loadSnapshot(email)?.address

    /**
     * Возвращает аккаунт в исходную страну. Три попытки: оставить человека
     * в чужой стране дороже, чем потратить лишние секунды.
     */
    fun restoreOriginal(email: String): RegionStatus? {
        val snapshot = loadSnapshot(email) ?: return null
        var lastError = ""
        for (attempt in 0..2) {
            if (attempt > 0) Thread.sleep(1800)
            val outcome = runCatching {
                val session = session(email)
                if (session.currentCountry().equals(snapshot.address.countryCode, true)) {
                    snapshot.address.countryCode
                } else {
                    session.changeTo(snapshot.address)
                }
            }
            outcome.getOrNull()?.let { code ->
                store.deleteRegionSnapshot(email)
                return RegionStatus(
                    code,
                    catalog.name(code),
                    "Страна магазина возвращена: ${catalog.name(code)}.",
                )
            }
            lastError = outcome.exceptionOrNull()?.message.orEmpty()
        }
        error(
            "Не удалось вернуть страну ${snapshot.address.countryName}. " +
                lastError.ifBlank { "Проверьте интернет и повторите." },
        )
    }

    /**
     * Проверяет и чинит незавершённые смены после перезапуска. Возвращает
     * готовые фразы для показа человеку — по одной на каждый затронутый аккаунт.
     */
    fun recoverInterruptedRegionChange(): List<String> {
        val messages = mutableListOf<String>()
        for (email in store.knownEmails()) {
            val snapshot = loadSnapshot(email) ?: continue
            val home = snapshot.address.countryName
            val stuck = snapshot.target.takeIf { it.length == 3 }?.let { " (${catalog.name(it)})" }.orEmpty()
            val outcome = runCatching { restoreOriginal(email) }
            val restored = outcome.getOrNull()
            when {
                restored != null -> messages += "Страна магазина для $email возвращена: $home."
                outcome.isSuccess -> Unit // снимка уже нет, чинить нечего
                else -> messages += "Страна магазина для $email осталась изменённой$stuck. " +
                    "Войдите в этот Apple ID, чтобы вернуть $home."
            }
        }
        return messages
    }

    /** Подтверждает обновлённые условия Apple, если она их просит. */
    fun acceptPendingMediaTerms(email: String) {
        runCatching { session(email).acceptMediaTerms() }
    }

    // ------------------------------------------------------------------ внутреннее

    private fun session(email: String): CommerceSession {
        val raw = store.loadHistoryCookies(email)?.takeIf { it.isNotBlank() }
            ?: error(CommerceSession.SESSION_MESSAGE)
        val jar = linkedMapOf<String, String>()
        raw.split(';').forEach { part ->
            val idx = part.indexOf('=')
            if (idx > 0) jar[part.substring(0, idx).trim()] = part.substring(idx + 1).trim()
        }
        if (!jar.containsKey("myacinfo")) error(CommerceSession.SESSION_MESSAGE)
        return CommerceSession(jar).also { it.open() }
    }

    private class Snapshot(val address: RegionAddress, val target: String)

    private fun saveSnapshot(email: String, address: RegionAddress, target: String) {
        val o = JSONObject()
            .put("country", address.countryCode)
            .put("name", address.countryName)
            .put("street", address.street)
            .put("city", address.city)
            .put("postal", address.postal)
            .put("phoneArea", address.phoneArea)
            .put("phoneNumber", address.phoneNumber)
            .put("target", target)
            .put("savedAt", System.currentTimeMillis())
        address.state?.let { o.put("state", it) }
        address.county?.let { o.put("county", it) }
        store.storeRegionSnapshot(email, o.toString())
    }

    private fun loadSnapshot(email: String): Snapshot? {
        val raw = store.loadRegionSnapshot(email) ?: return null
        val snapshot = runCatching {
            val o = JSONObject(raw)
            val code = o.optString("country").uppercase()
            val street = o.optString("street")
            val city = o.optString("city")
            val postal = o.optString("postal")
            if (code.length != 3 || street.isEmpty() || city.isEmpty() || postal.isEmpty()) {
                return@runCatching null
            }
            Snapshot(
                address = RegionAddress(
                    countryCode = code,
                    countryName = o.optString("name").ifEmpty { catalog.name(code) },
                    street = street,
                    city = city,
                    postal = postal,
                    state = o.optString("state").trim().takeIf { it.isNotEmpty() },
                    county = o.optString("county").trim().takeIf { it.isNotEmpty() },
                    phoneArea = o.optString("phoneArea").ifEmpty { "000" },
                    phoneNumber = o.optString("phoneNumber").ifEmpty { "00000000" },
                ),
                target = o.optString("target").uppercase(),
            )
        }.getOrNull()
        // Битый снимок чинить нечем, а держать его вечно — значит вечно тревожить человека.
        if (snapshot == null) store.deleteRegionSnapshot(email)
        return snapshot
    }
}
