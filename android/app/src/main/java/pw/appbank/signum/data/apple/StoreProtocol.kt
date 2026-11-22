package pw.appbank.signum.data.apple

import com.dd.plist.NSArray
import com.dd.plist.NSData
import com.dd.plist.NSDate
import com.dd.plist.NSDictionary
import com.dd.plist.NSNumber
import com.dd.plist.NSObject
import com.dd.plist.NSString
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import java.io.File
import java.net.URLEncoder
import java.security.MessageDigest
import java.util.Base64
import java.util.Date

/**
 * Протокол App Store в том виде, в каком его ведёт ipatool 1.3.2
 * (Sorvigolova/ipatool, ipatool-cpp; этот же движок встроен в Signum на ПК).
 *
 * Здесь только чистая логика без сети и без Android: коды отказов, формы
 * запросов, разбор ответов и решения каскада скачивания. Сетью занимается
 * [AppStoreClient]; вынос сюда позволяет проверять каждое правило JVM-тестом.
 */
object StoreProtocol {
    const val UPSTREAM_VERSION = "1.3.2"

    // ── Коды отказов Store (ipatool.h) ──────────────────────────────────────
    const val FAILURE_INVALID_CREDENTIALS = "-5000"
    const val FAILURE_PASSWORD_TOKEN_EXPIRED = "2034"
    const val FAILURE_LICENSE_NOT_FOUND = "9610"
    const val FAILURE_TEMPORARILY_UNAVAILABLE = "2059"
    const val FAILURE_ALREADY_PURCHASED = "5002"
    const val FAILURE_SIGN_IN_REQUIRED = "2042"
    const val FAILURE_DEVICE_VERIFICATION = "1008"

    const val CUSTOMER_MSG_BAD_LOGIN = "MZFinance.BadLogin.Configurator_message"
    const val CUSTOMER_MSG_ACCOUNT_DISABLED = "Your account is disabled."
    const val CUSTOMER_MSG_SUBSCRIPTION_REQUIRED = "Subscription Required"
    const val CUSTOMER_MSG_SIGN_IN = "Sign In to the iTunes Store"

    const val PRICING_APPSTORE = "STDQ"
    const val PRICING_ARCADE = "GAME"

    // ── Адреса: сумка (bag.xml) и то, что она отдаёт по умолчанию ───────────
    const val BAG_URL = "https://init.itunes.apple.com/bag.xml"
    const val BAG_USER_AGENT = "Configurator/2.21 (Macintosh; OS X 15.7; 24G222) AppleWebKit/0620.2.4.11.6"
    const val DEFAULT_AUTH_ENDPOINT = "https://auth.itunes.apple.com/auth/v1/native"
    const val DEFAULT_REDOWNLOAD_ENDPOINT = "https://downloaddispatch.itunes.apple.com/r/redownload"
    const val DEFAULT_UPDATE_ENDPOINT = "https://downloaddispatch.itunes.apple.com/up/updateProduct"
    const val DEFAULT_SONG_DOWNLOAD_DONE_ENDPOINT =
        "https://buy.itunes.apple.com/WebObjects/MZFastFinance.woa/wa/songDownloadDone"
    const val PURCHASE_PATH = "/WebObjects/MZFinance.woa/wa/buyProduct"
    const val PLATFORM_LOOKUP_URL = "https://uclient-api.itunes.apple.com/WebObjects/MZStorePlatform.woa/wa/lookup"
    val PLATFORM_CATALOGS = listOf("enterprisestore", "iphone", "ipad")

    const val REDOWNLOAD_UNAVAILABLE_MESSAGE = "Redownload Unavailable with This Apple Account"
    const val REDOWNLOAD_UNAVAILABLE_EXPLANATION =
        "This redownload is not available for this Apple Account either because it " +
            "was bought by a different user or the item was refunded or cancelled."

    // ── Ошибки (ipatool.h: IpaError и наследники) ────────────────────────────
    //
    // Все они IllegalStateException, как и error(): существующие вызывающие
    // ловят Exception и показывают message, так что поведение для них не меняется.
    open class StoreException(message: String) : IllegalStateException(message)

    class AuthCodeRequiredException :
        StoreException("Введите 6-значный код 2FA с доверенного устройства Apple.")

    class LicenseRequiredException :
        StoreException("Приложение не куплено на этом Apple ID: нужна лицензия (license is required).")

    class PasswordTokenExpiredException :
        StoreException("Сессия App Store устарела (password token is expired). Войдите в Apple ID заново.")

    class SubscriptionRequiredException :
        StoreException("Для этого приложения нужна подписка (subscription required).")

    class PaidAppNotSupportedException :
        StoreException("Платное приложение нельзя купить из Signum: купите его в App Store на iPhone, затем скачайте здесь.")

    class AlreadyPurchasedException :
        StoreException("Приложение уже куплено на этом Apple ID (license already exists).")

    class TemporarilyUnavailableException :
        StoreException("Приложение временно недоступно (item is temporarily unavailable).")

    // ── Доступ к значениям plist ────────────────────────────────────────────

    /** Строка или число по ключу; пусто, если ключа нет. */
    fun scalar(d: NSDictionary?, key: String): String {
        val v = d?.objectForKey(key) ?: return ""
        return when (v) {
            is NSString -> v.content
            is NSNumber -> when (v.type()) {
                NSNumber.INTEGER -> v.longValue().toString()
                NSNumber.BOOLEAN -> v.boolValue().toString()
                else -> v.toString()
            }
            is NSDate -> v.toString()
            else -> ""
        }
    }

    fun dict(d: NSDictionary?, key: String): NSDictionary? = d?.objectForKey(key) as? NSDictionary

    fun songList(d: NSDictionary?): List<NSObject> =
        (d?.objectForKey("songList") as? NSArray)?.array?.filterNotNull().orEmpty()

    fun firstItem(d: NSDictionary?): NSDictionary? = songList(d).firstOrNull() as? NSDictionary

    /** failureType, а без него metrics.messageCode — туда Apple заворачивает 2042. */
    fun failureOf(d: NSDictionary?): String {
        val ft = scalar(d, "failureType")
        if (ft.isNotEmpty()) return ft
        return scalar(dict(d, "metrics"), "messageCode")
    }

    /** Элемент годится для скачивания, только если в нём есть sinf (или dpInfo для macOS). */
    fun hasSinfs(d: NSDictionary?): Boolean {
        val item = firstItem(d) ?: return false
        val sinfs = item.objectForKey("sinfs") as? NSArray ?: return false
        return sinfs.array.any { s ->
            val sd = s as? NSDictionary ?: return@any false
            listOf("sinf", "dpInfo").any { key -> (sd.objectForKey(key) as? NSData)?.bytes()?.isNotEmpty() == true }
        }
    }

    /** «Обслужен» ли запрос: есть элемент, а для скачивания ещё и лицензия. */
    fun served(d: NSDictionary?, needSinfs: Boolean): Boolean {
        if (songList(d).isEmpty()) return false
        return !needSinfs || hasSinfs(d)
    }

    fun sinfs(item: NSDictionary?): List<ByteArray> =
        (item?.objectForKey("sinfs") as? NSArray)?.array
            ?.mapNotNull { (it as? NSDictionary)?.objectForKey("sinf") as? NSData }
            ?.map { it.bytes() }
            .orEmpty()

    // ── Отказы redownload / updateProduct ───────────────────────────────────

    /**
     * Apple пишет «Apple Account» с неразрывным пробелом; приводим все виды
     * пробелов к обычному, схлопываем и обрезаем, чтобы сравнивать фразы целиком.
     */
    fun normalizeSpaces(input: String): String {
        val out = StringBuilder()
        var pending = false
        for (c in input) {
            val space = c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == ' ' ||
                c in ' '..' ' || c == ' '
            if (space) {
                pending = true
                continue
            }
            if (pending && out.isNotEmpty()) out.append(' ')
            pending = false
            out.append(c)
        }
        return out.toString()
    }

    /** Отказ redownload «эта покупка не на этом Apple ID» — значит, лицензии нет. */
    fun isRedownloadUnavailable(d: NSDictionary?): Boolean {
        fun match(m: NSDictionary?) = m != null &&
            normalizeSpaces(scalar(m, "message")) == REDOWNLOAD_UNAVAILABLE_MESSAGE &&
            normalizeSpaces(scalar(m, "explanation")) == REDOWNLOAD_UNAVAILABLE_EXPLANATION
        return match(d) || match(dict(d, "dialog"))
    }

    /** 200, без failureType и элементов, сообщение «… No Longer Available». */
    fun isUnavailableResponse(status: Int, d: NSDictionary?): Boolean {
        if (status != 200 || scalar(d, "failureType").isNotEmpty() || songList(d).isNotEmpty()) return false
        val m = scalar(d, "customerMessage").trim().lowercase()
        return m == "no longer available" || (m.length > " no longer available".length && m.endsWith(" no longer available"))
    }

    /** 2042 с диалогом kind=authorization — повторить с hasBeenAuthedForBuy. */
    fun isAuthorizationDialog(d: NSDictionary?): Boolean =
        failureOf(d) == FAILURE_SIGN_IN_REQUIRED && scalar(dict(d, "dialog"), "kind") == "authorization"

    /** Отказ сессии, а не лицензии: нужен повторный вход. */
    fun isSessionFailure(d: NSDictionary?): Boolean {
        val f = failureOf(d)
        return f == FAILURE_PASSWORD_TOKEN_EXPIRED || f == FAILURE_SIGN_IN_REQUIRED ||
            f == FAILURE_DEVICE_VERIFICATION || scalar(d, "customerMessage") == CUSTOMER_MSG_SIGN_IN
    }

    // ── Формы запросов ───────────────────────────────────────────────────────

    /** Тело redownloadProduct / updateProduct (download_request_payload). */
    fun downloadRequestPayload(guid: String, adamId: Long, appExtVrsId: String?): NSDictionary =
        NSDictionary().apply {
            put("creditDisplay", NSString(""))
            put("guid", NSString(guid))
            put("salableAdamId", NSNumber(adamId))
            put("serialNumber", NSString("0"))
            if (!appExtVrsId.isNullOrEmpty()) put("appExtVrsId", NSString(appExtVrsId))
        }

    /** Тело buyProduct (do_purchase). */
    fun purchasePayload(guid: String, adamId: Long, pricingParameters: String): NSDictionary =
        NSDictionary().apply {
            put("appExtVrsId", NSString("0"))
            put("hasAskedToFulfillPreorder", NSString("true"))
            put("buyWithoutAuthorization", NSString("true"))
            put("hasDoneAgeCheck", NSString("true"))
            put("guid", NSString(guid))
            put("needDiv", NSString("0"))
            put("origPage", NSString("Software-$adamId"))
            put("origPageLocation", NSString("Buy"))
            put("price", NSString("0"))
            put("pricingParameters", NSString(pricingParameters))
            put("productType", NSString("C"))
            put("salableAdamId", NSNumber(adamId))
        }

    fun purchaseUrl(pod: String?): String {
        val prefix = StorePodRouting.validPod(pod)?.let { "p$it-" }.orEmpty()
        return "https://${prefix}buy.itunes.apple.com$PURCHASE_PATH"
    }

    /**
     * Разбор ответа buyProduct в том же порядке проверок, что у ipatool.
     * Возвращает ответ при успехе, иначе бросает исключение.
     */
    fun checkPurchase(status: Int, d: NSDictionary): NSDictionary {
        val failure = scalar(d, "failureType")
        val message = scalar(d, "customerMessage")
        when {
            failure == FAILURE_TEMPORARILY_UNAVAILABLE -> throw TemporarilyUnavailableException()
            failure == FAILURE_ALREADY_PURCHASED -> throw AlreadyPurchasedException()
            message == CUSTOMER_MSG_SUBSCRIPTION_REQUIRED -> throw SubscriptionRequiredException()
            failure == FAILURE_PASSWORD_TOKEN_EXPIRED -> throw PasswordTokenExpiredException()
            message == CUSTOMER_MSG_SIGN_IN -> throw PasswordTokenExpiredException()
            failure.isNotEmpty() && message.isNotEmpty() -> throw StoreException(message)
            failure.isNotEmpty() -> throw StoreException("Покупка не удалась (failureType $failure).")
            status == 500 -> throw AlreadyPurchasedException()
            scalar(d, "jingleDocType") != "purchaseSuccess" || (scalar(d, "status").ifEmpty { "0" } != "0") ->
                throw StoreException("Не удалось получить приложение в App Store (failed to purchase app).")
        }
        return d
    }

    /**
     * Проверка ответа updateProduct (update_product). Структурный отказ
     * (failureType) возвращается как есть — его разбирает каскад.
     */
    fun checkUpdateResponse(status: Int, d: NSDictionary, adamId: Long, externalVersionId: String, bundleId: String?): NSDictionary {
        if (scalar(d, "failureType").isNotEmpty()) return d
        val message = scalar(d, "customerMessage")
        if (message.isNotEmpty()) throw StoreException("received update error: $message")
        if (status != 200) throw StoreException("received unexpected update status code: $status")
        val items = songList(d)
        if (items.size != 1 || items[0] !is NSDictionary) throw StoreException("update response must contain exactly one item")
        val meta = dict(items[0] as NSDictionary, "metadata")
            ?: throw StoreException("update response does not match the requested app or version")
        if (scalar(meta, "itemId") != adamId.toString() ||
            scalar(meta, "softwareVersionExternalIdentifier") != externalVersionId
        ) {
            throw StoreException("update response does not match the requested app or version")
        }
        val responseBundle = scalar(meta, "softwareVersionBundleId")
        if (responseBundle.isEmpty() || (!bundleId.isNullOrEmpty() && responseBundle != bundleId)) {
            throw StoreException("update response does not match the requested bundle identifier")
        }
        return d
    }

    /**
     * Итог каскада, когда ни один адрес не отдал приложение (хвост resolve_download).
     * Возвращает исключение, которое нужно бросить.
     */
    fun cascadeFailure(vsFailure: String, customerMessage: String, vsData: NSDictionary?): StoreException = when {
        vsFailure == FAILURE_LICENSE_NOT_FOUND -> LicenseRequiredException()
        customerMessage.isNotEmpty() -> StoreException(customerMessage)
        scalar(vsData, "jingleDocType") == "purchaseSuccess" ->
            StoreException("Приложение недоступно для скачивания в регионе этого Apple ID (лицензия выдана, но загрузка запрещена).")
        vsFailure.isNotEmpty() -> StoreException("App Store вернул ошибку: $vsFailure")
        else -> StoreException("App Store не вернул приложение (invalid response: empty songList).")
    }

    // ── Последняя версия через MZStorePlatform (mdm-lockup) ──────────────────

    fun platformLookupUrl(adamId: Long, catalog: String, countryCode: String): String {
        val params = sortedMapOf(
            "version" to "2", "id" to adamId.toString(), "p" to "mdm-lockup", "caller" to "MDM",
            "platform" to catalog, "cc" to countryCode.lowercase(), "l" to "en",
        )
        return PLATFORM_LOOKUP_URL + "?" + params.entries.joinToString("&") { (k, v) ->
            enc(k) + "=" + enc(v)
        }
    }

    /** appExtVrsId из строки buyParams («…&appExtVrsId=123&…»). */
    fun externalVersionIdFromBuyParams(buyParams: String): String =
        buyParams.split('&').firstNotNullOfOrNull { kv ->
            val eq = kv.indexOf('=')
            if (eq > 0 && kv.substring(0, eq) == "appExtVrsId") kv.substring(eq + 1) else null
        }.orEmpty()

    sealed class PlatformLookup {
        data class Found(val externalVersionId: String) : PlatformLookup()
        data class Missing(val reason: String) : PlatformLookup()
    }

    /** Разбор ответа mdm-lockup одного каталога. */
    fun parsePlatformLookup(root: JsonObject, adamId: Long): PlatformLookup {
        val item = (root["results"] as? JsonObject)?.get(adamId.toString()) as? JsonObject
            ?: return PlatformLookup.Missing("platform version lookup returned no app")
        val offer = (item["offers"]?.let { runCatching { it.jsonArray }.getOrNull() })
            ?.firstOrNull() as? JsonObject
            ?: return PlatformLookup.Missing("platform version lookup returned no offers")
        var ext = ((offer["version"] as? JsonObject)?.get("externalId"))?.let(::jsonIdString).orEmpty()
        if (ext.isEmpty()) {
            ext = (offer["buyParams"] as? JsonPrimitive)?.takeIf { it.isString }?.content
                ?.let(::externalVersionIdFromBuyParams).orEmpty()
        }
        if (ext.isEmpty()) throw StoreException("platform version lookup returned no external version id")
        return PlatformLookup.Found(ext)
    }

    private fun jsonIdString(v: JsonElement): String = when {
        v is JsonNull -> ""
        v is JsonPrimitive && v.isString -> v.content
        v is JsonPrimitive && v.longOrNull != null -> v.longOrNull.toString()
        v is JsonPrimitive -> v.contentOrNull.orEmpty()
        else -> throw StoreException("invalid external version id $v")
    }

    // ── Витрина → код страны ─────────────────────────────────────────────────

    private val STOREFRONTS: Map<String, String> = """
        AE 143481 AG 143540 AI 143538 AL 143575 AM 143524 AO 143564 AR 143505 AT 143445 AU 143460 AZ 143568
        BB 143541 BD 143490 BE 143446 BG 143526 BH 143559 BM 143542 BN 143560 BO 143556 BR 143503 BS 143539
        BW 143525 BY 143565 BZ 143555 CA 143455 CH 143459 CI 143527 CL 143483 CN 143465 CO 143501 CR 143495
        CY 143557 CZ 143489 DE 143443 DK 143458 DM 143545 DO 143508 DZ 143563 EC 143509 EE 143518 EG 143516
        ES 143454 FI 143447 FR 143442 GB 143444 GD 143546 GE 143615 GH 143573 GR 143448 GT 143504 GY 143553
        HK 143463 HN 143510 HR 143494 HU 143482 ID 143476 IE 143449 IL 143491 IN 143467 IQ 143617 IS 143558
        IT 143450 JM 143511 JO 143528 JP 143462 KE 143529 KN 143548 KR 143466 KW 143493 KY 143544 KZ 143517
        LB 143497 LC 143549 LI 143522 LK 143486 LT 143520 LU 143451 LV 143519 MD 143523 MG 143531 MK 143530
        ML 143532 MN 143592 MO 143515 MS 143547 MT 143521 MU 143533 MV 143488 MX 143468 MY 143473 NE 143534
        NG 143561 NI 143512 NL 143452 NO 143457 NP 143484 NZ 143461 OM 143562 PA 143485 PE 143507 PH 143474
        PK 143477 PL 143478 PT 143453 PY 143513 QA 143498 RO 143487 RS 143500 RU 143469 SA 143479 SE 143456
        SG 143464 SI 143499 SK 143496 SN 143535 SR 143554 SV 143506 TC 143552 TH 143475 TN 143536 TR 143480
        TT 143551 TW 143470 TZ 143572 UA 143492 UG 143537 US 143441 UY 143514 UZ 143566 VC 143550 VE 143502
        VG 143543 VN 143471 YE 143571 ZA 143472
    """.trim().split(Regex("\\s+")).chunked(2).associate { (cc, id) -> id to cc }

    /** «143469-16,32» → «RU»; null, если витрина неизвестна. */
    fun countryCodeFromStorefront(storeFront: String?): String? {
        val numeric = storeFront?.trim()?.substringBefore('-')?.substringBefore(',') ?: return null
        return STOREFRONTS[numeric]
    }

    // ── Поиск и lookup iTunes (search_url / lookup_url) ──────────────────────

    fun searchUrl(term: String, countryCode: String, limit: Int): String =
        "https://itunes.apple.com/search?" + query(
            "entity" to "software,iPadSoftware", "limit" to limit.toString(), "media" to "software",
            "term" to term, "country" to countryCode,
        )

    fun lookupUrl(countryCode: String, bundleId: String? = null, adamId: String? = null): String =
        "https://itunes.apple.com/lookup?" + query(
            "entity" to "software,iPadSoftware", "limit" to "1", "media" to "software",
            *listOfNotNull(bundleId?.let { "bundleId" to it }, adamId?.let { "id" to it }).toTypedArray(),
            "country" to countryCode,
        )

    private fun query(vararg params: Pair<String, String>): String =
        params.sortedBy { it.first }.joinToString("&") { (k, v) -> enc(k) + "=" + enc(v) }

    /** Кодирование как в ipatool: всё, кроме [A-Za-z0-9-_.~], — %XX. */
    fun enc(s: String): String = URLEncoder.encode(s, "UTF-8")
        .replace("+", "%20").replace("*", "%2A").replace("%7E", "~")

    // ── Прочее ───────────────────────────────────────────────────────────────

    /** Вымышленный, но корректный серийный номер из GUID (make_fserial). */
    fun fserial(guidHex: String): String {
        val bytes = mutableListOf<Byte>(0x54, 0xC8.toByte(), 0xB0.toByte(), 0xA9.toByte(), 0x88.toByte())
        val tail = if (guidHex.length >= 8) guidHex.takeLast(8) else guidHex
        var i = 0
        while (i + 1 < tail.length) {
            val b = tail.substring(i, i + 2).toIntOrNull(16) ?: break
            bytes += b.toByte()
            i += 2
        }
        return Base64.getEncoder().encodeToString(bytes.toByteArray())
    }

    /** Адрес отчёта о завершённой загрузке (report_download_done). */
    fun songDownloadDoneUrl(endpoint: String, pod: String?, songId: Long, downloadId: String, guid: String): String {
        var url = endpoint
        val p = StorePodRouting.validPod(pod)
        if (p != null) {
            val scheme = url.indexOf("://")
            if (scheme >= 0) {
                val host = url.substring(scheme + 3)
                if (!Regex("^p\\d+-").containsMatchIn(host)) {
                    url = url.substring(0, scheme + 3) + "p$p-" + host
                }
            }
        }
        return url + "?songId=$songId&download-id=$downloadId&Pod=${p.orEmpty()}&guid=$guid"
    }

    /** Адрес архива — настоящий .ipa (а не pkg для macOS)? */
    fun isIpaUrl(url: String?): Boolean =
        url?.substringBefore('?')?.lowercase()?.endsWith(".ipa") == true

    fun isMacPackage(url: String?, metadata: NSDictionary?): Boolean =
        url?.substringBefore('?')?.endsWith(".pkg") == true || scalar(metadata, "software-platform") == "macos"

    /**
     * iTunesMetadata.plist, как его пишет iTunes (apply_patches): данные
     * аккаунта, downloadInfo, дата загрузки, is-purchased-redownload, storeCohort.
     */
    fun patchedMetadata(
        source: NSDictionary?,
        appleId: String,
        firstName: String,
        lastName: String,
        storeFront: String,
        dsid: String,
        nowMillis: Long,
    ): NSDictionary {
        val meta = NSDictionary()
        source?.let { src -> for (key in src.allKeys()) meta.put(key, src.objectForKey(key)) }
        val userName = "$firstName $lastName"
        meta.put("appleId", NSString(appleId))
        meta.put("userName", NSString(userName))
        val dsidNum = dsid.toLongOrNull() ?: 0L
        val purchaseDate = java.time.Instant.ofEpochSecond(nowMillis / 1000).toString()
        val accountInfo = NSDictionary().apply {
            put("AppleID", NSString(appleId))
            put("UserName", NSString(userName))
            put("AccountStoreFront", NSString(storeFront))
            put("DSPersonID", NSNumber(dsidNum))
            put("PurchaserID", NSNumber(dsidNum))
            put("DownloaderID", NSNumber(0L))
            put("FamilyID", NSNumber(0L))
            put("FirstName", NSString(firstName))
            put("LastName", NSString(lastName))
        }
        meta.put("com.apple.iTunesStore.downloadInfo", NSDictionary().apply {
            put("accountInfo", accountInfo)
            put("purchaseDate", NSString(purchaseDate))
        })
        meta.put("is-purchased-redownload", NSNumber(true))
        meta.put("purchaseDate", NSDate(Date((nowMillis / 1000) * 1000)))
        val sf = storeFront.substringBefore('-')
        meta.put(
            "storeCohort",
            NSString("10|date=${(nowMillis / 1000) * 1000}&sf=$sf&app=com.apple.iTunes&pgtp=Purchases&prpg=Purchases"),
        )
        return meta
    }

    /** Адреса из сумки; пустые — значения по умолчанию. */
    data class Bag(
        val authEndpoint: String = DEFAULT_AUTH_ENDPOINT,
        val redownloadEndpoint: String = DEFAULT_REDOWNLOAD_ENDPOINT,
        val updateEndpoint: String = DEFAULT_UPDATE_ENDPOINT,
        val volumeStoreDownloadEndpoint: String = "",
        val songDownloadDoneEndpoint: String = DEFAULT_SONG_DOWNLOAD_DONE_ENDPOINT,
    )

    /** Разбор bag.xml: подписанная форма {bag=<data>}, urlBag или плоский словарь. */
    fun parseBag(root: NSDictionary, parse: (ByteArray) -> NSDictionary): Bag {
        val d = (root.objectForKey("bag") as? NSData)?.let { parse(it.bytes()) } ?: root
        val ub = dict(d, "urlBag") ?: d
        fun pick(key: String, fallback: String) = scalar(ub, key).ifEmpty { bagFind(d, key) }.ifEmpty { fallback }
        return Bag(
            authEndpoint = pick("authenticateAccount", DEFAULT_AUTH_ENDPOINT),
            redownloadEndpoint = pick("redownloadProduct", DEFAULT_REDOWNLOAD_ENDPOINT),
            updateEndpoint = pick("updateProduct", DEFAULT_UPDATE_ENDPOINT),
            volumeStoreDownloadEndpoint = pick("volumeStoreDownloadProduct", ""),
            songDownloadDoneEndpoint = pick("songDownloadDone", DEFAULT_SONG_DOWNLOAD_DONE_ENDPOINT),
        )
    }

    private fun bagFind(d: NSDictionary, key: String): String {
        scalar(d, key).takeIf { it.isNotEmpty() }?.let { return it }
        for (k in d.allKeys()) {
            val v = d.objectForKey(k) as? NSDictionary ?: continue
            bagFind(v, key).takeIf { it.isNotEmpty() }?.let { return it }
        }
        return ""
    }

    /** Список версий из metadata элемента (list_versions). */
    data class Versions(val ids: List<String>, val latest: String)

    fun versionsFromItem(item: NSDictionary?): Versions {
        val meta = dict(item, "metadata") ?: throw StoreException("failed to get version identifiers from item metadata")
        val arr = meta.objectForKey("softwareVersionExternalIdentifiers") as? NSArray
            ?: throw StoreException("failed to get version identifiers from item metadata")
        val ids = arr.array.mapNotNull { v ->
            when (v) {
                is NSNumber -> v.longValue().toString()
                is NSString -> v.content
                else -> null
            }
        }
        val latest = scalar(meta, "softwareVersionExternalIdentifier")
        if (meta.objectForKey("softwareVersionExternalIdentifier") == null) {
            throw StoreException("failed to get latest version from item metadata")
        }
        return Versions(ids, latest)
    }

    /** Метаданные одной сборки (get_version_metadata), без минимальной iOS. */
    data class VersionMetadata(
        val externalVersionId: String,
        val displayVersion: String,
        val releaseDate: String,
        val bundleId: String,
        val minimumOSVersion: String? = null,
        val supportedPlatforms: List<String> = emptyList(),
    )

    fun versionMetadataFromItem(item: NSDictionary?, externalVersionId: String): VersionMetadata {
        val meta = dict(item, "metadata") ?: throw StoreException("failed to get metadata from item")
        return VersionMetadata(
            externalVersionId = scalar(meta, "softwareVersionExternalIdentifier").ifEmpty { externalVersionId },
            displayVersion = scalar(meta, "bundleShortVersionString"),
            releaseDate = scalar(meta, "releaseDate"),
            bundleId = scalar(meta, "softwareVersionBundleId"),
        )
    }

    fun md5Hex(file: File): String {
        val md = MessageDigest.getInstance("MD5")
        file.inputStream().buffered().use { input ->
            val buf = ByteArray(256 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n <= 0) break
                md.update(buf, 0, n)
            }
        }
        return md.digest().joinToString("") { "%02x".format(it) }
    }

    /** Разбор ответа authenticate: что делать дальше (do_login). */
    sealed class AuthDecision {
        object Success : AuthDecision()
        object NeedsCode : AuthDecision()
        object CodeRejected : AuthDecision()
        object AccountDisabled : AuthDecision()
        object InvalidCredentials : AuthDecision()
        data class Failed(val message: String) : AuthDecision()
    }

    fun decideAuth(status: Int, d: NSDictionary, codeSent: Boolean): AuthDecision {
        val failure = scalar(d, "failureType")
        val message = scalar(d, "customerMessage")
        val token = scalar(d, "passwordToken")
        val dsid = scalar(d, "dsPersonId").ifEmpty { scalar(dict(d, "accountInfo"), "dsPersonId") }
        return when {
            token.isNotEmpty() && dsid.isNotEmpty() && failure.isEmpty() -> AuthDecision.Success
            failure.isEmpty() && message == CUSTOMER_MSG_BAD_LOGIN ->
                if (codeSent) AuthDecision.CodeRejected else AuthDecision.NeedsCode
            failure.isEmpty() && message == CUSTOMER_MSG_ACCOUNT_DISABLED -> AuthDecision.AccountDisabled
            failure == FAILURE_INVALID_CREDENTIALS -> AuthDecision.InvalidCredentials
            failure.isNotEmpty() -> AuthDecision.Failed(message.ifEmpty { "something went wrong" })
            else -> AuthDecision.Failed("something went wrong (status=$status)")
        }
    }
}
