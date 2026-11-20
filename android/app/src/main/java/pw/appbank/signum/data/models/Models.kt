package pw.appbank.signum.data.models

import com.appbank.signum.BuildConfig
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

const val APP_VERSION = BuildConfig.VERSION_NAME
const val APP_BUILD = BuildConfig.VERSION_CODE.toString()
val DEFAULT_API: String get() = pw.appbank.signum.data.api.Hx.apiBase()

@Serializable
data class CatalogFeed(
    val apps: List<CatalogApp> = emptyList(),
    val groups: List<CatalogGroup> = emptyList(),
    val banners: List<CatalogBanner> = emptyList(),
)

@Serializable
data class CatalogGroup(
    val id: String = "",
    val title: String = "",
    val priority: Int? = null,
)

@Serializable
data class CatalogBanner(
    val id: String = "",
    val title: String = "",
    val text: String? = null,
    val style: String? = null,
    val app_id: String? = null,
    val action_label: String? = null,
    val priority: Int? = null,
    val active: Boolean? = null,
)

@Serializable
data class CatalogApp(
    val id: String,
    val title: String,
    val icon_url: String? = null,
    val badge: String? = null,
    val version: String? = null,
    val min_ios: String? = null,
    val active: Boolean? = true,
    val in_store: Boolean? = null,
    val color: String? = null,
    val aliases: List<String>? = null,
    val category: String? = null,
    val activation: AppActivation? = null,
    val bundle_id: String? = null,
    val app_store_id: String? = null,
    /** Заполнено только у приложений, которые ставятся через TestFlight. */
    val testflight: TestFlightOffer? = null,
)

/**
 * Приглашение TestFlight.
 *
 * Такие приложения нельзя скачать как IPA: Apple раздаёт их только через
 * TestFlight, поэтому в каталоге у них нет ни версий, ни ссылки на файл.
 */
@Serializable
data class TestFlightOffer(
    val url: String,
)

@Serializable
data class AppActivation(
    val enabled: Boolean? = null,
    val title: String? = null,
    val message: String? = null,
    val provider: String? = null,
    val items: List<AppActivationItem> = emptyList(),
)

@Serializable
data class AppActivationItem(
    val id: String = "",
    val title: String = "",
    val subtitle: String? = null,
    val url: String? = null,
    val qr_payload: String? = null,
    val mode: String? = null,
    val hint: String? = null,
    val app_store_id: String? = null,
    val bundle_id: String? = null,
    val tb_id: String? = null,
)

@Serializable
data class AppStoreVersion(
    val version: String,
    @SerialName("externalVersionId") val externalVersionId: String,
    val isLatest: Boolean = false,
    val alreadyOwned: Boolean = false,
    /** Минимальная iOS именно этой сборки. null — ещё не выяснили. */
    val minIos: String? = null,
)

data class CatalogVersion(
    val title: String,
    val version: String,
    val bundleId: String? = null,
    val appStoreId: String? = null,
    val minIos: String? = null,
    val iconUrl: String? = null,
    val inStore: Boolean = false,
    val isLatest: Boolean = false,
    val description: String? = null,
) {
    val identity: String
        get() = when {
            !appStoreId.isNullOrBlank() -> "store:$appStoreId"
            !bundleId.isNullOrBlank() -> "bundle:$bundleId"
            version.isNotBlank() -> "version:$version"
            else -> title
        }

    val hasIdentity: Boolean
        get() = !appStoreId.isNullOrBlank() || (!bundleId.isNullOrBlank() && bundleId != "NONE")
}

data class CatalogAppDetail(
    val id: String,
    val title: String,
    val iconUrl: String? = null,
    val inStore: Boolean = false,
    val versions: List<CatalogVersion> = emptyList(),
    val aliases: List<String> = emptyList(),
)

data class CatalogSection(
    val id: String,
    val title: String,
    val apps: List<CatalogApp>,
)

data class AccountRestoreVersion(
    val appId: String,
    val appTitle: String,
    val title: String,
    val description: String? = null,
    val version: String,
    val bundleId: String,
    val appStoreId: String? = null,
    val minIos: String? = null,
    val iconUrl: String? = null,
    val inStore: Boolean,
    val deviceCompatible: Boolean? = null,
    val installedOnDevice: Boolean = false,
    val installedVersion: String? = null,
)

data class AccountRestoreScan(
    val deviceIos: String? = null,
    val items: List<AccountRestoreVersion> = emptyList(),
    val message: String? = null,
)

data class CatalogInstallOffer(
    val appId: String,
    val appTitle: String,
    val title: String,
    val description: String? = null,
    val version: String? = null,
    val bundleId: String? = null,
    val appStoreId: String? = null,
    val minIos: String? = null,
    val iconUrl: String? = null,
    val inStore: Boolean = true,
    val source: String = "app_store",
    val message: String = "",
    val showAccountVersions: Boolean = false,
    val catalogVersions: List<CatalogVersion> = emptyList(),
    val aliases: List<String> = emptyList(),
    val installedOnDevice: Boolean = false,
    val installedVersion: String? = null,
    val deviceCompatible: Boolean? = null,
    val deviceIos: String? = null,
)

data class DeviceInfo(
    val udid: String,
    val productType: String? = null,
    val productVersion: String? = null,
    val deviceName: String? = null,
    val battery: Int? = null,
    val connection: String = "usb",
    val paired: Boolean = false,
)

data class InstalledApp(
    val bundleId: String,
    val name: String? = null,
    val version: String? = null,
    val appStoreId: String? = null,
    val isPlaceholder: Boolean = false,
)

enum class JobStatus { queued, running, done, failed, cancelled }

enum class JobKind { install, export, sign }

data class InstallJob(
    val id: String,
    val title: String,
    val subtitle: String,
    val phase: String,
    val percent: Float,
    val message: String,
    val status: JobStatus,
    val error: String? = null,
    val createdAt: Long,
    val kind: JobKind = JobKind.install,
    val retryable: Boolean = false,
)

data class ActionHistoryEntry(
    val id: String,
    val accountEmail: String? = null,
    val deviceUdid: String? = null,
    val title: String,
    val subtitle: String,
    val status: String,
    val kind: String,
    val finishedAt: Long,
)

enum class SessionState { active, saved, expired, needs_2fa, damaged, not_saved }

data class AppleAccountInfo(
    val email: String,
    val active: Boolean,
    val sessionSaved: Boolean,
    val sessionState: SessionState,
    val addedAt: Long,
)

data class AuthStatus(
    val authenticated: Boolean,
    val email: String? = null,
    val accounts: List<AppleAccountInfo> = emptyList(),
    val historyPasswordSaved: Boolean = false,
    val historySessionSaved: Boolean = false,
)

data class LoginResult(
    val email: String,
    val needs2fa: Boolean = false,
    val historyNeeds2fa: Boolean = false,
    val historyReady: Boolean = false,
    val historyError: String? = null,
    val message: String? = null,
)

data class PurchaseHistoryApp(
    val adamId: String,
    val name: String,
    val artist: String? = null,
    val artwork: String? = null,
    val price: String? = null,
    val date: String? = null,
    val bundleId: String? = null,
)

data class PurchaseHistoryResult(
    val email: String,
    val count: Int,
    val apps: List<PurchaseHistoryApp>,
    val message: String? = null,
    val needs2fa: Boolean = false,
    val hasMore: Boolean = false,
    val nextBatchId: String? = null,
    val sessionSaved: Boolean = false,
    val fromCache: Boolean = false,
)

/** Разработчик из App Store. */
data class StoreDeveloper(
    val artistId: String,
    val name: String,
    val appCount: Int = 0,
)

data class AppStoreSearchHit(
    val adamId: String,
    val name: String,
    val artist: String? = null,
    val artistId: String? = null,
    val artwork: String? = null,
    val bundleId: String? = null,
    val version: String? = null,
    val minIos: String? = null,
    val price: String? = null,
    val foundOnAccount: Boolean = false,
)

data class SignOptions(
    val lowerMinIos: Boolean = false,
    val minIosVersion: String? = "12.0",
    val fileSharing: Boolean = false,
    val removeDeviceLimits: Boolean = false,
    val changeAppName: Boolean = false,
    val appName: String? = null,
    val changeVersion: Boolean = false,
    val appVersion: String? = null,
    val autoBundleId: Boolean = true,
    val customBundleId: String? = null,
    val forceUniqueBundle: Boolean = false,
    val stripPlugins: Boolean = false,
    val stripWatch: Boolean = true,
    val clearUrlSchemes: Boolean = false,
    val documentsInPlace: Boolean = false,
    val supportsOpeningDocs: Boolean = false,
    val allowHttp: Boolean = false,
    val removeOrientationLock: Boolean = false,
    val stripFrameworks: Boolean = false,
)

data class SigningStatus(
    val authenticated: Boolean,
    val email: String? = null,
    val teamId: String? = null,
    val teamName: String? = null,
    val teamKind: String? = null,
    val certReady: Boolean = false,
    val message: String? = null,
)

data class AppSettings(
    val theme: String = "dark",
    val autoUpdate: Boolean = true,
    val liteDesign: Boolean = false,
    val detailedErrors: Boolean = false,
    val liteStaticLoaders: Boolean = true,
    val liteReduceMotion: Boolean = true,
    val liteSimpleScrolling: Boolean = true,
    val liteDisableBlur: Boolean = true,
    val liteDisableShadows: Boolean = true,
    val liteFlatBackgrounds: Boolean = true,
    val selectedDeviceUdid: String? = null,
    val downloadsDir: String = "",
    val exportsDir: String = "",
    /** Приложения, закреплённые пользователем наверху каталога. */
    val favoriteAppIds: List<String> = emptyList(),
    val ipatoolEngine: String = "auto",
    /** Прокси для обращений к Apple. Нужен там, где Apple недоступна напрямую. */
    val proxyEnabled: Boolean = false,
    /** socks5 | socks4 | http */
    val proxyKind: String = "socks5",
    val proxyHost: String = "",
    val proxyPort: Int = 0,
    val proxyUser: String = "",
    val proxyPassword: String = "",
)

data class OtaUpdateInfo(
    val hasUpdate: Boolean = false,
    val version: String? = null,
    val build: String? = null,
    val downloadUrl: String? = null,
    val notes: String? = null,
    val mandatory: Boolean = false,
    // Обновление — это файл, который мы сами предложим установить. Сверять
    // его с тем, что обещал сервер, обязательно.
    val sha256: String? = null,
)

data class CachedIpa(
    val filename: String,
    val path: String,
    val bundleId: String? = null,
    val name: String? = null,
    val version: String? = null,
    val size: Long,
    val modifiedAt: Long,
)

data class IpaMetadata(
    val bundleId: String,
    val name: String,
    val version: String? = null,
    val minIos: String? = null,
)

@Serializable
data class LooseJson(val raw: JsonElement? = null)
