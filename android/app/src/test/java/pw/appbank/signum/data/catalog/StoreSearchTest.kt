package pw.appbank.signum.data.catalog

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import pw.appbank.signum.data.models.AppStoreSearchHit
import pw.appbank.signum.data.models.CatalogApp

class StoreSearchTest {
    @Test
    fun queryNormalization() {
        assertEquals("389801252", StoreSearch.normalizeQuery("https://apps.apple.com/ru/app/x/id389801252"))
        assertEquals("389801252", StoreSearch.normalizeQuery("389 801 252"))
        assertEquals("сбер", StoreSearch.normalizeQuery("  сбер "))
        assertNull(StoreSearch.normalizeQuery("я"))
        assertNull(StoreSearch.normalizeQuery("   "))
    }

    @Test
    fun cyrillicGetsLatinVariant() {
        assertEquals(listOf("сбербанк", "sberbank"), StoreSearch.queryVariants("сбербанк"))
        assertEquals(listOf("tinkoff"), StoreSearch.queryVariants("tinkoff"))
    }

    @Test
    fun appMagicKeepsIosAndMarksRemoved() {
        val body = """
            {"applications":[
              {"name":"Live","store_ids":["1_com.live","2_1111111111"],"store":[2],"removed":false},
              {"name":"Gone","store_ids":["3_2222222222"],"removed":true,"removed_stores":[3]},
              {"name":"Android only","store_ids":["1_com.droid"]},
              {"name":"Dup","store_ids":["2_1111111111"]}
            ]}
        """.trimIndent()
        val hits = StoreSearch.parseAppMagic(body, "live")
        assertEquals(listOf("1111111111", "2222222222"), hits.map { it.adamId })
        assertFalse(hits[0].removedFromStore)
        assertTrue(hits[1].removedFromStore)
    }

    @Test
    fun itunesSkipsArtistRow() {
        val body = """
            {"results":[
              {"wrapperType":"artist","artistId":42,"artistName":"Studio"},
              {"kind":"software","trackId":389801252,"trackName":"Demo","artistName":"Studio","artistId":42,
               "artworkUrl100":"https://is1-ssl.mzstatic.com/a.png","bundleId":"com.demo","version":"1.2"}
            ]}
        """.trimIndent()
        val hits = StoreSearch.parseItunes(body)
        assertEquals(1, hits.size)
        assertEquals("389801252", hits[0].adamId)
        assertEquals("com.demo", hits[0].bundleId)
        val devs = StoreSearch.groupByDeveloper(body)
        assertEquals(1, devs.size)
        assertEquals(1, devs[0].appCount)
        assertEquals(1, devs[0].icons.size)
    }

    @Test
    fun appMagicAndItunesMergeById() {
        val magic = listOf(
            listOf(
                AppStoreSearchHit(adamId = "1", name = "A", artwork = "https://appmagic/a.png"),
                AppStoreSearchHit(adamId = "2", name = "B", removedFromStore = true),
            ),
            listOf(AppStoreSearchHit(adamId = "1", name = "A again")),
        )
        val itunes = listOf(
            AppStoreSearchHit(adamId = "1", name = "A", artist = "Dev", artwork = "https://is1.mzstatic.com/a.png", bundleId = "com.a"),
            AppStoreSearchHit(adamId = "3", name = "C"),
        )
        val merged = StoreSearch.mergeHits(magic, itunes)
        assertEquals(listOf("1", "2", "3"), merged.map { it.adamId })
        assertEquals("Dev", merged[0].artist)
        assertEquals("com.a", merged[0].bundleId)
        assertEquals("https://is1.mzstatic.com/a.png", merged[0].artwork)
        assertTrue(merged[1].removedFromStore)
    }

    @Test
    fun catalogCardReplacesStoreHitWithSameId() {
        val sber = CatalogApp(id = "sber", title = "СберБанк", app_store_ids = listOf("100"))
        val other = CatalogApp(id = "vtb", title = "ВТБ", app_store_id = "200")
        val hits = listOf(
            AppStoreSearchHit(adamId = "100", name = "SberBank", artwork = "icon"),
            AppStoreSearchHit(adamId = "200", name = "VTB"),
            AppStoreSearchHit(adamId = "300", name = "Other"),
            AppStoreSearchHit(adamId = "100", name = "SberBank dup"),
        )
        val merged = StoreSearch.mergeWithCatalog(hits, listOf(sber, other), shownCatalogIds = setOf("vtb"))
        assertEquals(listOf("100", "300"), merged.map { it.hit.adamId })
        assertEquals("sber", merged[0].catalog?.id)
        // Карточка каталога дополнена значком из App Store.
        assertEquals("icon", merged[0].catalog?.icon_url)
        assertNull(merged[1].catalog)
    }
}
