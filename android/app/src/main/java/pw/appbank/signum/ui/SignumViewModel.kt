package pw.appbank.signum.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.coroutines.launch
import pw.appbank.signum.data.api.DiagLog
import pw.appbank.signum.data.api.Errors
import pw.appbank.signum.data.catalog.CatalogResolve
import pw.appbank.signum.data.catalog.CatalogSearch
import pw.appbank.signum.data.catalog.StoreSearch
import pw.appbank.signum.data.apple.BankOtp
import pw.appbank.signum.data.engine.SignumEngine
import pw.appbank.signum.data.models.AccountRestoreScan
import pw.appbank.signum.data.models.AccountRestoreVersion
import pw.appbank.signum.data.models.AppSettings
import pw.appbank.signum.data.models.AppStoreSearchHit
import pw.appbank.signum.data.models.StoreDeveloper
import pw.appbank.signum.data.models.AppStoreVersion
import pw.appbank.signum.data.models.AuthStatus
import pw.appbank.signum.data.models.OtaUpdateInfo
import pw.appbank.signum.data.models.CatalogApp
import pw.appbank.signum.data.models.CatalogBanner
import pw.appbank.signum.data.models.CatalogGroup
import pw.appbank.signum.data.models.CatalogInstallOffer
import pw.appbank.signum.data.models.CatalogSection
import pw.appbank.signum.data.models.CatalogVersion
import pw.appbank.signum.data.models.DeviceInfo
import pw.appbank.signum.data.models.InstalledApp
import pw.appbank.signum.data.models.JobStatus
import pw.appbank.signum.data.models.PurchaseHistoryApp
import pw.appbank.signum.data.models.HISTORY_SOURCE_LIBRARY
import pw.appbank.signum.data.models.HISTORY_SOURCE_REPORTPROBLEM
import pw.appbank.signum.data.models.SignOptions

data class UiState(
    val settings: AppSettings = AppSettings(),
    val catalog: List<CatalogApp> = emptyList(),
    val groups: List<CatalogGroup> = emptyList(),
    val banners: List<CatalogBanner> = emptyList(),
    val catalogLoading: Boolean = false,
    val catalogError: String = "",
    val search: String = "",
    /** Где искать: `catalog` — каталог Signum (в памяти), `store` — App Store (AppMagic + iTunes). */
    val catalogScope: String = "catalog",
    val storeHits: List<AppStoreSearchHit> = emptyList(),
    val storeSearching: Boolean = false,
    val storeSearchError: String = "",
    val groupFilter: String = "all",
    val offer: CatalogInstallOffer? = null,
    val offerLoading: Boolean = false,
    val offerError: String = "",
    val versions: List<AppStoreVersion> = emptyList(),
    val versionsLoading: Boolean = false,
    val versionsError: String = "",
    val versionsSearch: String = "",
    val versionsVisibleCount: Int = 12,
    val chosenVersion: AppStoreVersion? = null,
    val versionFitBusy: Boolean = false,
    val selectMode: Boolean = false,
    val selectedAppIds: Set<String> = emptySet(),
    val bulkBusy: Boolean = false,
    val regionNotice: String = "",
    val regionCode: String = "",
    val regionName: String = "",
    val regionBusy: Boolean = false,
    val regionMessage: String = "",
    val regionPendingRestore: Boolean = false,
    val restoreScan: AccountRestoreScan? = null,
    val restoreOpen: Boolean = false,
    val restoreLoading: Boolean = false,
    val restoreSearch: String = "",
    val restoreVisibleCount: Int = 12,
    val restoreScans: Map<String, AccountRestoreScan> = emptyMap(),
    val bankAltOpen: Boolean = false,
    val auth: AuthStatus = AuthStatus(false),
    val loginEmail: String = "",
    val loginPassword: String = "",
    val loginCode: String = "",
    val loginStep: String = "credentials",
    val loginBusy: Boolean = false,
    val loginMessage: String = "",
    val history: List<PurchaseHistoryApp> = emptyList(),
    val historySearch: String = "",
    val historyLoading: Boolean = false,
    val historyNeeds2fa: Boolean = false,
    val history2fa: String = "",
    val historyMessage: String = "",
    val historySelected: Set<String> = emptySet(),
    val historyBusy: Boolean = false,
    /** `reportproblem` или `library` (без кода, список может быть неполным). */
    val historySource: String = HISTORY_SOURCE_REPORTPROBLEM,
    val historyLibraryLoading: Boolean = false,
    val devices: List<DeviceInfo> = emptyList(),
    val selectedDevice: DeviceInfo? = null,
    val installed: List<InstalledApp> = emptyList(),
    /** Открытая карточка TestFlight-приложения; null — карточка закрыта. */
    val testFlightApp: CatalogApp? = null,
    val testFlightUrl: String = "",
    val deviceMessage: String = "",
    // Альтернативный способ: шаг потока — phone, code, done.
    val bankAltStep: String = "phone",
    val bankAltPhone: String = "",
    val bankAltCode: String = "",
    val bankAltBusy: Boolean = false,
    val bankAltMessage: String = "",
    val bankAltLogin: String = "",
    val bankAltPassword: String = "",
    val bankAltLink: String = "",
    val devHits: List<StoreDeveloper> = emptyList(),
    val openedDeveloper: StoreDeveloper? = null,
    val developerApps: List<AppStoreSearchHit> = emptyList(),
    val developerLoading: Boolean = false,
    val developerError: String = "",
    /** Отмеченные для пакетной загрузки приложения разработчика (App Store ID). */
    val developerPicked: Set<String> = emptySet(),
    val developerBatchBusy: Boolean = false,
    /** Поиск разработчиков идёт параллельно поиску приложений — свой флаг. */
    val devSearching: Boolean = false,
    /** Сборки, для которых ещё выясняем минимальную iOS (рисуем «iOS…»). */
    val versionsIosPending: Set<String> = emptySet(),
    val ota: OtaUpdateInfo? = null,
    val otaBusy: Boolean = false,
    val otaPercent: Float = 0f,
    val otaMessage: String = "",
    val otaFile: String = "",
    val diagRecording: Boolean = false,
    val diagAuto: Boolean = true,
    val diagBusy: Boolean = false,
    val diagMessage: String = "",
    val devicesLoading: Boolean = false,
    val signPath: String = "",
    val signOptions: SignOptions = SignOptions(),
    val signMessage: String = "",
    // ── Экраны аккаунта, устройств, файла и настроек ──
    /** Сведения о выбранном IPA: имя, bundle id, версия, минимальная iOS. */
    val signMeta: pw.appbank.signum.data.models.IpaMetadata? = null,
    val signBusy: Boolean = false,
    val historyHasMore: Boolean = false,
    val historyNextBatchId: String? = null,
    val historyLoadingMore: Boolean = false,
    /** Форма входа поверх уже вошедшего аккаунта: «Добавить Apple ID» и «Войти снова». */
    val addingAccount: Boolean = false,
    val installedLoading: Boolean = false,
    val otaChecking: Boolean = false,
    val otaCheckMessage: String = "",
    val cacheBytes: Long = -1L,
    /** Окно с QR активации: из карточки приложения или сразу после установки. */
    val activation: ActivationView? = null,
    /** Сервер не принимает эту версию Signum — экран закрыт окном обновления. */
    val clientOutdated: pw.appbank.signum.data.api.ClientOutdated? = null,
)

/** QR активации, готовый к показу: живые коды tb.php уже подставлены. */
data class ActivationView(
    val appTitle: String,
    val source: pw.appbank.signum.data.models.AppActivation,
    val items: List<ActivationItemView>,
    val loading: Boolean = false,
    val error: String = "",
)

data class ActivationItemView(
    val item: pw.appbank.signum.data.models.AppActivationItem,
    val payload: String,
    val hint: String,
    /** Когда код перестанет действовать (мс), если сервер дал срок. */
    val expiresAt: Long? = null,
) {
    val inApp: Boolean get() = item.mode == "in_app_scan"
    val live: Boolean get() = !item.tb_id.isNullOrBlank()
}

class SignumViewModel(val engine: SignumEngine) : ViewModel() {
    private val _state = MutableStateFlow(
        // Запись переживает перезапуск приложения, поэтому её состояние берём
        // у самого журнала, а не считаем выключенным по умолчанию.
        UiState(
            settings = engine.settingsFlow.value,
            diagRecording = DiagLog.isActive(),
            diagAuto = DiagLog.autoEnabled(),
        ),
    )
    val state: StateFlow<UiState> = _state

    init {
        refreshAuth()
        loadCatalog()
        engine.devices.onUsbChange = { refreshDevices() }
        viewModelScope.launch {
            engine.settingsFlow.collect { s -> _state.update { it.copy(settings = s) } }
        }
        checkOta()
        recoverRegionAfterCrash()
        restoreRegionWhenQueueIdle()
        watchClientOutdated()
        showActivationAfterInstall()
    }

    // ── Устаревший клиент ──────────────────────────────────────────────

    private fun watchClientOutdated() {
        viewModelScope.launch {
            pw.appbank.signum.data.api.SignumApi.clientOutdated.collect { info ->
                if (info != null) _state.update { it.copy(clientOutdated = info) }
            }
        }
    }

    // ── Активация после установки ──────────────────────────────────────
    //
    // Некоторым банкам после установки нужна активация по QR. Как на ПК:
    // если у выбранной версии есть свой QR, показываем его, как только дело
    // в очереди завершилось.

    private val pendingActivations = java.util.concurrent.ConcurrentHashMap<String, Pair<String, pw.appbank.signum.data.models.AppActivation>>()

    private fun showActivationAfterInstall() {
        viewModelScope.launch {
            engine.jobs.jobsFlow.collect { jobs ->
                if (pendingActivations.isEmpty()) return@collect
                for (job in jobs) {
                    when (job.status) {
                        JobStatus.done -> {
                            val (title, activation) = pendingActivations.remove(job.id) ?: continue
                            openActivation(title, activation, force = true)
                        }
                        JobStatus.failed, JobStatus.cancelled -> pendingActivations.remove(job.id)
                        else -> Unit
                    }
                }
            }
        }
    }

    /** Активация из карточки каталога, без пустых пунктов. */
    fun activationFor(appId: String?): pw.appbank.signum.data.models.AppActivation? {
        if (appId.isNullOrBlank()) return null
        val raw = _state.value.catalog.firstOrNull { it.id == appId }?.activation ?: return null
        if (raw.enabled == false) return null
        val items = raw.items.filter {
            !it.qr_payload.isNullOrBlank() || !it.url.isNullOrBlank() || !it.tb_id.isNullOrBlank()
        }
        return if (items.isEmpty()) null else raw.copy(items = items)
    }

    /** Пункт активации именно для ставящейся версии: по App Store ID, bundle ID или названию. */
    private fun activationForInstalled(
        full: pw.appbank.signum.data.models.AppActivation,
        appStoreId: String?,
        bundleId: String?,
        title: String?,
    ): pw.appbank.signum.data.models.AppActivation? {
        val store = appStoreId?.trim().orEmpty()
        val bundle = bundleId?.trim().orEmpty()
        val name = title?.trim()?.lowercase().orEmpty()
        val item = full.items.firstOrNull { store.isNotEmpty() && it.app_store_id?.trim() == store }
            ?: full.items.firstOrNull { bundle.isNotEmpty() && bundle != "NONE" && it.bundle_id?.trim() == bundle }
            ?: full.items.firstOrNull { name.isNotEmpty() && it.title.trim().lowercase() == name }
            ?: return null
        return pw.appbank.signum.data.models.AppActivation(
            enabled = true,
            provider = full.provider,
            title = full.title?.takeIf { it.isNotBlank() } ?: "Активация после установки",
            message = item.hint?.takeIf { it.isNotBlank() } ?: full.message
                ?: "Для входа в банк нужно активировать установленное приложение.",
            items = listOf(item),
        )
    }

    fun openActivation(appTitle: String, activation: pw.appbank.signum.data.models.AppActivation, force: Boolean = false) {
        _state.update {
            it.copy(
                activation = ActivationView(
                    appTitle = appTitle,
                    source = activation,
                    items = activation.items.map(::staticActivationItem),
                    loading = activation.items.any { item -> !item.tb_id.isNullOrBlank() },
                ),
            )
        }
        refreshActivation(force)
    }

    fun closeActivation() = _state.update { it.copy(activation = null) }

    private var activationRequest = 0

    /** Подставить живые коды tb.php. force — попросить у сервера новый код. */
    fun refreshActivation(force: Boolean) {
        val current = _state.value.activation ?: return
        if (current.source.items.none { !it.tb_id.isNullOrBlank() }) return
        val request = ++activationRequest
        _state.update { st -> st.copy(activation = st.activation?.copy(loading = true, error = "")) }
        viewModelScope.launch {
            val items = coroutineScope {
                current.source.items.map { item ->
                    async {
                        val tb = item.tb_id?.trim().orEmpty()
                        if (tb.isEmpty()) return@async staticActivationItem(item)
                        runCatching { engine.fetchTbActivation(tb, force) }.fold(
                            { live ->
                                ActivationItemView(
                                    item = item,
                                    payload = live.payload,
                                    hint = item.hint?.takeIf { it.isNotBlank() } ?: live.hint?.takeIf { it.isNotBlank() }
                                        ?: "Отсканируйте QR камерой iPhone. Код действует 10 минут, затем обновляется.",
                                    // Без срока (0) код считаем бессрочным, иначе окно обновляло бы его без конца.
                                    expiresAt = if (live.expiresInSec > 0) System.currentTimeMillis() + live.expiresInSec * 1000L else null,
                                )
                            },
                            { e ->
                                val base = staticActivationItem(item)
                                base.copy(
                                    payload = if (force) "" else base.payload,
                                    hint = "Не удалось сгенерировать QR: " + Errors.localize(e.message ?: "ошибка сети"),
                                )
                            },
                        )
                    }
                }.awaitAll()
            }
            if (request != activationRequest) return@launch
            _state.update { st ->
                val view = st.activation ?: return@update st
                if (view.source != current.source) return@update st
                st.copy(activation = view.copy(items = items, loading = false))
            }
        }
    }

    private fun staticActivationItem(item: pw.appbank.signum.data.models.AppActivationItem): ActivationItemView {
        val payload = (item.qr_payload ?: item.url ?: "").trim()
        val hint = item.hint?.trim()?.takeIf { it.isNotEmpty() } ?: if (item.mode == "in_app_scan") {
            "Откройте приложение и отсканируйте QR внутри него (не через системную камеру iPhone)."
        } else {
            "Отсканируйте QR камерой iPhone — откроется приложение."
        }
        return ActivationItemView(item = item, payload = payload, hint = hint)
    }

    /**
     * Возвращает страну обратно, как только очередь опустела.
     *
     * Установка идёт асинхронно, поэтому вернуть страну сразу после постановки
     * в очередь нельзя — файл ещё не скачан. Ждём, пока не останется активных
     * дел, и только тогда переезжаем назад.
     */
    private fun restoreRegionWhenQueueIdle() {
        viewModelScope.launch {
            engine.jobs.jobsFlow.collect { jobs ->
                val busy = jobs.any { it.status == JobStatus.queued || it.status == JobStatus.running }
                if (busy || !_state.value.regionPendingRestore) return@collect
                val email = engine.settings.activeEmail() ?: return@collect
                val status = coroutineScope {
                    async(Dispatchers.IO) {
                        runCatching { engine.region.restoreOriginal(email) }.getOrNull()
                    }.await()
                }
                _state.update {
                    it.copy(
                        regionPendingRestore = engine.region.pendingRestore(email) != null,
                        regionCode = status?.code ?: it.regionCode,
                        regionName = status?.name ?: it.regionName,
                        regionNotice = status?.let { s -> "Страна магазина возвращена: ${s.name}." }
                            ?: it.regionNotice,
                    )
                }
            }
        }
    }

    /**
     * Смена страны магазина могла оборваться: Android легко убивает процесс в
     * фоне, и тогда человек остаётся с аккаунтом в чужой стране, ничего об
     * этом не зная. Проверяем при каждом запуске и возвращаем исходную.
     */
    private fun recoverRegionAfterCrash() {
        viewModelScope.launch {
            val notes = coroutineScope {
                async(Dispatchers.IO) {
                    runCatching { engine.region.recoverInterruptedRegionChange() }
                        .getOrDefault(emptyList())
                }.await()
            }
            if (notes.isNotEmpty()) {
                _state.update { it.copy(regionNotice = notes.joinToString("\n\n")) }
            }
        }
    }

    fun dismissRegionNotice() = _state.update { it.copy(regionNotice = "") }

    override fun onCleared() {
        engine.devices.onUsbChange = null
        super.onCleared()
    }

    fun loadCatalog() {
        viewModelScope.launch {
            _state.update { it.copy(catalogLoading = true, catalogError = "") }
            runCatching { engine.fetchCatalog() }
                .onSuccess { feed ->
                    _state.update {
                        it.copy(
                            catalog = feed.apps,
                            groups = feed.groups,
                            banners = feed.banners,
                            catalogLoading = false,
                        )
                    }
                }
                .onFailure { e ->
                    _state.update { it.copy(catalogLoading = false, catalogError = Errors.localize(e.message ?: "")) }
                }
        }
    }

    /**
     * Задача поиска по App Store. Держим её, чтобы отменять предыдущую:
     * иначе каждое нажатие клавиши оставляло висеть свой запрос, и на слово
     * из восьми букв уходило восемь обращений к Apple, чьи ответы приходили
     * вразнобой.
     */
    private var searchJob: Job? = null

    fun setSearch(q: String) {
        val storeScope = q.trim().length >= 2
        searchJob?.cancel()
        _state.update {
            it.copy(
                search = q,
                storeHits = emptyList(),
                devHits = emptyList(),
                storeSearchError = "",
                storeSearching = storeScope,
                devSearching = storeScope,
            )
        }
        if (!storeScope) return
        searchJob = viewModelScope.launch {
            // Пауза после последнего нажатия: пока человек печатает, в сеть
            // не ходим вовсе.
            delay(300)
            // Разработчиков ищем тем же вводом и параллельно, как на ПК:
            // приложения показываем выше, студии — ниже, и если студий не
            // нашлось, раздела просто нет.
            launch {
                val devs = try {
                    StoreSearch.searchDevelopers(q)
                } catch (e: kotlinx.coroutines.CancellationException) {
                    throw e
                } catch (_: Exception) {
                    emptyList()
                }
                if (_state.value.search == q) _state.update { it.copy(devHits = devs, devSearching = false) }
            }
            val hits = try {
                StoreSearch.searchApps(q, ownedOnAccount = ::ownedOnActiveAccount)
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                if (_state.value.search == q) {
                    _state.update {
                        it.copy(
                            storeSearching = false,
                            storeSearchError = Errors.localize(e.message ?: "Не удалось выполнить поиск."),
                        )
                    }
                }
                return@launch
            }
            if (_state.value.search != q) return@launch
            _state.update { it.copy(storeHits = hits, storeSearching = false) }
            val withIcons = runCatching { StoreSearch.fillIosIcons(hits) }.getOrNull() ?: return@launch
            if (withIcons !== hits && _state.value.search == q) _state.update { it.copy(storeHits = withIcons) }
        }
    }

    /**
     * Есть ли номер приложения в покупках текущего Apple ID. Спрашиваем, только
     * когда введён номер, которого не знает ни один магазин: приложение могли
     * снять с продажи, а в покупках оно осталось.
     */
    private suspend fun ownedOnActiveAccount(adamId: String): Boolean {
        val owner = engine.settings.activeEmail() ?: return false
        val owned = runCatching { engine.listVersionIds(adamId).isNotEmpty() }.getOrDefault(false)
        return owned && engine.settings.activeEmail() == owner
    }

    /** Открыть разработчика и показать всё, что у него есть в магазине. */
    fun openDeveloper(dev: StoreDeveloper) {
        _state.update {
            it.copy(
                openedDeveloper = dev,
                developerApps = emptyList(),
                developerLoading = true,
                developerError = "",
                developerPicked = emptySet(),
            )
        }
        viewModelScope.launch {
            val result = runCatching { StoreSearch.developerApps(dev.artistId) }
            if (_state.value.openedDeveloper?.artistId != dev.artistId) return@launch
            _state.update {
                it.copy(
                    developerApps = result.getOrDefault(emptyList()),
                    developerLoading = false,
                    developerError = result.exceptionOrNull()
                        ?.let { e -> Errors.localize(e.message ?: "Не удалось загрузить приложения разработчика.") }
                        .orEmpty(),
                )
            }
        }
    }

    fun closeDeveloper() = _state.update {
        it.copy(
            openedDeveloper = null,
            developerApps = emptyList(),
            developerLoading = false,
            developerError = "",
            developerPicked = emptySet(),
        )
    }

    fun toggleDeveloperPick(adamId: String) = _state.update { state ->
        val next = state.developerPicked.toMutableSet()
        if (!next.remove(adamId)) next.add(adamId)
        state.copy(developerPicked = next)
    }

    /** «Выбрать все» / «Снять все». */
    fun toggleDeveloperPickAll() = _state.update { state ->
        val all = state.developerApps.mapTo(HashSet()) { it.adamId }
        state.copy(developerPicked = if (state.developerPicked.size == all.size) emptySet() else all)
    }

    /**
     * Скачать отмеченные приложения разработчика разом.
     *
     * Ставим по одному и не прерываемся на ошибке: если одно не встало в
     * очередь, остальные всё равно должны уехать — иначе непонятно, что
     * скачалось, а что нет.
     */
    fun queueDeveloperPicked(onDone: () -> Unit) {
        val state = _state.value
        if (state.developerPicked.isEmpty() || state.developerBatchBusy) return
        if (!engine.authStatus().authenticated) {
            _state.update { it.copy(developerError = "Сначала войдите в Apple ID на странице «Аккаунт».") }
            return
        }
        val chosen = state.developerApps.filter { it.adamId in state.developerPicked }
        DiagLog.note("ui", "нажал «Скачать отмеченные» — ${chosen.size} шт · ${state.openedDeveloper?.name.orEmpty()}")
        _state.update { it.copy(developerBatchBusy = true, developerError = "") }
        val failed = mutableListOf<String>()
        for (hit in chosen) {
            runCatching {
                engine.enqueueInstall(
                    title = hit.name,
                    adamId = hit.adamId,
                    bundleId = hit.bundleId,
                    inStore = !hit.removedFromStore,
                    alreadyOwned = hit.removedFromStore,
                    version = null,
                    externalVersionId = null,
                    install = false,
                )
            }.onFailure { failed += hit.name }
        }
        if (failed.size == chosen.size) {
            _state.update {
                it.copy(developerBatchBusy = false, developerError = "Не удалось поставить в очередь ни одно приложение.")
            }
            return
        }
        _state.update {
            it.copy(
                developerBatchBusy = false,
                developerPicked = emptySet(),
                developerError = if (failed.isEmpty()) "" else
                    "Не удалось поставить в очередь: ${failed.joinToString(", ")}. Остальные скачиваются.",
            )
        }
        if (failed.isEmpty()) {
            closeDeveloper()
            onDone()
        }
    }

    fun setGroup(id: String) = _state.update { it.copy(groupFilter = id) }

    fun visibleApps(): List<CatalogApp> {
        val s = _state.value
        val byGroup = if (s.groupFilter == "all") s.catalog else s.catalog.filter { (it.category ?: "app") == s.groupFilter }
        return CatalogSearch.search(byGroup, s.search)
    }

    fun deviceIos(): String? = _state.value.selectedDevice?.productVersion

    fun catalogSections(): List<CatalogSection>? {
        val s = _state.value
        if (s.groupFilter != "all" || s.search.isNotBlank()) return null
        return CatalogResolve.sections(s.catalog, s.groups)
    }

    fun closeTestFlight() {
        _state.update { it.copy(testFlightApp = null, testFlightUrl = "") }
    }

    fun openOffer(app: CatalogApp) {
        DiagLog.note("ui", "открыл приложение — ${app.title} · ${app.id}")
        // TestFlight-приложение нечего разрешать в версии и нечего покупать:
        // ставит его сам iPhone по приглашению. Ветка стоит здесь, а не в
        // карточках каталога, — через openOffer проходят все три места,
        // откуда открывают приложение.
        val invite = app.testflight?.url?.trim().orEmpty()
        if (invite.isNotEmpty()) {
            _state.update { it.copy(testFlightApp = app, testFlightUrl = invite) }
            return
        }
        viewModelScope.launch {
            _state.update {
                it.copy(
                    offerLoading = true,
                    offerError = "",
                    offer = null,
                    versions = emptyList(),
                    versionsLoading = false,
                    versionsError = "",
                    versionsSearch = "",
                    versionsVisibleCount = 12,
                    chosenVersion = null,
                    restoreOpen = false,
                    restoreSearch = "",
                    restoreVisibleCount = 12,
                    bankAltOpen = false,
                )
            }
            val deviceIos = _state.value.selectedDevice?.productVersion
            runCatching { engine.resolveCatalogInstall(app, deviceIos) }
                .onSuccess { offer ->
                    _state.update {
                        it.copy(
                            offer = offer,
                            offerLoading = false,
                        )
                    }
                    loadStoreVersions(offer.appStoreId, force = false)
                    probeRestore(offer, force = false)
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(offerLoading = false, offerError = Errors.localize(e.message ?: ""))
                    }
                }
        }
    }

    fun openStoreHit(hit: AppStoreSearchHit) {
        DiagLog.note("ui", "открыл результат App Store — ${hit.name} · ${hit.adamId}")
        val gone = hit.removedFromStore
        val offer = engine.resolveOffer(
            appId = "store:${hit.adamId}",
            title = hit.name,
            icon = hit.artwork,
            inStore = !gone && !hit.foundOnAccount,
            version = hit.version,
            minIos = hit.minIos,
            bundleId = hit.bundleId,
            storeId = hit.adamId,
            deviceIos = _state.value.selectedDevice?.productVersion,
        )
        // Тексты те же, что на ПК: человек должен сразу понимать, откуда
        // возьмётся приложение.
        val resolved = when {
            hit.foundOnAccount -> offer.copy(
                source = "restore",
                message = "Приложение найдено в покупках текущего Apple ID. Можно скачать IPA или установить на iPhone.",
            )
            gone -> offer.copy(
                source = "restore",
                message = "Этого приложения уже нет в App Store. Установить можно, только если оно есть в покупках этого Apple ID.",
            )
            else -> offer
        }
        _state.update {
            it.copy(
                offer = resolved,
                offerError = "",
                versions = emptyList(),
                chosenVersion = null,
                versionsError = "",
                versionsSearch = "",
                versionsVisibleCount = 12,
                versionsIosPending = emptySet(),
                restoreOpen = false,
                bankAltOpen = false,
            )
        }
        loadStoreVersions(hit.adamId, force = false)
    }

    fun closeOffer() = _state.update {
        it.copy(
            offer = null,
            offerError = "",
            offerLoading = false,
            bankAltOpen = false,
            restoreOpen = false,
            versionsLoading = false,
        )
    }

    fun closeOfferError() = _state.update { it.copy(offerError = "") }

    fun chooseVersion(v: AppStoreVersion) = _state.update { it.copy(chosenVersion = v) }

    fun setVersionsSearch(q: String) = _state.update {
        it.copy(versionsSearch = q, versionsVisibleCount = 12)
    }

    fun showMoreVersions() = _state.update { it.copy(versionsVisibleCount = it.versionsVisibleCount + 12) }

    /**
     * Ищет самую свежую сборку, которая запустится на подключённом iPhone.
     *
     * Перебирать все сборки подряд бессмысленно: минимальная iOS не убывает с
     * ростом версии, поэтому внизу работает деление пополам — у приложения с
     * сотнями сборок это около десятка проверок вместо всех.
     */
    fun pickVersionForDevice() {
        val adam = _state.value.offer?.appStoreId ?: return
        if (_state.value.versionFitBusy) return
        val deviceIos = _state.value.selectedDevice?.productVersion
        if (deviceIos.isNullOrBlank()) {
            _state.update { it.copy(versionsError = "Подключите iPhone, чтобы подобрать версию.") }
            return
        }
        viewModelScope.launch {
            _state.update { it.copy(versionFitBusy = true, versionsError = "") }
            val fit = coroutineScope {
                async(Dispatchers.IO) {
                    runCatching { engine.store.findVersionForDevice(adam, deviceIos) }.getOrNull()
                }.await()
            }
            if (_state.value.offer?.appStoreId != adam) return@launch
            if (fit == null) {
                _state.update {
                    it.copy(
                        versionFitBusy = false,
                        versionsError = "Ни одна версия из покупок не запустится на iOS $deviceIos.",
                    )
                }
                return@launch
            }
            val picked = AppStoreVersion(
                version = fit.version,
                externalVersionId = fit.externalVersionId,
                isLatest = fit.isLatest,
                alreadyOwned = true,
                minIos = fit.minIos,
            )
            _state.update { state ->
                val merged = if (state.versions.any { it.externalVersionId == picked.externalVersionId }) {
                    state.versions.map { if (it.externalVersionId == picked.externalVersionId) picked else it }
                } else {
                    listOf(picked) + state.versions
                }
                state.copy(
                    versions = merged,
                    chosenVersion = picked,
                    versionsSearch = "",
                    versionFitBusy = false,
                )
            }
        }
    }

    /**
     * Что стоит на iPhone в версии старее каталожной.
     *
     * Обложки (`in_store == false`) сознательно пропускаем: их «версии» — это
     * разные приложения под одной карточкой, и предложить «обновиться» значит
     * подменить установленное чужим. Сверка идёт по данным, которые каталог уже
     * прислал, поэтому сеть здесь не нужна.
     */
    fun availableUpdates(): List<Pair<CatalogApp, InstalledApp>> {
        val installed = _state.value.installed.associateBy { it.bundleId }
        return _state.value.catalog.mapNotNull { app ->
            if (app.in_store != true) return@mapNotNull null
            val bundle = app.bundle_id?.trim().orEmpty()
            if (bundle.isEmpty()) return@mapNotNull null
            val have = installed[bundle] ?: return@mapNotNull null
            val remote = app.version?.trim().orEmpty()
            val current = have.version?.trim().orEmpty()
            if (remote.isEmpty() || current.isEmpty()) return@mapNotNull null
            if (CatalogResolve.compareIos(remote, current) <= 0) return@mapNotNull null
            app to have
        }
    }

    /** Ставит в очередь обновление одного приложения из каталога. */
    fun queueUpdate(app: CatalogApp, onDone: () -> Unit) {
        if (_state.value.bulkBusy) return
        viewModelScope.launch {
            _state.update { it.copy(bulkBusy = true) }
            val ok = runCatching { queueCatalogApp(app) }.getOrDefault(false)
            _state.update {
                it.copy(
                    bulkBusy = false,
                    catalogError = if (ok) "" else "Не удалось поставить в очередь: ${app.title}",
                )
            }
            if (ok) onDone()
        }
    }

    fun queueAllUpdates(onDone: () -> Unit) {
        val apps = availableUpdates().map { it.first }
        if (apps.isEmpty() || _state.value.bulkBusy) return
        viewModelScope.launch {
            _state.update { it.copy(bulkBusy = true) }
            var queued = 0
            for (app in apps) if (runCatching { queueCatalogApp(app) }.getOrDefault(false)) queued++
            _state.update { it.copy(bulkBusy = false) }
            if (queued > 0) onDone()
        }
    }

    fun storeRegions(): List<pw.appbank.signum.data.region.StoreRegion> =
        runCatching { engine.region.regions() }.getOrDefault(emptyList())

    fun loadStoreRegion() {
        val email = engine.settings.activeEmail() ?: run {
            _state.update { it.copy(regionMessage = "Сначала войдите в Apple ID.") }
            return
        }
        if (_state.value.regionBusy) return
        viewModelScope.launch {
            _state.update { it.copy(regionBusy = true, regionMessage = "") }
            val status = coroutineScope {
                async(Dispatchers.IO) { runCatching { engine.region.currentRegion(email) } }.await()
            }
            _state.update {
                it.copy(
                    regionBusy = false,
                    regionCode = status.getOrNull()?.code.orEmpty(),
                    regionName = status.getOrNull()?.name.orEmpty(),
                    regionMessage = status.exceptionOrNull()?.message.orEmpty(),
                    regionPendingRestore = engine.region.pendingRestore(email) != null,
                )
            }
        }
    }

    /**
     * Меняет страну магазина.
     *
     * `temporary = true` пишет снимок исходной страны до переключения, чтобы её
     * можно было вернуть даже если приложение убьют посреди установки.
     */
    fun changeStoreRegion(iso3: String, temporary: Boolean = false) {
        val email = engine.settings.activeEmail() ?: run {
            _state.update { it.copy(regionMessage = "Сначала войдите в Apple ID.") }
            return
        }
        if (_state.value.regionBusy) return
        viewModelScope.launch {
            _state.update { it.copy(regionBusy = true, regionMessage = "") }
            val status = coroutineScope {
                async(Dispatchers.IO) {
                    runCatching {
                        if (temporary) engine.region.switchTemporarily(email, iso3)
                        else engine.region.switchTo(email, iso3)
                    }
                }.await()
            }
            _state.update {
                it.copy(
                    regionBusy = false,
                    regionCode = status.getOrNull()?.code ?: it.regionCode,
                    regionName = status.getOrNull()?.name ?: it.regionName,
                    regionMessage = status.exceptionOrNull()?.message
                        ?: status.getOrNull()?.message.orEmpty(),
                    regionPendingRestore = engine.region.pendingRestore(email) != null,
                )
            }
        }
    }

    fun restoreStoreRegion() {
        val email = engine.settings.activeEmail() ?: return
        if (_state.value.regionBusy) return
        viewModelScope.launch {
            _state.update { it.copy(regionBusy = true, regionMessage = "") }
            val status = coroutineScope {
                async(Dispatchers.IO) { runCatching { engine.region.restoreOriginal(email) } }.await()
            }
            _state.update {
                it.copy(
                    regionBusy = false,
                    regionCode = status.getOrNull()?.code ?: it.regionCode,
                    regionName = status.getOrNull()?.name ?: it.regionName,
                    regionMessage = status.exceptionOrNull()?.message.orEmpty(),
                    regionPendingRestore = engine.region.pendingRestore(email) != null,
                )
            }
        }
    }

    fun toggleFavorite(appId: String) = engine.toggleFavorite(appId)

    fun setSelectMode(on: Boolean) = _state.update {
        it.copy(selectMode = on, selectedAppIds = if (on) it.selectedAppIds else emptySet())
    }

    fun toggleSelected(appId: String) = _state.update { state ->
        val next = state.selectedAppIds.toMutableSet()
        if (!next.remove(appId)) next.add(appId)
        state.copy(selectedAppIds = next)
    }

    /**
     * Ставит в очередь всё отмеченное разом.
     *
     * Каждое приложение всё равно надо разобрать так же, как при открытии
     * карточки, поэтому идём по одному, а не пачкой: зато то, что не удалось,
     * возвращаем текстом, а не роняем молча всю операцию.
     */
    fun installSelected(onDone: () -> Unit) {
        val ids = _state.value.selectedAppIds.toList()
        if (ids.isEmpty() || _state.value.bulkBusy) return
        if (!engine.authStatus().authenticated) {
            _state.update { it.copy(catalogError = "Сначала войдите в Apple ID на странице «Аккаунт».") }
            return
        }
        viewModelScope.launch {
            _state.update { it.copy(bulkBusy = true, catalogError = "") }
            var queued = 0
            val failed = mutableListOf<String>()
            for (appId in ids) {
                val app = _state.value.catalog.firstOrNull { it.id == appId } ?: continue
                val ok = runCatching { queueCatalogApp(app) }.getOrDefault(false)
                if (ok) queued++ else failed += app.title
            }
            _state.update {
                it.copy(
                    bulkBusy = false,
                    selectMode = queued > 0 && failed.isEmpty(),
                    selectedAppIds = if (queued > 0) emptySet() else it.selectedAppIds,
                    catalogError = when {
                        failed.isEmpty() -> ""
                        queued == 0 -> "Не удалось поставить в очередь: ${failed.joinToString(", ")}"
                        else -> "Поставлено ${queued}. Не получилось: ${failed.joinToString(", ")}"
                    },
                )
            }
            if (queued > 0) onDone()
        }
    }

    private suspend fun queueCatalogApp(app: CatalogApp): Boolean {
        val deviceIos = _state.value.selectedDevice?.productVersion
        val offer = runCatching { engine.resolveCatalogInstall(app, deviceIos) }.getOrNull() ?: return false
        val adam = offer.appStoreId?.trim().orEmpty()
        if (adam.isEmpty()) return false
        engine.enqueueInstall(
            title = offer.title,
            adamId = adam,
            bundleId = offer.bundleId,
            inStore = offer.inStore,
            alreadyOwned = !offer.inStore,
            version = offer.version,
            externalVersionId = null,
            install = true,
        )
        return true
    }

    fun refreshStoreVersions() {
        val adam = _state.value.offer?.appStoreId ?: return
        loadStoreVersions(adam, force = true)
    }

    fun openRestoreVersions() {
        val offer = _state.value.offer ?: return
        val cached = _state.value.restoreScans[offer.appId]
        if (cached != null) {
            _state.update { it.copy(restoreOpen = true, restoreScan = cached, restoreSearch = "", restoreVisibleCount = 12) }
            return
        }
        _state.update { it.copy(restoreOpen = true, restoreSearch = "", restoreVisibleCount = 12) }
        probeRestore(offer, force = false)
    }

    fun closeRestoreVersions() = _state.update { it.copy(restoreOpen = false) }

    fun setRestoreSearch(q: String) = _state.update {
        it.copy(restoreSearch = q, restoreVisibleCount = 12)
    }

    fun showMoreRestore() = _state.update { it.copy(restoreVisibleCount = it.restoreVisibleCount + 12) }

    fun refreshRestoreVersions() {
        val offer = _state.value.offer ?: return
        probeRestore(offer, force = true)
    }

    fun chooseRestoreItem(item: AccountRestoreVersion) {
        val offer = _state.value.offer ?: return
        val deviceIos = _state.value.selectedDevice?.productVersion
        val next = CatalogResolve.applyRestoreItem(offer, item, deviceIos)
        _state.update {
            it.copy(
                offer = next,
                restoreOpen = false,
                versions = emptyList(),
                chosenVersion = null,
                versionsError = "",
                versionsSearch = "",
                versionsVisibleCount = 12,
                offerError = "",
            )
        }
        loadStoreVersions(item.appStoreId, force = false)
        probeRestore(next, force = false)
    }

    /**
     * Альтернативный способ — временный Apple ID банка.
     *
     * Раньше кнопка открывала окно «сейчас этот способ недоступен». Теперь за
     * ней тот же поток, что и на ПК: телефон → код из SMS → логин и пароль.
     */
    private var bankSession: BankOtp.Session? = null

    fun openBankAlt() = _state.update {
        it.copy(
            bankAltOpen = true,
            bankAltStep = "phone",
            bankAltCode = "",
            bankAltMessage = "",
            bankAltLogin = "",
            bankAltPassword = "",
            bankAltLink = "",
        )
    }

    fun closeBankAlt() = _state.update { it.copy(bankAltOpen = false) }

    fun setBankAltPhone(v: String) = _state.update { it.copy(bankAltPhone = v) }
    fun setBankAltCode(v: String) = _state.update { it.copy(bankAltCode = v) }

    fun bankAltSendSms(bankKey: String) {
        if (_state.value.bankAltBusy) return
        _state.update { it.copy(bankAltBusy = true, bankAltMessage = "Отправляем SMS…") }
        viewModelScope.launch {
            runCatching { engine.bankSendSms(bankKey, _state.value.bankAltPhone) }
                .onSuccess { (session, msg) ->
                    bankSession = session
                    _state.update {
                        it.copy(bankAltBusy = false, bankAltStep = "code", bankAltMessage = msg)
                    }
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(bankAltBusy = false, bankAltMessage = Errors.localize(e.message ?: "Не удалось отправить SMS."))
                    }
                }
        }
    }

    fun bankAltConfirm() {
        val session = bankSession ?: return
        if (_state.value.bankAltBusy) return
        _state.update { it.copy(bankAltBusy = true, bankAltMessage = "Проверяем код…") }
        viewModelScope.launch {
            runCatching {
                engine.bankValidate(session, _state.value.bankAltCode)
                engine.bankCredentials(session)
            }
                .onSuccess { creds ->
                    _state.update {
                        it.copy(
                            bankAltBusy = false,
                            bankAltStep = "done",
                            bankAltLogin = creds.login,
                            bankAltPassword = creds.password,
                            bankAltLink = creds.link.orEmpty(),
                            bankAltMessage = "Эта учётная запись живёт около 30 минут.",
                        )
                    }
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(bankAltBusy = false, bankAltMessage = Errors.localize(e.message ?: "Не удалось получить учётную запись."))
                    }
                }
        }
    }

    fun queueOffer(install: Boolean): Boolean {
        val offer = _state.value.offer ?: return false
        val adam = offer.appStoreId?.trim().orEmpty()
        if (adam.isEmpty()) {
            _state.update {
                it.copy(offerError = "Не удалось определить приложение. Выберите другую версию в списке.")
            }
            return false
        }
        if (!engine.authStatus().authenticated) {
            _state.update { it.copy(offerError = "Сначала войдите в Apple ID на странице «Аккаунт».") }
            return false
        }
        val chosen = _state.value.chosenVersion
        val jobId = engine.enqueueInstall(
            title = offer.title,
            adamId = adam,
            bundleId = offer.bundleId,
            inStore = offer.inStore,
            alreadyOwned = chosen?.alreadyOwned ?: !offer.inStore,
            version = chosen?.version ?: offer.version,
            externalVersionId = chosen?.externalVersionId,
            install = install,
        )
        if (install) {
            activationFor(offer.appId)
                ?.let { activationForInstalled(it, adam, offer.bundleId, offer.title) }
                ?.let { pendingActivations[jobId] = offer.appTitle to it }
        }
        _state.update { it.copy(offer = null, offerError = "", bankAltOpen = false) }
        return true
    }

    private var storeVersionsRequest = 0

    private fun loadStoreVersions(adamId: String?, force: Boolean) {
        val request = ++storeVersionsRequest
        if (adamId.isNullOrBlank()) {
            _state.update { it.copy(versions = emptyList(), versionsLoading = false, versionsError = "") }
            return
        }
        if (!engine.authStatus().authenticated) {
            _state.update {
                it.copy(
                    versions = emptyList(),
                    versionsLoading = false,
                    versionsError = "Войдите в Apple ID, чтобы увидеть версии с этой учётки.",
                )
            }
            return
        }
        viewModelScope.launch {
            _state.update {
                it.copy(
                    versions = emptyList(),
                    chosenVersion = null,
                    versionsLoading = true,
                    versionsError = "",
                    versionsIosPending = emptySet(),
                )
            }
            val ids = runCatching { engine.listVersionIds(adamId, force) }
                .onFailure { e ->
                    if (_state.value.offer?.appStoreId == adamId) {
                        _state.update {
                            it.copy(
                                versionsLoading = false,
                                versionsError = Errors.localize(e.message ?: ""),
                            )
                        }
                    }
                }
                .getOrNull() ?: return@launch
            if (request != storeVersionsRequest || _state.value.offer?.appStoreId != adamId) return@launch
            if (ids.isEmpty()) {
                _state.update {
                    it.copy(
                        versions = emptyList(),
                        chosenVersion = null,
                        versionsLoading = false,
                        versionsError = "",
                    )
                }
                return@launch
            }
            // То, что уже знаем, показываем сразу: значение неизменно, и
            // повторный заход по тому же приложению рисуется без ожидания.
            val knownIos = runCatching { engine.store.cachedVersionMinIos(adamId, ids) }
                .getOrDefault(emptyMap())
            var current = ids.mapIndexed { index, id ->
                AppStoreVersion(id, id, isLatest = index == 0, alreadyOwned = true, minIos = knownIos[id])
            }
            _state.update {
                it.copy(
                    versions = emptyList(),
                    chosenVersion = null,
                    versionsLoading = true,
                )
            }
            ids.chunked(4).forEach { chunk ->
                // Номера версий не ждут чтения Info.plist.
                val labels = coroutineScope {
                    val labelsAsync = chunk.map { id ->
                        async(Dispatchers.IO) { id to engine.versionDisplay(adamId, id) }
                    }
                    labelsAsync.awaitAll()
                }
                if (request != storeVersionsRequest || _state.value.offer?.appStoreId != adamId) return@launch
                val map = labels.toMap()
                current = current.map { item ->
                    val human = map[item.externalVersionId] ?: item.version
                    item.copy(
                        version = human,
                    )
                }.sortedWith(
                    compareByDescending<AppStoreVersion> { marketingScore(it.version) }
                        .thenByDescending { it.externalVersionId },
                ).mapIndexed { index, item -> item.copy(isLatest = index == 0) }
                _state.update {
                    val selectedId = it.chosenVersion?.externalVersionId
                    val named = current.filter { v -> v.version.isNotBlank() && v.version != v.externalVersionId && !v.version.matches(Regex("\\d{8,}")) }
                    it.copy(
                        versions = named,
                        chosenVersion = named.firstOrNull { v -> v.externalVersionId == selectedId }
                            ?: named.firstOrNull(),
                    )
                }
            }
            val pendingIos = ids.filter { knownIos[it].isNullOrBlank() }.toSet()
            _state.update { it.copy(versionsLoading = false, versionsIosPending = pendingIos) }
            withContext(Dispatchers.IO) {
                runCatching {
                    engine.store.versionMinIosBatch(adamId, ids) { ext, value ->
                        if (request == storeVersionsRequest && _state.value.offer?.appStoreId == adamId) {
                            _state.update { state -> state.copy(
                                versions = state.versions.map { if (it.externalVersionId == ext) it.copy(minIos = value) else it },
                                chosenVersion = state.chosenVersion?.let { if (it.externalVersionId == ext) it.copy(minIos = value) else it },
                                versionsIosPending = state.versionsIosPending - ext,
                            ) }
                        }
                    }
                }
            }
            // Совместимость — подсказка, а не условие: неудача не мешает скачать.
            if (request == storeVersionsRequest) _state.update { it.copy(versionsIosPending = emptySet()) }
        }
    }

    private fun probeRestore(offer: CatalogInstallOffer, force: Boolean) {
        if (!engine.authStatus().authenticated) return
        if (!force) {
            _state.value.restoreScans[offer.appId]?.let { cached ->
                _state.update {
                    it.copy(
                        restoreScan = cached,
                        offer = it.offer?.copy(showAccountVersions = cached.items.isNotEmpty()),
                    )
                }
                return
            }
        }
        viewModelScope.launch {
            _state.update { it.copy(restoreLoading = true) }
            val selected = offer.catalogVersions.firstOrNull { it.appStoreId == offer.appStoreId && it.bundleId == offer.bundleId }
                ?: offer.catalogVersions.firstOrNull { it.appStoreId == offer.appStoreId }
            runCatching {
                engine.probeAccountRestore(
                    appId = offer.appId,
                    appTitle = offer.appTitle,
                    versions = offer.catalogVersions,
                    exclude = selected,
                    deviceIos = offer.deviceIos ?: _state.value.selectedDevice?.productVersion,
                    force = force,
                )
            }.onSuccess { scan ->
                _state.update {
                    it.copy(
                        restoreLoading = false,
                        restoreScan = scan,
                        restoreScans = it.restoreScans + (offer.appId to scan),
                        offer = it.offer?.copy(showAccountVersions = scan.items.isNotEmpty()),
                    )
                }
            }.onFailure { e ->
                _state.update {
                    it.copy(
                        restoreLoading = false,
                        restoreScan = AccountRestoreScan(message = Errors.localize(e.message ?: "")),
                    )
                }
            }
        }
    }

    private fun marketingScore(version: String): Long {
        val cleaned = version.trim().trimStart('v', 'V')
        if (cleaned.all { it.isDigit() } && cleaned.length >= 8) return -1
        val parts = cleaned.split(Regex("[^0-9]+")).mapNotNull { it.toLongOrNull() }
        if (parts.isEmpty()) return -1
        return parts.getOrElse(0) { 0 } * 1_000_000 + parts.getOrElse(1) { 0 } * 1_000 + parts.getOrElse(2) { 0 }
    }

    fun setLoginEmail(v: String) = _state.update { it.copy(loginEmail = v) }
    fun setLoginPassword(v: String) = _state.update { it.copy(loginPassword = v) }
    fun setLoginCode(v: String) = _state.update { it.copy(loginCode = v.filter { ch -> ch.isDigit() }.take(6)) }

    fun submitLogin() {
        viewModelScope.launch {
            _state.update { it.copy(loginBusy = true, loginMessage = "") }
            val s = _state.value
            runCatching { engine.login(s.loginEmail, s.loginPassword, s.loginCode.takeIf { it.length >= 6 }) }
                .onFailure { e ->
                    android.util.Log.e("SignumLogin", e.message ?: e.javaClass.simpleName, e)
                }
                .onSuccess { r ->
                    refreshAuth()
                    _state.update {
                        it.copy(
                            loginBusy = false,
                            loginStep = if (r.needs2fa) "two_factor" else "credentials",
                            loginMessage = r.message.orEmpty(),
                            loginCode = "",
                        )
                    }
                    if (!r.needs2fa) loadHistory(false)
                }
                .onFailure { e ->
                    _state.update { it.copy(loginBusy = false, loginMessage = Errors.localize(e.message ?: "")) }
                }
        }
    }

    fun resendLoginCode() {
        viewModelScope.launch {
            _state.update { it.copy(loginBusy = true) }
            runCatching { engine.resendCode() }
                .onSuccess { r ->
                    _state.update { it.copy(loginBusy = false, loginMessage = r.message.orEmpty(), loginStep = "two_factor") }
                }
                .onFailure { e ->
                    _state.update { it.copy(loginBusy = false, loginMessage = Errors.localize(e.message ?: "")) }
                }
        }
    }

    fun logout() {
        DiagLog.note("ui", "нажал «Выйти из Apple ID»")
        engine.logout()
        refreshAuth()
        _state.update {
            it.copy(
                history = emptyList(),
                historyHasMore = false,
                historyNextBatchId = null,
                historyNeeds2fa = false,
                history2fa = "",
                historyMessage = "",
                historySelected = emptySet(),
                loginMessage = "Вы вышли из Apple ID",
                loginStep = "credentials",
            )
        }
    }

    fun loadHistory(force: Boolean) {
        viewModelScope.launch {
            _state.update { it.copy(historyLoading = true, historyMessage = "") }
            engine.cachedHistory()?.let { cached ->
                if (!force) _state.update { it.copy(history = cached.apps, historySource = cached.source) }
            }
            runCatching { engine.fetchHistory(_state.value.history2fa.takeIf { it.length >= 6 }, false, null, force) }
                .onSuccess { r ->
                    _state.update {
                        // Запрос кода приходит с пустым списком — уже показанную
                        // историю при этом не стираем.
                        val keepShown = r.needs2fa && r.apps.isEmpty()
                        it.copy(
                            history = if (keepShown) it.history else r.apps,
                            historySource = if (keepShown) it.historySource else r.source,
                            historyLoading = false,
                            historyNeeds2fa = r.needs2fa,
                            historyMessage = r.message.orEmpty(),
                            history2fa = if (r.needs2fa) it.history2fa else "",
                            historyHasMore = if (keepShown) it.historyHasMore else r.hasMore,
                            historyNextBatchId = if (keepShown) it.historyNextBatchId else r.nextBatchId,
                        )
                    }
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(historyLoading = false, historyMessage = Errors.localize(e.message ?: ""))
                    }
                }
        }
    }

    /**
     * «Попробовать без кода»: история через уже сохранённую сессию App Store
     * (commerce-API, как ipatool list-purchases). Список может быть неполным —
     * экран показывает плашку и кнопку «Повторить с кодом».
     */
    fun loadHistoryViaLibrary() {
        if (_state.value.historyLibraryLoading) return
        viewModelScope.launch {
            _state.update { it.copy(historyLibraryLoading = true, historyMessage = "") }
            runCatching { engine.fetchHistoryViaLibrary() }
                .onSuccess { r ->
                    _state.update {
                        it.copy(
                            history = r.apps,
                            historySource = r.source,
                            historyLibraryLoading = false,
                            historyNeeds2fa = false,
                            history2fa = "",
                            historySelected = emptySet(),
                            historyMessage = "",
                        )
                    }
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(historyLibraryLoading = false, historyMessage = Errors.localize(e.message ?: ""))
                    }
                }
        }
    }

    /** Полная история с reportaproblem — Apple при необходимости попросит код. */
    fun retryHistoryWithCode() = loadHistory(true)

    fun setHistorySearch(q: String) = _state.update { it.copy(historySearch = q) }
    fun setHistory2fa(q: String) = _state.update { it.copy(history2fa = q.filter { it.isDigit() }.take(6)) }

    fun toggleHistorySelected(id: String) {
        if (id.isBlank()) return
        _state.update {
            val next = it.historySelected.toMutableSet()
            if (!next.add(id)) next.remove(id)
            it.copy(historySelected = next)
        }
    }

    fun toggleHistorySelectAll(ids: List<String>) {
        _state.update {
            val visible = ids.filter { id -> id.isNotBlank() }
            if (visible.isEmpty()) return@update it
            val allOn = visible.all { id -> it.historySelected.contains(id) }
            val next = it.historySelected.toMutableSet()
            if (allOn) next.removeAll(visible.toSet()) else next.addAll(visible)
            it.copy(historySelected = next)
        }
    }

    fun clearHistorySelection() = _state.update { it.copy(historySelected = emptySet()) }

    fun installFromHistory(item: PurchaseHistoryApp, install: Boolean) {
        engine.enqueueInstall(
            title = item.name,
            adamId = item.adamId,
            bundleId = item.bundleId,
            inStore = false,
            alreadyOwned = true,
            version = null,
            externalVersionId = null,
            install = install,
        )
        _state.update {
            it.copy(historyMessage = "«${item.name}» в очереди. Список можно не обновлять.")
        }
    }

    fun enqueueSelectedHistory(install: Boolean) {
        val selected = _state.value.historySelected
        val items = _state.value.history.filter { selected.contains(it.adamId) }
        if (items.isEmpty()) return
        _state.update { it.copy(historyBusy = true) }
        items.forEach { item ->
            engine.enqueueInstall(
                title = item.name,
                adamId = item.adamId,
                bundleId = item.bundleId,
                inStore = false,
                alreadyOwned = true,
                version = null,
                externalVersionId = null,
                install = install,
            )
        }
        _state.update {
            it.copy(
                historyBusy = false,
                historySelected = emptySet(),
                historyMessage = if (install) {
                    "${items.size} в очереди установки. Список можно не обновлять."
                } else {
                    "${items.size} в очереди загрузки. Список можно не обновлять."
                },
            )
        }
    }

    /**
     * Обновить список устройств.
     *
     * keepMessage нужен после «Доверять»: там уже написан ответ об успехе, а
     * обновление списка стирало его в ту же секунду — человек видел, что
     * доверие установилось, но приложение об этом молчало.
     */
    fun refreshDevices(keepMessage: Boolean = false) {
        viewModelScope.launch {
            _state.update {
                it.copy(
                    devicesLoading = true,
                    deviceMessage = if (keepMessage) it.deviceMessage else "",
                )
            }
            runCatching { engine.listDevices() }
                .onSuccess { list ->
                    val saved = engine.settingsFlow.value.selectedDeviceUdid
                    val selected = list.find { it.udid == saved } ?: list.firstOrNull()
                    _state.update {
                        it.copy(devices = list, selectedDevice = selected, devicesLoading = false)
                    }
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(devicesLoading = false, deviceMessage = Errors.localize(e.message ?: ""))
                    }
                }
        }
    }

    fun trust() {
        DiagLog.note("ui", "нажал «Доверять»")
        viewModelScope.launch {
            _state.update { it.copy(deviceMessage = "Разблокируйте iPhone и нажмите «Доверять»…") }
            runCatching { engine.trustDevice() }
                .onSuccess { msg ->
                    _state.update { it.copy(deviceMessage = msg) }
                    refreshDevices(keepMessage = true)
                }
                .onFailure { e -> _state.update { it.copy(deviceMessage = Errors.localize(e.message ?: "")) } }
        }
    }

    fun loadInstalled() {
        DiagLog.note("ui", "запросил список приложений на iPhone")
        if (_state.value.installedLoading) return
        viewModelScope.launch {
            _state.update { it.copy(installedLoading = true) }
            runCatching { engine.installedApps() }
                .onSuccess { apps -> _state.update { it.copy(installed = apps, installedLoading = false) } }
                .onFailure { e ->
                    _state.update { it.copy(installedLoading = false, deviceMessage = Errors.localize(e.message ?: "")) }
                }
        }
    }

    fun restoreOffloaded(app: InstalledApp) {
        if (!app.isPlaceholder) return
        viewModelScope.launch { restoreOffloadedNow(app) }
    }

    /**
     * «Восстановить все» — по одному, а не пачкой параллельных запросов:
     * раньше каждое приложение било в App Store одновременно, а сообщения
     * затирали друг друга, и человек видел только последнее.
     */
    fun restoreAllOffloaded() {
        val apps = _state.value.installed.filter { it.isPlaceholder }
        if (apps.isEmpty()) return
        viewModelScope.launch {
            var ok = 0
            for (app in apps) if (restoreOffloadedNow(app)) ok++
            _state.update {
                it.copy(deviceMessage = "В очередь восстановления добавлено: $ok из ${apps.size}.")
            }
        }
    }

    private suspend fun restoreOffloadedNow(app: InstalledApp): Boolean {
        val owner = engine.settings.activeEmail()
        val deviceIos = _state.value.selectedDevice?.productVersion
        _state.update { it.copy(deviceMessage = "Проверяем покупку: ${app.name ?: app.bundleId}…") }
        // resolveAdamId, versionMinIos и ownsListing ходят в сеть синхронно.
        // На главном потоке это NetworkOnMainThreadException: восстановление
        // сгруженного приложения не работало никогда, всегда падало с ошибкой.
        return withContext(Dispatchers.IO) {
            runCatching {
                if (owner.isNullOrBlank()) error("Сначала войдите в Apple ID на странице «Аккаунт».")
                if (deviceIos.isNullOrBlank()) error("Не удалось определить версию iOS устройства.")
                val adamId = app.appStoreId ?: engine.store.resolveAdamId(null, app.bundleId)
                    ?: error("Не удалось определить App Store ID сгруженного приложения.")
                val ownerBefore = engine.settings.activeEmail()
                val versions = engine.listVersionIds(adamId)
                val compatible = versions.firstNotNullOfOrNull { externalId ->
                    val minimum = engine.store.versionMinIos(adamId, externalId) ?: return@firstNotNullOfOrNull null
                    if (CatalogResolve.compareIos(deviceIos, minimum) >= 0) externalId to minimum else null
                } ?: error("В покупках нет версии, совместимой с iOS $deviceIos.")
                if (engine.settings.activeEmail() != ownerBefore) error("Apple ID изменился. Повторите попытку.")
                if (!engine.store.ownsListing(adamId, app.bundleId)) error("Этого приложения нет в покупках активного Apple ID.")
                engine.enqueueInstall(
                    title = app.name ?: app.bundleId,
                    adamId = adamId,
                    bundleId = app.bundleId,
                    inStore = false,
                    alreadyOwned = true,
                    version = null,
                    externalVersionId = compatible.first,
                    install = true,
                )
            }.onSuccess {
                _state.update { it.copy(deviceMessage = "${app.name ?: app.bundleId} добавлено в очередь восстановления.") }
            }.onFailure { error ->
                _state.update { it.copy(deviceMessage = Errors.localize(error.message ?: "Не удалось восстановить приложение.")) }
            }.isSuccess
        }
    }

    fun selectDevice(d: DeviceInfo) {
        engine.selectDevice(d.udid)
        _state.update { it.copy(selectedDevice = d) }
    }

    fun requestUsb() {
        val msg = runCatching { engine.requestUsb() }.getOrElse { e ->
            Errors.localize(e.message ?: "Не удалось запросить доступ к iPhone")
        }
        _state.update { it.copy(deviceMessage = msg) }
        refreshDevices()
    }

    fun setSignPath(p: String) = _state.update { it.copy(signPath = p) }

    fun cancelJob(id: String) = engine.cancelJob(id)

    fun queueLocal(path: String, sign: Boolean, install: Boolean) {
        engine.enqueueLocal(path, sign, install)
        _state.update { it.copy(signMessage = "Добавлено в очередь") }
    }

    /**
     * Узнать, вышла ли новая версия Signum.
     *
     * Спрашиваем один раз при запуске и молчим, если обновления нет: на ПК
     * человек видит баннер только когда есть что ставить.
     */
    private fun checkOta() {
        viewModelScope.launch {
            val info = runCatching { engine.fetchOta() }.getOrNull() ?: return@launch
            // Устаревший клиент закрываем окном обновления всегда — даже при
            // выключенной автопроверке: сервер его всё равно не примет.
            if (!info.clientSupported) {
                _state.update {
                    it.copy(
                        ota = info,
                        clientOutdated = pw.appbank.signum.data.api.ClientOutdated(
                            message = info.notes?.trim()?.takeIf { n -> n.isNotEmpty() }
                                ?: "Эта версия Signum устарела. Обновите приложение.",
                            minClientVersion = info.minClientVersion,
                        ),
                    )
                }
                return@launch
            }
            // Настройка «Автопроверка обновлений»: баннер не показываем.
            if (!engine.settingsFlow.value.autoUpdate) return@launch
            if (!info.hasUpdate || info.downloadUrl.isNullOrBlank()) return@launch
            _state.update { it.copy(ota = info) }
        }
    }

    fun dismissOta() = _state.update { it.copy(ota = null, otaMessage = "") }

    /** Скачать обновление. Устанавливает его уже экран — ему доступен Context. */
    fun downloadOta() {
        val info = _state.value.ota ?: return
        if (_state.value.otaBusy) return
        _state.update { it.copy(otaBusy = true, otaPercent = 0f, otaMessage = "Загрузка…") }
        viewModelScope.launch {
            runCatching { engine.downloadUpdate(info) { pct -> _state.update { s -> s.copy(otaPercent = pct) } } }
                .onSuccess { file ->
                    _state.update {
                        it.copy(otaBusy = false, otaPercent = 100f, otaFile = file.absolutePath, otaMessage = "")
                    }
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(otaBusy = false, otaMessage = Errors.localize(e.message ?: "Не удалось скачать обновление."))
                    }
                }
        }
    }

    /**
     * «Обновить Signum» из окна устаревшего клиента. Окно могло открыться по
     * ответу 426 на любой запрос, когда сведений об обновлении ещё нет, —
     * тогда сначала спрашиваем ota.php.
     */
    fun updateFromOutdatedGate() {
        if (_state.value.otaBusy || _state.value.otaChecking) return
        if (_state.value.ota?.downloadUrl.isNullOrBlank().not()) {
            downloadOta()
            return
        }
        viewModelScope.launch {
            _state.update { it.copy(otaChecking = true, otaMessage = "") }
            val info = runCatching { engine.fetchOta() }.getOrNull()
            if (info == null || info.downloadUrl.isNullOrBlank()) {
                _state.update {
                    it.copy(
                        otaChecking = false,
                        otaMessage = "Ссылка на обновление недоступна. Скачайте новую версию на сайте appbank.pw.",
                    )
                }
                return@launch
            }
            _state.update { it.copy(otaChecking = false, ota = info.copy(mandatory = true)) }
            downloadOta()
        }
    }

    fun otaInstallStarted() = _state.update { it.copy(otaFile = "") }

    fun otaInstallFailed(message: String) =
        _state.update { it.copy(otaFile = "", otaMessage = message) }

    // ── Запись для поддержки ────────────────────────────────────────────
    //
    // Отправка идёт в фоновом потоке: она ходит в сеть, а вызывают её с
    // главного. Ответ показываем строкой под кнопками, как на ПК.

    fun setDiagRecording(on: Boolean) {
        if (on) DiagLog.start() else DiagLog.stop()
        _state.update { it.copy(diagRecording = on, diagMessage = if (on) "Запись идёт. Повторите проблему и нажмите «Отправить отчёт»." else "Запись остановлена.") }
    }

    fun setDiagAuto(on: Boolean) {
        DiagLog.setAutoEnabled(on)
        _state.update {
            it.copy(
                diagAuto = on,
                diagRecording = DiagLog.isActive(),
                diagMessage = if (on) "" else "Журнал больше не отправляется.",
            )
        }
    }

    fun sendDiagReport() {
        if (_state.value.diagBusy) return
        _state.update { it.copy(diagBusy = true, diagMessage = "Отправляем…") }
        viewModelScope.launch {
            val outcome = runCatching { withContext(Dispatchers.IO) { DiagLog.send() } }
            _state.update {
                it.copy(
                    diagBusy = false,
                    diagMessage = outcome.fold(
                        { "Отчёт отправлен. Спасибо — по нему видно, что пошло не так." },
                        { e -> Errors.localize(e.message ?: "Не удалось отправить отчёт.") },
                    ),
                )
            }
        }
    }

    fun switchAccount(email: String) {
        engine.switchAccount(email)
        refreshAuth()
        _state.update {
            it.copy(
                history = emptyList(),
                historyHasMore = false,
                historyNextBatchId = null,
                historySelected = emptySet(),
                historyMessage = "",
                loginMessage = "Выбран Apple ID $email",
            )
        }
        loadCatalog()
    }

    fun removeAccount(email: String) {
        engine.removeAccount(email)
        refreshAuth()
        _state.update {
            it.copy(
                history = emptyList(),
                historyHasMore = false,
                historyNextBatchId = null,
                historySelected = emptySet(),
                loginMessage = "Apple ID $email убран",
            )
        }
    }

    fun clearDiagLog() {
        DiagLog.clear()
        _state.update { it.copy(diagMessage = "Запись очищена.") }
    }

    fun saveSettings(s: AppSettings) = engine.saveSettings(s)

    fun refreshAuth() {
        _state.update { it.copy(auth = engine.authStatus()) }
    }

    // ── Аккаунт: несколько Apple ID и история покупок по страницам ──────

    /** «Добавить Apple ID» при уже вошедшем аккаунте. */
    fun startAddAccount() = _state.update {
        it.copy(
            addingAccount = true,
            loginEmail = "",
            loginPassword = "",
            loginCode = "",
            loginStep = "credentials",
            loginMessage = "",
        )
    }

    /** «Войти снова» для сохранённого Apple ID без действующего входа. */
    fun startRelogin(email: String) = _state.update {
        it.copy(
            addingAccount = true,
            loginEmail = email,
            loginPassword = "",
            loginCode = "",
            loginStep = "credentials",
            loginMessage = "Введите пароль Apple ID $email",
        )
    }

    fun cancelAddAccount() = _state.update {
        it.copy(addingAccount = false, loginStep = "credentials", loginCode = "", loginPassword = "", loginMessage = "")
    }

    /** Вызывается экраном, когда вход завершился: форму добавления закрываем. */
    fun finishAddAccountIfDone() {
        val s = _state.value
        if (s.addingAccount && s.auth.authenticated && s.loginStep == "credentials" && !s.loginBusy &&
            s.auth.email.equals(s.loginEmail.trim(), ignoreCase = true)
        ) {
            _state.update { it.copy(addingAccount = false, loginPassword = "") }
        }
    }

    /**
     * «Загрузить ещё» и «До конца». Apple отдаёт историю страницами; страницу
     * приклеиваем к уже показанному списку без дублей.
     */
    fun loadMoreHistory(all: Boolean) {
        if (_state.value.historyLoadingMore || !_state.value.historyHasMore) return
        viewModelScope.launch {
            _state.update { it.copy(historyLoadingMore = true, historyMessage = "") }
            var pages = 0
            try {
                while (true) {
                    val batch = _state.value.historyNextBatchId ?: break
                    val r = engine.fetchHistory(null, true, batch, false)
                    _state.update {
                        val known = it.history.mapTo(HashSet()) { a -> a.adamId }
                        val merged = it.history + r.apps.filter { a -> known.add(a.adamId) }
                        it.copy(
                            history = merged,
                            historyHasMore = r.hasMore && !r.nextBatchId.isNullOrBlank() && r.nextBatchId != batch,
                            historyNextBatchId = r.nextBatchId,
                            historyMessage = "Загружено покупок: ${merged.size}",
                        )
                    }
                    pages++
                    // Ограничение на случай, если Apple будет возвращать ту же страницу.
                    if (!all || !_state.value.historyHasMore || pages >= 200) break
                }
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                _state.update { it.copy(historyMessage = Errors.localize(e.message ?: "")) }
            } finally {
                _state.update { it.copy(historyLoadingMore = false) }
            }
        }
    }

    // ── Свой файл ──────────────────────────────────────────────────────

    /**
     * Скопировать выбранный IPA к себе и прочитать его Info.plist.
     *
     * Раньше копирование шло прямо в обработчике выбора файла, на главном
     * потоке: файл в сотни мегабайт вешал экран до «Приложение не отвечает».
     * И каждая копия оставалась в кэше навсегда — теперь прежняя удаляется.
     */
    fun pickIpa(ctx: android.content.Context, uri: android.net.Uri) {
        if (_state.value.signBusy) return
        val previous = _state.value.signPath
        viewModelScope.launch {
            _state.update { it.copy(signBusy = true, signMessage = "Копируем файл…", signMeta = null) }
            val outcome = withContext(Dispatchers.IO) {
                runCatching {
                    val name = queryDisplayName(ctx, uri)
                    val dest = java.io.File(ctx.cacheDir, "picked/${System.currentTimeMillis()}-${name.ifBlank { "app.ipa" }}")
                    dest.parentFile?.mkdirs()
                    val input = ctx.contentResolver.openInputStream(uri) ?: error("Не удалось открыть файл")
                    input.use { src -> dest.outputStream().use { src.copyTo(it, 256 * 1024) } }
                    val meta = runCatching { pw.appbank.signum.data.signing.IpaTweaks.metadata(dest) }.getOrElse {
                        dest.delete()
                        error("Это не IPA-файл или он повреждён.")
                    }
                    if (previous.isNotBlank() && previous != dest.absolutePath && previous.contains("/picked")) {
                        runCatching { java.io.File(previous).delete() }
                    }
                    dest to meta
                }
            }
            outcome.onSuccess { (file, meta) ->
                _state.update { it.copy(signBusy = false, signPath = file.absolutePath, signMeta = meta, signMessage = "") }
            }.onFailure { e ->
                _state.update { it.copy(signBusy = false, signMessage = Errors.localize(e.message ?: "Не удалось прочитать файл")) }
            }
        }
    }

    private fun queryDisplayName(ctx: android.content.Context, uri: android.net.Uri): String {
        val raw = runCatching {
            ctx.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)
                ?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
        }.getOrNull() ?: uri.lastPathSegment?.substringAfterLast('/') ?: ""
        return raw.replace(Regex("[^\\p{L}\\p{N}._ -]"), "_").take(80)
    }

    fun clearPickedIpa() {
        val path = _state.value.signPath
        if (path.contains("/picked")) runCatching { java.io.File(path).delete() }
        _state.update { it.copy(signPath = "", signMeta = null, signMessage = "") }
    }

    // ── Настройки ───────────────────────────────────────────────────────

    /** Ручная проверка обновлений — работает и при выключенной автопроверке. */
    fun checkUpdatesNow() {
        if (_state.value.otaChecking) return
        viewModelScope.launch {
            _state.update { it.copy(otaChecking = true, otaCheckMessage = "") }
            val result = runCatching { engine.fetchOta() }
            _state.update { st ->
                result.fold(
                    { info ->
                        if (!info.clientSupported) {
                            st.copy(
                                otaChecking = false,
                                ota = info,
                                clientOutdated = pw.appbank.signum.data.api.ClientOutdated(
                                    message = info.notes?.trim()?.takeIf { n -> n.isNotEmpty() }
                                        ?: "Эта версия Signum устарела. Обновите приложение.",
                                    minClientVersion = info.minClientVersion,
                                ),
                            )
                        } else if (info.hasUpdate && !info.downloadUrl.isNullOrBlank()) {
                            st.copy(otaChecking = false, ota = info, otaCheckMessage = "Доступна версия ${info.version.orEmpty()}".trim())
                        } else {
                            st.copy(otaChecking = false, otaCheckMessage = "Установлена последняя версия")
                        }
                    },
                    { e -> st.copy(otaChecking = false, otaCheckMessage = Errors.localize(e.message ?: "Не удалось проверить обновления")) },
                )
            }
        }
    }

    /** Все наши временные файлы: кэш IPA, подготовленные, выбранные и недокачанные. */
    private fun cacheRoots(): List<java.io.File> = listOf(
        engine.settings.ipaCacheDir(),
        engine.settings.signedDir(),
        java.io.File(engine.settings.cacheDir(), "picked"),
        java.io.File(engine.settings.cacheDir(), "temp"),
    )

    fun refreshCacheSize() {
        viewModelScope.launch {
            val bytes = withContext(Dispatchers.IO) {
                cacheRoots().sumOf { root -> root.walkTopDown().filter { it.isFile }.sumOf { it.length() } }
            }
            _state.update { it.copy(cacheBytes = bytes) }
        }
    }

    /**
     * Очистить кэш. Файлы идущих дел не трогаем: удалить их посреди загрузки
     * или установки — значит уронить дело.
     */
    fun clearCacheNow() {
        val busy = engine.jobs.jobsFlow.value.any { it.status == JobStatus.running || it.status == JobStatus.queued }
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                val keep = _state.value.signPath
                cacheRoots().forEachIndexed { index, root ->
                    // temp — папки идущих загрузок; при активной очереди не трогаем.
                    if (busy && index >= 1) return@forEachIndexed
                    root.listFiles()?.forEach { f ->
                        if (f.absolutePath != keep) f.deleteRecursively()
                    }
                }
            }
            refreshCacheSize()
        }
    }

    class Factory(private val engine: SignumEngine) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T = SignumViewModel(engine) as T
    }
}
