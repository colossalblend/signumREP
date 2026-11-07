package pw.appbank.signum.data.catalog

import pw.appbank.signum.data.models.AccountRestoreVersion
import pw.appbank.signum.data.models.CatalogApp
import pw.appbank.signum.data.models.CatalogGroup
import pw.appbank.signum.data.models.CatalogInstallOffer
import pw.appbank.signum.data.models.CatalogSection
import pw.appbank.signum.data.models.CatalogVersion
import pw.appbank.signum.data.models.InstalledApp

object CatalogResolve {
    val FALLBACK_TITLES = mapOf(
        "appbank" to "AppBank",
        "sber" to "Сбер",
        "tbank" to "Т-Банк",
        "alfa" to "Альфа",
        "gazprom" to "Газпромбанк",
        "novikom" to "Новикомбанк",
        "sovcom" to "Совкомбанк",
        "rocket" to "Рокетбанк",
        "zenit" to "Зенит Банк",
        "psb" to "ПСБ",
        "rsxb" to "РСХБ",
        "mts" to "МТС",
        "lenta" to "Лента",
        "vk" to "ВК",
        "yoomoney" to "ЮMoney",
        "rutube" to "Rutube",
        "avito" to "Авито",
        "happ" to "Happ",
        "bank" to "Банки",
        "investment" to "Инвестиции",
        "app" to "Другие",
        "other" to "Другие",
    )

    val DEFAULT_GROUPS = listOf(
        CatalogGroup("sber", "Сбер", 100),
        CatalogGroup("tbank", "Т-Банк", 90),
        CatalogGroup("alfa", "Альфа", 80),
        CatalogGroup("gazprom", "Газпромбанк", 70),
        CatalogGroup("novikom", "Новикомбанк", 60),
        CatalogGroup("other", "Другие", 10),
    )

    fun sections(apps: List<CatalogApp>, groups: List<CatalogGroup>): List<CatalogSection> {
        val map = linkedMapOf<String, MutableList<CatalogApp>>()
        for (app in apps) {
            val key = app.category?.trim()?.takeIf { it.isNotEmpty() } ?: "other"
            map.getOrPut(key) { mutableListOf() }.add(app)
        }
        val ordered = groups.ifEmpty { DEFAULT_GROUPS }.sortedByDescending { it.priority ?: 0 }
        val used = ordered.map { it.id }.toSet()
        val out = mutableListOf<CatalogSection>()
        for (group in ordered) {
            val items = map[group.id] ?: continue
            if (items.isNotEmpty()) out += CatalogSection(group.id, group.title.ifBlank { group.id }, items)
        }
        for ((id, items) in map) {
            if (id in used || items.isEmpty()) continue
            out += CatalogSection(id, FALLBACK_TITLES[id] ?: id, items)
        }
        return out
    }

    fun pickPrimary(versions: List<CatalogVersion>, deviceIos: String?): CatalogVersion? {
        if (versions.isEmpty()) return null
        val sorted = versions.sortedWith(
            compareByDescending<CatalogVersion> { it.inStore }
                .thenByDescending { it.isLatest }
                .thenByDescending { versionScore(it.version) },
        )
        sorted.firstOrNull { it.inStore && compatible(it, deviceIos) }?.let { return it }
        sorted.firstOrNull { it.inStore }?.let { return it }
        sorted.firstOrNull { !it.inStore && compatible(it, deviceIos) && it.hasIdentity }?.let { return it }
        return sorted.firstOrNull { it.hasIdentity } ?: sorted.firstOrNull()
    }

    fun applyVersion(
        offer: CatalogInstallOffer,
        version: CatalogVersion,
        deviceIos: String?,
    ): CatalogInstallOffer {
        val incompatible = !compatible(version, deviceIos) &&
            !version.minIos.isNullOrBlank() &&
            !deviceIos.isNullOrBlank()
        val message = when {
            version.inStore && incompatible ->
                "Версия из App Store требует iOS ${version.minIos}. Установить можно, но на этом iPhone приложение не запустится. Можно выбрать другую версию."
            version.inStore ->
                "Версия доступна в App Store и будет привязана к Apple ID перед установкой."
            else ->
                "Этой версии уже нет в App Store. Установить можно только если она уже есть в покупках Apple ID. Ниже можно выбрать другую версию."
        }
        return offer.copy(
            title = version.title.ifBlank { offer.appTitle },
            description = version.description,
            version = version.version.ifBlank { offer.version },
            bundleId = version.bundleId ?: offer.bundleId,
            appStoreId = version.appStoreId,
            minIos = version.minIos,
            iconUrl = version.iconUrl ?: offer.iconUrl,
            inStore = version.inStore,
            source = if (version.inStore) "app_store" else "restore",
            message = message,
            deviceCompatible = when {
                version.minIos.isNullOrBlank() || deviceIos.isNullOrBlank() -> null
                incompatible -> false
                else -> true
            },
            deviceIos = deviceIos,
        )
    }

    fun compatible(version: CatalogVersion, deviceIos: String?): Boolean {
        val need = version.minIos?.trim().orEmpty()
        val have = deviceIos?.trim().orEmpty()
        if (need.isEmpty() || have.isEmpty()) return true
        return compareIos(have, need) >= 0
    }

    fun compareIos(have: String, need: String): Int {
        val a = iosParts(have)
        val b = iosParts(need)
        val n = maxOf(a.size, b.size)
        for (i in 0 until n) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x.compareTo(y)
        }
        return 0
    }

    private fun iosParts(value: String): List<Int> =
        value.split(Regex("[^0-9]+")).mapNotNull { it.toIntOrNull() }

    fun toRestoreItem(
        appId: String,
        appTitle: String,
        version: CatalogVersion,
        deviceIos: String?,
        installed: Map<String, InstalledApp>,
    ): AccountRestoreVersion? {
        if (!version.hasIdentity) return null
        val bundle = version.bundleId.orEmpty()
        val onDevice = bundle.isNotEmpty() && bundle != "NONE" && installed.containsKey(bundle)
        return AccountRestoreVersion(
            appId = appId,
            appTitle = appTitle,
            title = version.title.ifBlank { appTitle },
            description = version.description,
            version = version.version,
            bundleId = bundle,
            appStoreId = version.appStoreId,
            minIos = version.minIos,
            iconUrl = version.iconUrl,
            inStore = version.inStore,
            deviceCompatible = when {
                version.minIos.isNullOrBlank() || deviceIos.isNullOrBlank() -> null
                else -> compatible(version, deviceIos)
            },
            installedOnDevice = onDevice,
            installedVersion = installed[bundle]?.version,
        )
    }

    fun applyRestoreItem(
        offer: CatalogInstallOffer,
        item: AccountRestoreVersion,
        deviceIos: String?,
    ): CatalogInstallOffer {
        val cover = CatalogVersion(
            title = item.title,
            version = item.version,
            bundleId = item.bundleId,
            appStoreId = item.appStoreId,
            minIos = item.minIos,
            iconUrl = item.iconUrl,
            inStore = item.inStore,
            description = item.description,
        )
        return applyVersion(offer, cover, deviceIos).copy(
            installedOnDevice = item.installedOnDevice,
            installedVersion = item.installedVersion,
            deviceCompatible = item.deviceCompatible,
        )
    }

    fun formatAppStoreVersionLabel(version: String, externalVersionId: String): String {
        val v = version.trim()
        val id = externalVersionId.trim()
        if (v.isEmpty()) return if (id.isNotEmpty()) "сборка $id" else "—"
        if (id.isNotEmpty() && v == id) return "сборка $id"
        if (v.all { it.isDigit() } && v.length >= 8) return "сборка $v"
        return if (v.startsWith("v") || v.startsWith("V")) v else "v$v"
    }

    fun versionMatchesSearch(version: String, externalVersionId: String, query: String): Boolean {
        val q = query.trim().lowercase()
        if (q.isEmpty()) return true
        val v = version.trim().lowercase()
        val id = externalVersionId.trim().lowercase()
        val qDigits = q.replace(" ", "")
        if (v.contains(q) || id.contains(q) || id.contains(qDigits)) return true
        return v.removePrefix("v").contains(q.removePrefix("v"))
    }

    private fun versionScore(value: String): Int {
        val parts = iosParts(value)
        var score = 0
        parts.take(4).forEachIndexed { index, n ->
            score += n * when (index) {
                0 -> 1_000_000
                1 -> 1_000
                2 -> 10
                else -> 1
            }
        }
        return score
    }
}

enum class BankAltKey { Sovcombank, Rocketbank }

fun detectBankAlt(vararg parts: String?): BankAltKey? {
    val n = parts.filterNotNull().joinToString(" ").lowercase().replace('ё', 'е')
    val compact = n.replace(Regex("\\s+"), "")
    if (compact.contains("рокет") || compact.contains("rocket")) return BankAltKey.Rocketbank
    if (
        compact.contains("совком") ||
        compact.contains("sovcom") ||
        compact.contains("халва") ||
        compact.contains("halva")
    ) {
        return BankAltKey.Sovcombank
    }
    return null
}

fun BankAltKey.displayName(): String = when (this) {
    BankAltKey.Sovcombank -> "Совкомбанк"
    BankAltKey.Rocketbank -> "Рокетбанк"
}
