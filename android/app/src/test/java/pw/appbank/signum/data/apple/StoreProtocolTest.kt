package pw.appbank.signum.data.apple

import com.dd.plist.NSArray
import com.dd.plist.NSData
import com.dd.plist.NSDictionary
import com.dd.plist.NSNumber
import com.dd.plist.NSString
import com.dd.plist.PropertyListParser
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class StoreProtocolTest {
    private fun dict(vararg pairs: Pair<String, Any>): NSDictionary = NSDictionary().apply {
        for ((k, v) in pairs) put(k, v)
    }

    private fun item(withSinf: Boolean, meta: NSDictionary = NSDictionary()): NSDictionary {
        val song = NSDictionary()
        song.put("URL", "https://iosapps.itunes.apple.com/a/Foo.signed.dpkg.ipa?accessKey=1")
        song.put("metadata", meta)
        if (withSinf) song.put("sinfs", NSArray(dict("id" to 0, "sinf" to NSData(byteArrayOf(1, 2, 3)))))
        return dict("songList" to NSArray(song))
    }

    private inline fun <reified T : Throwable> assertThrows(block: () -> Unit): T {
        try {
            block()
        } catch (e: Throwable) {
            if (e is T) return e
            throw AssertionError("expected ${T::class.java.simpleName}, got $e", e)
        }
        fail("expected ${T::class.java.simpleName}")
        throw IllegalStateException()
    }

    @Test
    fun servedRequiresSinfOnlyForDownload() {
        val noSinf = item(withSinf = false)
        assertFalse(StoreProtocol.served(noSinf, needSinfs = true))
        assertTrue(StoreProtocol.served(noSinf, needSinfs = false))
        assertTrue(StoreProtocol.served(item(withSinf = true), needSinfs = true))
        assertFalse(StoreProtocol.served(NSDictionary(), needSinfs = false))
    }

    @Test
    fun failureFallsBackToMetricsMessageCode() {
        val d = dict("metrics" to dict("messageCode" to "2042"), "dialog" to dict("kind" to "authorization"))
        assertEquals("2042", StoreProtocol.failureOf(d))
        assertTrue(StoreProtocol.isAuthorizationDialog(d))
        assertTrue(StoreProtocol.isSessionFailure(d))
        assertTrue(StoreProtocol.isSessionFailure(dict("customerMessage" to "Sign In to the iTunes Store")))
        assertTrue(StoreProtocol.isSessionFailure(dict("failureType" to "1008")))
        assertFalse(StoreProtocol.isSessionFailure(dict("failureType" to "9610")))
    }

    @Test
    fun redownloadUnavailableMatchesNoBreakSpaces() {
        val msg = "Redownload Unavailable with This Apple Account"
        val expl = "This redownload is not available for this Apple Account either because it " +
            "was bought by a different user or the item was refunded or cancelled."
        assertTrue(StoreProtocol.isRedownloadUnavailable(dict("dialog" to dict("message" to msg, "explanation" to expl))))
        assertTrue(StoreProtocol.isRedownloadUnavailable(dict("message" to msg, "explanation" to expl)))
        assertFalse(StoreProtocol.isRedownloadUnavailable(dict("message" to msg, "explanation" to "other")))
    }

    @Test
    fun unavailableResponseNeedsSuffix() {
        assertTrue(StoreProtocol.isUnavailableResponse(200, dict("customerMessage" to "  No Longer Available ")))
        assertTrue(StoreProtocol.isUnavailableResponse(200, dict("customerMessage" to "This app is no longer available")))
        assertFalse(StoreProtocol.isUnavailableResponse(500, dict("customerMessage" to "No Longer Available")))
        assertFalse(StoreProtocol.isUnavailableResponse(200, dict("customerMessage" to "No Longer Available", "failureType" to "1")))
    }

    @Test
    fun purchasePayloadMatchesIpatool() {
        val p = StoreProtocol.purchasePayload("AABBCCDDEEFF", 123L, "STDQ")
        assertEquals("Software-123", StoreProtocol.scalar(p, "origPage"))
        assertEquals("true", StoreProtocol.scalar(p, "buyWithoutAuthorization"))
        assertEquals("0", StoreProtocol.scalar(p, "appExtVrsId"))
        assertTrue(p.objectForKey("salableAdamId") is NSNumber)
        assertEquals("https://p25-buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/buyProduct", StoreProtocol.purchaseUrl("25"))
        assertEquals("https://buy.itunes.apple.com/WebObjects/MZFinance.woa/wa/buyProduct", StoreProtocol.purchaseUrl(null))
    }

    @Test
    fun purchaseResponsesMapToIpatoolErrors() {
        assertThrows<StoreProtocol.TemporarilyUnavailableException> { StoreProtocol.checkPurchase(200, dict("failureType" to "2059")) }
        assertThrows<StoreProtocol.AlreadyPurchasedException> { StoreProtocol.checkPurchase(200, dict("failureType" to "5002")) }
        assertThrows<StoreProtocol.SubscriptionRequiredException> { StoreProtocol.checkPurchase(200, dict("customerMessage" to "Subscription Required")) }
        assertThrows<StoreProtocol.PasswordTokenExpiredException> { StoreProtocol.checkPurchase(200, dict("failureType" to "2034")) }
        assertThrows<StoreProtocol.AlreadyPurchasedException> { StoreProtocol.checkPurchase(500, NSDictionary()) }
        val e = assertThrows<StoreProtocol.StoreException> { StoreProtocol.checkPurchase(200, dict("failureType" to "1", "customerMessage" to "Nope")) }
        assertEquals("Nope", e.message)
        assertThrows<StoreProtocol.StoreException> { StoreProtocol.checkPurchase(200, dict("jingleDocType" to "other")) }
        val ok = dict("jingleDocType" to "purchaseSuccess", "status" to 0)
        assertEquals(ok, StoreProtocol.checkPurchase(200, ok))
    }

    @Test
    fun updateResponseIsValidated() {
        val meta = dict("itemId" to 42L, "softwareVersionExternalIdentifier" to "777", "softwareVersionBundleId" to "com.x")
        val good = item(withSinf = true, meta = meta)
        StoreProtocol.checkUpdateResponse(200, good, 42L, "777", "com.x")
        StoreProtocol.checkUpdateResponse(200, good, 42L, "777", null)
        assertThrows<StoreProtocol.StoreException> { StoreProtocol.checkUpdateResponse(200, good, 42L, "778", null) }
        assertThrows<StoreProtocol.StoreException> { StoreProtocol.checkUpdateResponse(200, good, 42L, "777", "com.y") }
        assertThrows<StoreProtocol.StoreException> { StoreProtocol.checkUpdateResponse(404, good, 42L, "777", null) }
        val structured = dict("failureType" to "9610")
        assertEquals(structured, StoreProtocol.checkUpdateResponse(500, structured, 42L, "777", null))
    }

    @Test
    fun cascadeFailureMapping() {
        assertTrue(StoreProtocol.cascadeFailure("9610", "msg", null) is StoreProtocol.LicenseRequiredException)
        assertEquals("msg", StoreProtocol.cascadeFailure("", "msg", null).message)
        assertTrue(StoreProtocol.cascadeFailure("", "", null).message!!.contains("empty songList"))
    }

    @Test
    fun platformLookupParsesExternalIdAndBuyParams() {
        val a = Json.parseToJsonElement("""{"results":{"5":{"offers":[{"version":{"externalId":123456}}]}}}""").jsonObject
        assertEquals(StoreProtocol.PlatformLookup.Found("123456"), StoreProtocol.parsePlatformLookup(a, 5))
        val b = Json.parseToJsonElement("""{"results":{"5":{"offers":[{"buyParams":"productType=C&appExtVrsId=99&x=1"}]}}}""").jsonObject
        assertEquals(StoreProtocol.PlatformLookup.Found("99"), StoreProtocol.parsePlatformLookup(b, 5))
        val c = Json.parseToJsonElement("""{"results":{}}""").jsonObject
        assertTrue(StoreProtocol.parsePlatformLookup(c, 5) is StoreProtocol.PlatformLookup.Missing)
        val url = StoreProtocol.platformLookupUrl(5, "iphone", "RU")
        assertTrue(url.startsWith("https://uclient-api.itunes.apple.com/WebObjects/MZStorePlatform.woa/wa/lookup?"))
        assertTrue(url.contains("cc=ru") && url.contains("p=mdm-lockup") && url.contains("platform=iphone"))
    }

    @Test
    fun storefrontCountry() {
        assertEquals("RU", StoreProtocol.countryCodeFromStorefront("143469-16,32"))
        assertEquals("US", StoreProtocol.countryCodeFromStorefront("143441"))
        assertNull(StoreProtocol.countryCodeFromStorefront("1"))
        assertNull(StoreProtocol.countryCodeFromStorefront(null))
    }

    @Test
    fun searchUrlEncodesCyrillic() {
        val url = StoreProtocol.searchUrl("сбер онлайн", "RU", 5)
        assertTrue(url.contains("term=%D1%81%D0%B1%D0%B5%D1%80%20%D0%BE%D0%BD%D0%BB%D0%B0%D0%B9%D0%BD"))
        assertTrue(url.contains("entity=software%2CiPadSoftware"))
        assertTrue(StoreProtocol.lookupUrl("RU", bundleId = "com.a.b").contains("bundleId=com.a.b"))
    }

    @Test
    fun fserialAndSongDownloadDoneUrl() {
        // 54 C8 B0 A9 88 + CC DD EE FF
        assertEquals(java.util.Base64.getEncoder().encodeToString(
            byteArrayOf(0x54, 0xC8.toByte(), 0xB0.toByte(), 0xA9.toByte(), 0x88.toByte(), 0xCC.toByte(), 0xDD.toByte(), 0xEE.toByte(), 0xFF.toByte()),
        ), StoreProtocol.fserial("AABBCCDDEEFF"))
        assertEquals(
            "https://p25-buy.itunes.apple.com/WebObjects/MZFastFinance.woa/wa/songDownloadDone?songId=1&download-id=&Pod=25&guid=G",
            StoreProtocol.songDownloadDoneUrl(StoreProtocol.DEFAULT_SONG_DOWNLOAD_DONE_ENDPOINT, "25", 1, "", "G"),
        )
        assertEquals(
            "https://p7-buy.itunes.apple.com/x?songId=1&download-id=d&Pod=25&guid=G",
            StoreProtocol.songDownloadDoneUrl("https://p7-buy.itunes.apple.com/x", "25", 1, "d", "G"),
        )
    }

    @Test
    fun metadataPatchMirrorsItunes() {
        val src = dict("bundleShortVersionString" to "1.0", "itemId" to 5L)
        val m = StoreProtocol.patchedMetadata(src, "a@b.c", "Ivan", "Petrov", "143469-16,32", "123", 1_700_000_000_123L)
        assertEquals("1.0", StoreProtocol.scalar(m, "bundleShortVersionString"))
        assertEquals("a@b.c", StoreProtocol.scalar(m, "appleId"))
        assertEquals("Ivan Petrov", StoreProtocol.scalar(m, "userName"))
        assertEquals("true", StoreProtocol.scalar(m, "is-purchased-redownload"))
        val info = StoreProtocol.dict(StoreProtocol.dict(m, "com.apple.iTunesStore.downloadInfo"), "accountInfo")
        assertEquals("123", StoreProtocol.scalar(info, "DSPersonID"))
        assertEquals("143469-16,32", StoreProtocol.scalar(info, "AccountStoreFront"))
        assertEquals(
            "10|date=1700000000000&sf=143469&app=com.apple.iTunes&pgtp=Purchases&prpg=Purchases",
            StoreProtocol.scalar(m, "storeCohort"),
        )
        // Сериализуется в XML без ошибок.
        assertTrue(m.toXMLPropertyList().contains("com.apple.iTunesStore.downloadInfo"))
    }

    @Test
    fun bagParsesSignedAndFlatForms() {
        val inner = dict("urlBag" to dict("redownloadProduct" to "https://r", "songDownloadDone" to "https://s"))
        val signed = dict("bag" to NSData(inner.toXMLPropertyList().toByteArray()))
        val bag = StoreProtocol.parseBag(signed) { PropertyListParser.parse(it) as NSDictionary }
        assertEquals("https://r", bag.redownloadEndpoint)
        assertEquals("https://s", bag.songDownloadDoneEndpoint)
        assertEquals(StoreProtocol.DEFAULT_UPDATE_ENDPOINT, bag.updateEndpoint)
        val flat = StoreProtocol.parseBag(dict("authenticateAccount" to "https://a", "nested" to dict("updateProduct" to "https://u"))) {
            error("not signed")
        }
        assertEquals("https://a", flat.authEndpoint)
        assertEquals("https://u", flat.updateEndpoint)
    }

    @Test
    fun versionsAndMetadataFromItem() {
        val meta = dict(
            "softwareVersionExternalIdentifiers" to NSArray(NSNumber(1L), NSNumber(2L), NSString("3")),
            "softwareVersionExternalIdentifier" to 3L,
            "bundleShortVersionString" to "2.1",
            "releaseDate" to "2026-01-01T00:00:00Z",
            "softwareVersionBundleId" to "com.x",
        )
        val song = StoreProtocol.firstItem(item(withSinf = false, meta = meta))
        val v = StoreProtocol.versionsFromItem(song)
        assertEquals(listOf("1", "2", "3"), v.ids)
        assertEquals("3", v.latest)
        val md = StoreProtocol.versionMetadataFromItem(song, "3")
        assertEquals("2.1", md.displayVersion)
        assertEquals("2026-01-01T00:00:00Z", md.releaseDate)
        assertThrows<StoreProtocol.StoreException> { StoreProtocol.versionsFromItem(NSDictionary()) }
        assertTrue(StoreProtocol.isIpaUrl("https://h/a/Foo.signed.dpkg.ipa?accessKey=1"))
        assertFalse(StoreProtocol.isIpaUrl("https://h/a/Foo.pkg?x=.ipa"))
    }

    @Test
    fun authDecisionsFollowDoLogin() {
        val bad = dict("customerMessage" to StoreProtocol.CUSTOMER_MSG_BAD_LOGIN)
        assertEquals(StoreProtocol.AuthDecision.NeedsCode, StoreProtocol.decideAuth(200, bad, codeSent = false))
        assertEquals(StoreProtocol.AuthDecision.CodeRejected, StoreProtocol.decideAuth(200, bad, codeSent = true))
        // Неверный пароль: тот же текст, но failureType заполнен.
        val wrong = dict("customerMessage" to StoreProtocol.CUSTOMER_MSG_BAD_LOGIN, "failureType" to "-5000")
        assertEquals(StoreProtocol.AuthDecision.InvalidCredentials, StoreProtocol.decideAuth(200, wrong, codeSent = false))
        assertEquals(
            StoreProtocol.AuthDecision.AccountDisabled,
            StoreProtocol.decideAuth(200, dict("customerMessage" to "Your account is disabled."), false),
        )
        assertEquals(
            StoreProtocol.AuthDecision.Success,
            StoreProtocol.decideAuth(200, dict("passwordToken" to "t", "dsPersonId" to "1"), false),
        )
        assertEquals(
            StoreProtocol.AuthDecision.Failed("something went wrong"),
            StoreProtocol.decideAuth(200, dict("failureType" to "5"), false),
        )
    }

    @Test
    fun normalizeSpacesCollapses() {
        assertEquals("a b c", StoreProtocol.normalizeSpaces("  a   b\n\tc  "))
    }
}
