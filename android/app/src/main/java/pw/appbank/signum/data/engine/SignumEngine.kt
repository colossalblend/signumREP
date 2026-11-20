package pw.appbank.signum.data.engine

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.withContext
import kotlinx.coroutines.CancellationException
import pw.appbank.signum.data.api.Errors
import pw.appbank.signum.data.models.DEFAULT_API
import pw.appbank.signum.data.api.SignumApi
import pw.appbank.signum.data.apple.AnisetteClient
import pw.appbank.signum.data.apple.AppStoreClient
import pw.appbank.signum.data.apple.AppleAccount
import pw.appbank.signum.data.apple.GrandSlamClient
import pw.appbank.signum.data.apple.PurchaseHistoryClient
import pw.appbank.signum.data.api.DiagLog
import pw.appbank.signum.data.apple.BankOtp
import pw.appbank.signum.data.catalog.CatalogResolve
import pw.appbank.signum.data.device.DeviceService
import pw.appbank.signum.data.device.IpaInstaller
import pw.appbank.signum.data.jobs.JobQueue
import pw.appbank.signum.data.jobs.CatalogDownloadFiles
import pw.appbank.signum.data.jobs.QueuedWork
import pw.appbank.signum.data.models.AccountRestoreScan
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.Request
import pw.appbank.signum.data.api.Hx
import pw.appbank.signum.data.api.Http
import pw.appbank.signum.data.api.ProxyConfig
import pw.appbank.signum.data.models.AppSettings
import pw.appbank.signum.data.models.AppStoreSearchHit
import pw.appbank.signum.data.models.AppStoreVersion
import pw.appbank.signum.data.models.AuthStatus
import pw.appbank.signum.data.models.CatalogApp
import pw.appbank.signum.data.models.CatalogFeed
import pw.appbank.signum.data.models.CatalogInstallOffer
import pw.appbank.signum.data.models.CatalogVersion
import pw.appbank.signum.data.models.DeviceInfo
import pw.appbank.signum.data.models.InstalledApp
import pw.appbank.signum.data.models.LoginResult
import pw.appbank.signum.data.models.OtaUpdateInfo
import pw.appbank.signum.data.models.PurchaseHistoryResult
import pw.appbank.signum.data.models.SignOptions
import pw.appbank.signum.data.models.SigningStatus
import pw.appbank.signum.data.settings.SettingsStore
import pw.appbank.signum.data.signing.IpaTweaks
import java.io.File

class SignumEngine(context: Context) {
    private val app = context.applicationContext
    val settings = SettingsStore(app)
    private val api = SignumApi()
    private val gs = GrandSlamClient()
    val anisette = AnisetteClient(settings)
    val apple = AppleAccount(settings, anisette, gs)
    val store = AppStoreClient(settings, anisette, gs)
    val region = pw.appbank.signum.data.region.StoreRegionManager(settings)
    val history = PurchaseHistoryClient(settings)
    val devices = DeviceService(app, settings)
    private val installer = IpaInstaller(devices)

    private val _settings = MutableStateFlow(this.settings.load())
    val settingsFlow: StateFlow<AppSettings> = _settings

    val jobs = JobQueue { work, progress ->
        when (work) {
            is QueuedWork.CatalogInstall -> runCatalog(work, progress)
            is QueuedWork.LocalIpa -> runLocal(work, progress)
        }
    }

    init {
        DiagLog.note("engine", "Android Store client: ipatool protocol ${pw.appbank.signum.data.apple.StorePodRouting.UPSTREAM_VERSION} routing port; native SAP helper")
        store.restoreTokens()
        Errors.detailed = _settings.value.detailedErrors
        // Прокси применяем до первого запроса: иначе сохранённая настройка
        // подхватилась бы только после того, как человек сохранит её заново.
        if (ProxyConfig.apply(_settings.value)) {
            Http.rebuild()
        }
        if (!store.isReady() && store.hasSavedCredentials()) {
            Thread({
                runCatching { store.authenticate() }
            }, "signum-store-auth").apply {
                isDaemon = true
                start()
            }
        }
    }

    fun authStatus(): AuthStatus {
        val email = settings.activeEmail()
        val accounts = settings.accountInfos()
        return AuthStatus(
            authenticated = email != null &&
                (store.isReady() || settings.loadSessionBlob("store_$email") != null),
            email = email,
            accounts = accounts,
            historyPasswordSaved = email != null && !settings.loadPassword(email).isNullOrBlank(),
            historySessionSaved = email != null && !settings.loadHistoryCookies(email).isNullOrBlank(),
        )
    }

    suspend fun fetchCatalog(): CatalogFeed = withContext(Dispatchers.IO) { api.fetchCatalogFeed() }

    suspend fun fetchOta(): OtaUpdateInfo = withContext(Dispatchers.IO) { api.checkOta() }

    /**
     * Скачать обновление Signum и вернуть файл.
     *
     * Адрес приходит от сервера относительным (/releases/1.1.4.apk) — сводим
     * его с адресом API и проверяем узел: подменённая настройка не должна
     * уводить установку пакета на чужой сервер.
     */
    suspend fun downloadUpdate(
        info: OtaUpdateInfo,
        progress: (Float) -> Unit,
    ): java.io.File = withContext(Dispatchers.IO) {
        val raw = info.downloadUrl?.trim().orEmpty()
        if (raw.isEmpty()) error("Сервер не дал ссылку на обновление.")
        val base = DEFAULT_API.trimEnd('/').toHttpUrl()
        val url = base.resolve(raw) ?: error("Не удалось разобрать ссылку на обновление.")
        if (url.host != Hx.apiHost()) error("Обновление предлагают с чужого адреса — отменено.")

        val target = java.io.File(settings.updatesDir(), "Signum-${info.version ?: "new"}-${info.build ?: "new"}.apk")
        val client = Http.api.newBuilder()
            .callTimeout(0, java.util.concurrent.TimeUnit.SECONDS)
            .readTimeout(60, java.util.concurrent.TimeUnit.SECONDS)
            .retryOnConnectionFailure(true)
            .build()
        pw.appbank.signum.data.api.UpdateDownloader.download(client, url, target, info.sha256, progress)
        DiagLog.note("app", "обновление скачано: ${target.name}")
        target
    }

    suspend fun login(email: String, password: String, code: String?): LoginResult =
        withContext(Dispatchers.IO) {
            DiagLog.note("app", "вход в Apple ID: код ${if (code.isNullOrBlank()) "не введён" else "введён"}")
            store.login(email, password, code)
        }

    suspend fun resendCode(): LoginResult = withContext(Dispatchers.IO) { store.resendCode() }

    /**
     * Выйти из Apple ID.
     *
     * store.clear() очищает только то, что в памяти. Сессия лежит на диске, и
     * authStatus() считает человека вошедшим, пока есть active_email и блоб
     * session_<почта>. Ни то, ни другое здесь не убиралось — выход не работал
     * вовсе: экран сразу показывал прежний аккаунт.
     *
     * Сам аккаунт из списка не убираем: он остаётся, чтобы на него можно было
     * переключиться, просто уже без сохранённого входа.
     */
    fun logout() {
        val email = settings.activeEmail()
        history.clear()
        store.clear()
        apple.logout()
        email?.let {
            settings.deletePassword(it)
            settings.deleteHistoryPending(it)
            // "store_<почта>", а не просто почта: под этим ключом лежит
            // сессия App Store, её и проверяет authStatus().
            settings.deleteSessionBlob("store_$it")
            settings.deleteHistoryCookies(it)
        }
        settings.setActiveEmail(null)
    }

    /**
     * Переключиться на другой сохранённый Apple ID.
     *
     * Сессию в памяти сбрасываем и поднимаем заново: она принадлежит прежнему
     * аккаунту, и без сброса следующий запрос ушёл бы от его имени.
     */
    fun switchAccount(email: String) {
        val target = email.trim().lowercase()
        if (target.isEmpty() || target == settings.activeEmail()) return
        DiagLog.note("app", "смена Apple ID")
        history.clear()
        store.clear()
        settings.setActiveEmail(target)
        store.restoreTokens()
    }

    /** Убрать сохранённый Apple ID со всеми его данными. */
    fun removeAccount(email: String) {
        val target = email.trim().lowercase()
        if (target.isEmpty()) return
        DiagLog.note("app", "удаление Apple ID из списка")
        val wasActive = target == settings.activeEmail()
        settings.removeAccount(target)
        if (wasActive) {
            history.clear()
            store.clear()
            apple.logout()
            // removeAccount уже перевёл активным первый из оставшихся —
            // поднимаем его сессию, иначе человек остался бы «нигде».
            store.restoreTokens()
        }
    }

    suspend fun searchStore(query: String): List<AppStoreSearchHit> = coroutineScope {
        val requests = listOf<() -> List<AppStoreSearchHit>>(
            { store.search(query, "ru") }, { store.search(query, "us") }, { store.searchAppMagic(query) },
        ).map { fetch -> async(Dispatchers.IO) {
            try { fetch() } catch (e: CancellationException) { throw e }
            catch (_: Exception) { emptyList() }
        } }
        val hits = requests.flatMap { it.await() }.distinctBy { it.adamId }.take(100)
        val adam = store.adamIdFromQuery(query)
        if (adam == null || hits.any { it.adamId == adam }) return@coroutineScope hits
        val owner = settings.activeEmail()
            ?: error("Приложение не найдено в AppMagic и iTunes. Войдите в Apple ID, чтобы проверить покупки.")
        val owned = withContext(Dispatchers.IO) { store.listVersionIds(adam).isNotEmpty() }
        if (settings.activeEmail() != owner) error("Apple ID изменился. Повторите поиск.")
        if (!owned) return@coroutineScope hits
        listOf(AppStoreSearchHit(adamId = adam, name = "Приложение $adam", foundOnAccount = true)) + hits
    }

    // ── Альтернативный способ: временный Apple ID банка ─────────────────

    suspend fun bankSendSms(bankKey: String, phone: String): Pair<BankOtp.Session, String> =
        withContext(Dispatchers.IO) {
            val bank = BankOtp.Bank.parse(bankKey) ?: error("Неизвестный банк.")
            BankOtp.sendSms(bank, phone)
        }

    suspend fun bankValidate(session: BankOtp.Session, code: String): String =
        withContext(Dispatchers.IO) { BankOtp.validate(session, code) }

    suspend fun bankCredentials(session: BankOtp.Session): BankOtp.Credentials =
        withContext(Dispatchers.IO) { BankOtp.credentials(session) }

    suspend fun developerApps(artistId: String): List<AppStoreSearchHit> =
        withContext(Dispatchers.IO) { store.developerApps(artistId) }

    suspend fun listVersionIds(adamId: String, force: Boolean = false): List<String> =
        withContext(Dispatchers.IO) { store.listVersionIds(adamId, force) }

    suspend fun versionDisplay(adamId: String, externalVersionId: String): String =
        withContext(Dispatchers.IO) { store.versionDisplay(adamId, externalVersionId) }

    suspend fun listVersions(adamId: String, force: Boolean = false): List<AppStoreVersion> =
        withContext(Dispatchers.IO) { store.listVersions(adamId, force) }

    suspend fun fetchHistory(
        code: String?,
        loadMore: Boolean,
        batchId: String?,
        force: Boolean,
    ): PurchaseHistoryResult = withContext(Dispatchers.IO) {
        val email = settings.activeEmail() ?: error("Сначала войдите в Apple ID.")
        history.fetch(email, settings.loadPassword(email), code, loadMore, batchId, force)
    }

    fun cachedHistory(): PurchaseHistoryResult? {
        val email = settings.activeEmail() ?: return null
        return history.loadCached(email)
    }

    suspend fun listDevices(): List<DeviceInfo> = withContext(Dispatchers.IO) {
        val listed = devices.listDevices()
        if (listed.isNotEmpty()) return@withContext listed
        devices.attached().map {
            DeviceInfo(
                udid = it.serial ?: it.deviceName,
                deviceName = it.name,
                connection = "usb",
                paired = it.hasPermission,
            )
        }
    }

    suspend fun trustDevice(): String = withContext(Dispatchers.IO) { devices.trust() }

    suspend fun installedApps(): List<InstalledApp> =
        withContext(Dispatchers.IO) { devices.installedApps() }

    fun requestUsb(): String = devices.requestAccess()

    fun toggleFavorite(appId: String) {
        _settings.value = _settings.value.copy(favoriteAppIds = settings.toggleFavorite(appId))
    }

    fun saveSettings(next: AppSettings) {
        settings.save(next)
        _settings.value = next
        Errors.detailed = next.detailedErrors
        // Прокси нельзя подменить у готового клиента OkHttp, поэтому при смене
        // настройки клиентов пересобираем. Иначе человек нажал бы «Сохранить»,
        // а запросы продолжали бы идти напрямую.
        if (ProxyConfig.apply(next)) {
            Http.rebuild()
        }
    }

    fun selectDevice(udid: String) {
        saveSettings(_settings.value.copy(selectedDeviceUdid = udid))
    }

    suspend fun resolveCatalogInstall(app: CatalogApp, deviceIos: String?): CatalogInstallOffer =
        withContext(Dispatchers.IO) {
            val detail = runCatching { api.fetchCatalogAppDetail(app.id) }.getOrElse { error ->
                if (!app.app_store_id.isNullOrBlank()) {
                    return@withContext resolveOffer(
                        appId = app.id,
                        title = app.title,
                        icon = app.icon_url,
                        inStore = app.in_store == true,
                        version = app.version,
                        minIos = app.min_ios,
                        bundleId = app.bundle_id,
                        storeId = app.app_store_id,
                        deviceIos = deviceIos,
                        aliases = app.aliases.orEmpty(),
                    )
                }
                throw error
            }
            val versions = detail.versions
            val aliases = (app.aliases.orEmpty() + detail.aliases).distinct()
            if (versions.isEmpty()) {
                return@withContext resolveOffer(
                    appId = app.id,
                    title = detail.title,
                    icon = detail.iconUrl ?: app.icon_url,
                    inStore = app.in_store == true,
                    version = app.version,
                    minIos = app.min_ios,
                    bundleId = app.bundle_id,
                    storeId = app.app_store_id,
                    deviceIos = deviceIos,
                    aliases = aliases,
                )
            }
            val loggedIn = authStatus().authenticated
            val storeCompatible = CatalogResolve.pickPrimary(versions.filter { it.inStore && CatalogResolve.compatible(it, deviceIos) }, deviceIos)
            val storeAny = CatalogResolve.pickPrimary(versions.filter { it.inStore }, deviceIos)
            val restoreOnly = storeCompatible == null && storeAny == null
            val owned = if (loggedIn && restoreOnly) probeOwnedCatalogVersions(versions, false) else emptyList()
            val picked = when {
                storeCompatible != null -> storeCompatible
                storeAny != null -> storeAny
                owned.isNotEmpty() -> CatalogResolve.pickPrimary(owned, deviceIos)
                else -> CatalogResolve.pickPrimary(versions, deviceIos)
            }
            val base = CatalogInstallOffer(
                appId = app.id,
                appTitle = detail.title,
                title = picked?.title ?: detail.title,
                iconUrl = picked?.iconUrl ?: detail.iconUrl ?: app.icon_url,
                inStore = picked?.inStore == true,
                source = if (picked?.inStore == true) "app_store" else "restore",
                message = "",
                catalogVersions = versions,
                showAccountVersions = false,
                aliases = aliases,
                deviceIos = deviceIos,
            )
            if (picked == null) {
                return@withContext base.copy(message = "В каталоге нет версий для установки")
            }
            val offer = CatalogResolve.applyVersion(base, picked, deviceIos)
            when {
                picked.inStore && storeCompatible != null ->
                    offer.copy(message = "Доступна новая версия в App Store.")
                picked.inStore -> offer
                owned.any { it.identity == picked.identity } ->
                    offer.copy(message = "Версия найдена в покупках Apple ID.")
                else -> offer.copy(
                    message = "Версия из каталога. Если её нет в покупках Apple ID, установка не удастся.",
                )
            }
        }

    suspend fun probeAccountRestore(
        appId: String,
        appTitle: String,
        versions: List<CatalogVersion>,
        exclude: CatalogVersion?,
        deviceIos: String?,
        force: Boolean,
    ): AccountRestoreScan = withContext(Dispatchers.IO) {
        if (!authStatus().authenticated) {
            return@withContext AccountRestoreScan(
                deviceIos = deviceIos,
                message = "Сначала войдите в Apple ID на странице «Аккаунт».",
            )
        }
        val owned = probeOwnedCatalogVersions(versions, force)
        val installed = runCatching { devices.installedApps() }.getOrDefault(emptyList())
            .filter { !it.isPlaceholder }.associateBy { it.bundleId }
        val excludeId = exclude?.identity.orEmpty()
        val items = owned
            .filter { it.identity != excludeId }
            .mapNotNull { CatalogResolve.toRestoreItem(appId, appTitle, it, deviceIos, installed) }
            .sortedWith { a, b ->
                val cmp = CatalogResolve.compareIos(b.version, a.version)
                if (cmp != 0) cmp else a.title.compareTo(b.title, ignoreCase = true)
            }
        val message = when {
            items.isNotEmpty() -> null
            owned.isEmpty() ->
                "На этом Apple ID нет версий «$appTitle» в покупках.\n\n" +
                    "Показаны только приложения, которые уже покупались на аккаунте. " +
                    "Если покупали на другом Apple ID — войдите в него и нажмите «Обновить список»."
            else -> "Других версий в покупках нет — на аккаунте только текущая."
        }
        AccountRestoreScan(deviceIos = deviceIos, items = items, message = message)
    }

    private suspend fun probeOwnedCatalogVersions(
        versions: List<CatalogVersion>,
        force: Boolean,
    ): List<CatalogVersion> = coroutineScope {
        versions.map { version ->
            async(Dispatchers.IO) {
                if (!version.hasIdentity) return@async null
                val owned = runCatching {
                    store.ownsListing(version.appStoreId, version.bundleId, force)
                }.getOrElse { error ->
                    val msg = error.message.orEmpty()
                    if (msg.contains("password token", true) || msg.contains("устарела")) throw error
                    false
                }
                if (owned) version else null
            }
        }.awaitAll().filterNotNull()
    }

    fun applyCatalogVersion(
        offer: CatalogInstallOffer,
        version: CatalogVersion,
        deviceIos: String?,
    ): CatalogInstallOffer = CatalogResolve.applyVersion(offer, version, deviceIos)

    fun resolveOffer(
        appId: String,
        title: String,
        icon: String?,
        inStore: Boolean,
        version: String?,
        minIos: String?,
        bundleId: String?,
        storeId: String?,
        deviceIos: String?,
        aliases: List<String> = emptyList(),
    ): CatalogInstallOffer {
        val incompatible = !minIos.isNullOrBlank() && !deviceIos.isNullOrBlank() &&
            CatalogResolve.compareIos(deviceIos, minIos) < 0
        val msg = if (inStore) {
            "Версия доступна в App Store и будет привязана к Apple ID перед установкой."
        } else {
            "Этой версии уже нет в App Store. Установить можно только если она уже есть в покупках Apple ID."
        }
        val iosNote = if (incompatible) {
            " Телефон не поддерживает эту версию (нужна iOS $minIos, у вас $deviceIos)."
        } else ""
        return CatalogInstallOffer(
            appId = appId,
            appTitle = title,
            title = title,
            version = version,
            bundleId = bundleId,
            appStoreId = storeId,
            minIos = minIos,
            iconUrl = icon,
            inStore = inStore,
            source = if (inStore) "app_store" else "restore",
            message = msg + iosNote,
            aliases = aliases,
            deviceCompatible = if (incompatible) false else null,
            deviceIos = deviceIos,
        )
    }

    fun enqueueInstall(
        title: String,
        adamId: String?,
        bundleId: String?,
        inStore: Boolean,
        alreadyOwned: Boolean,
        version: String?,
        externalVersionId: String?,
        install: Boolean,
    ): String {
        return jobs.enqueueCatalog(
            title, adamId, bundleId, inStore, alreadyOwned, version, externalVersionId, install,
        )
    }

    fun enqueueLocal(path: String, sign: Boolean, install: Boolean): String {
        val name = File(path).name
        return jobs.enqueueLocal(path, name, sign, install)
    }

    fun cancelJob(id: String) = jobs.cancel(id)

    fun signingStatus(): SigningStatus {
        val auth = authStatus()
        return SigningStatus(
            authenticated = auth.authenticated,
            email = auth.email,
            certReady = auth.authenticated,
            message = if (auth.authenticated) "Apple ID готов" else "Войдите в Apple ID",
        )
    }

    fun cacheSize(): Long = settings.ipaCacheDir().walkTopDown().filter { it.isFile }.sumOf { it.length() }

    fun clearCache() {
        settings.ipaCacheDir().listFiles()?.forEach { it.deleteRecursively() }
    }

    fun cachedIpas() = settings.ipaCacheDir().listFiles()?.filter { it.extension.equals("ipa", true) }
        ?.map {
            val meta = runCatching { IpaTweaks.metadata(it) }.getOrNull()
            pw.appbank.signum.data.models.CachedIpa(
                filename = it.name,
                path = it.absolutePath,
                bundleId = meta?.bundleId,
                name = meta?.name,
                version = meta?.version,
                size = it.length(),
                modifiedAt = it.lastModified(),
            )
        }
        ?.sortedByDescending { it.modifiedAt }
        ?: emptyList()

    private fun runCatalog(
        work: QueuedWork.CatalogInstall,
        progress: (Float, String, String) -> Unit,
    ) {
        val adam = work.adamId?.trim().orEmpty()
        if (adam.isEmpty()) error("Не удалось определить приложение. Выберите другую версию.")
        if (settings.activeEmail() == null) error("Сначала войдите в Apple ID на странице «Аккаунт»")
        if (!work.install && !settings.canExportToDownloads()) {
            error("Выберите доступную папку скачивания в настройках.")
        }
        progress(5f, "auth", "Вход в App Store…")
        store.authenticate()
        val dest = CatalogDownloadFiles.output(settings.cacheDir(), work.jobId)
        store.purchaseAndDownload(
            adamId = adam,
            dest = dest,
            externalVersionId = work.externalVersionId,
            alreadyOwned = work.alreadyOwned || !work.inStore,
        ) { pct, msg -> progress(pct, "download", msg) }
        jobs.throwIfCancelled(work.jobId)
        if (!work.install) {
            // Файл от App Store склеен дозаписью: без пересборки iPhone откажется
            // ставить его по ссылке.
            progress(96f, "download", "Пересобираем файл…")
            IpaTweaks.repack(dest)
            jobs.throwIfCancelled(work.jobId)
            // Имя по названию и версии, а не по числовому adamId: файл уезжает
            // в папку пользователя, где «6763622616.ipa» ничего не значит.
            val pretty = listOfNotNull(
                work.title.trim().takeIf { it.isNotEmpty() },
                IpaTweaks.metadata(dest).version?.trim()?.takeIf { it.isNotEmpty() },
            ).joinToString(" ").ifBlank { adam }
            val saved = settings.exportToDownloads(dest, pretty)
            CatalogDownloadFiles.cleanup(dest)
            progress(
                100f,
                "done",
                "Сохранено: $saved",
            )
            return
        }
        progress(71f, "device", "Подключаем iPhone…")
        installer.install(dest) { pct, msg -> progress(pct, "install", msg) }
        CatalogDownloadFiles.cleanup(dest)
    }

    private fun runLocal(
        work: QueuedWork.LocalIpa,
        progress: (Float, String, String) -> Unit,
    ) {
        val src = File(work.path)
        if (!src.exists()) error("Файл IPA не найден")
        var ipa = src
        if (work.sign) {
            progress(10f, "sign", "Готовим файл…")
            val tweaked = File(settings.signedDir(), "signed-${src.name}")
            IpaTweaks.apply(src, tweaked, SignOptions())
            ipa = tweaked
            progress(40f, "sign", "Файл готов. Ставим на iPhone…")
        }
        if (work.install) {
            progress(55f, "install", "Установка…")
            installer.install(ipa) { pct, msg -> progress(pct, "install", msg) }
        }
    }

}
