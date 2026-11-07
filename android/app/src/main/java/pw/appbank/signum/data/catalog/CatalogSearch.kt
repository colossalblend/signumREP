package pw.appbank.signum.data.catalog

import pw.appbank.signum.data.models.CatalogApp

object CatalogSearch {
    private val aliasGroups = listOf(
        listOf("альфа банк", "альфабанк", "alfa bank", "alfabank", "alpha bank"),
        listOf("промсвязьбанк", "пром связь банк", "псб", "psb", "promsvyazbank"),
        listOf("газпромбанк", "газпром банк", "гпб", "gpb", "gazprombank"),
        listOf("т банк", "тбанк", "тинькофф", "t bank", "tbank", "tinkoff"),
        listOf("сбербанк", "сбер банк", "сбер", "sberbank", "sber"),
        listOf("сбер инвестиции", "сберинвестиции", "sber invest"),
        listOf("втб", "vtb"),
        listOf("россельхозбанк", "рсхб", "rshb", "rosselkhozbank"),
        listOf("райффайзенбанк", "райф", "raiffeisen"),
        listOf("московский кредитный банк", "мкб", "mkb"),
        listOf("совкомбанк", "халва", "sovcombank", "sovcom"),
        listOf("рокетбанк", "rocketbank", "рокет"),
        listOf("почта банк", "почтабанк", "pochtabank"),
        listOf("озон банк", "озонбанк", "ozon bank"),
    )

    private val cyrToLat = mapOf(
        'а' to "a", 'б' to "b", 'в' to "v", 'г' to "g", 'д' to "d", 'е' to "e",
        'ж' to "zh", 'з' to "z", 'и' to "i", 'й' to "y", 'к' to "k", 'л' to "l",
        'м' to "m", 'н' to "n", 'о' to "o", 'п' to "p", 'р' to "r", 'с' to "s",
        'т' to "t", 'у' to "u", 'ф' to "f", 'х' to "h", 'ц' to "ts", 'ч' to "ch",
        'ш' to "sh", 'щ' to "sch", 'ъ' to "", 'ы' to "y", 'ь' to "", 'э' to "e",
        'ю' to "yu", 'я' to "ya",
    )

    fun normalize(value: String): String =
        value.lowercase()
            .replace('ё', 'е')
            .replace(Regex("[^a-zа-я0-9]+"), " ")
            .trim()
            .replace(Regex("\\s+"), " ")

    private fun transliterate(value: String) =
        value.map { cyrToLat[it] ?: it.toString() }.joinToString("")

    private fun compact(value: String) = value.replace(" ", "")

    private fun addVariant(target: MutableSet<String>, value: String) {
        val n = normalize(value)
        if (n.isEmpty()) return
        target += n
        target += compact(n)
        val latin = normalize(transliterate(n))
        if (latin.isNotEmpty()) {
            target += latin
            target += compact(latin)
        }
    }

    private fun expand(values: List<String>): Set<String> {
        val result = mutableSetOf<String>()
        values.forEach { addVariant(result, it) }
        for (group in aliasGroups) {
            if (values.any { v -> group.any { alias -> phraseMatches(v, alias) } }) {
                group.forEach { addVariant(result, it) }
            }
        }
        return result
    }

    private fun phraseMatches(value: String, alias: String): Boolean {
        val nv = compact(normalize(value))
        val na = compact(normalize(alias))
        return nv == na || (na.length >= 4 && nv.contains(na))
    }

    fun search(apps: List<CatalogApp>, rawQuery: String): List<CatalogApp> {
        val q = normalize(rawQuery)
        if (q.isEmpty()) return apps
        val query = expand(listOf(q))
        return apps.mapIndexed { index, app ->
            val terms = expand(listOfNotNull(app.title, app.id) + (app.aliases ?: emptyList()))
            Triple(app, index, relevance(query, terms))
        }.filter { it.third > 0 }
            .sortedWith(compareByDescending<Triple<CatalogApp, Int, Int>> { it.third }.thenBy { it.second })
            .map { it.first }
    }

    private fun relevance(query: Set<String>, terms: Set<String>): Int {
        var best = 0
        for (search in query) {
            val tokens = search.split(" ").filter { it.isNotEmpty() }
            for (term in terms) {
                best = maxOf(
                    best,
                    when {
                        term == search -> 1000
                        term.startsWith(search) -> 850
                        term.contains(search) -> 700
                        tokens.size > 1 && tokens.all { t -> term.split(" ").any { it.startsWith(t) } } -> 650
                        else -> 0
                    },
                )
            }
        }
        return best
    }

    fun formatVersion(version: String?, minIos: String?): String {
        val ver = version?.trim().orEmpty()
        val ios = minIos?.trim()?.takeIf { it.isNotEmpty() }?.let { "iOS $it+" }
        return when {
            ver.isNotEmpty() && ios != null -> "v$ver · $ios"
            ver.isNotEmpty() -> "v$ver"
            ios != null -> ios
            else -> "Версия уточняется"
        }
    }
}
