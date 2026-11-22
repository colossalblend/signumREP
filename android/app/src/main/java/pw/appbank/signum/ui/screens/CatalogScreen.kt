package pw.appbank.signum.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.animation.animateContentSize
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil.compose.AsyncImage
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.CheckBoxOutlineBlank
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.NorthEast
import androidx.compose.material.icons.outlined.QrCode2
import androidx.compose.material.icons.outlined.Restore
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.StarBorder
import androidx.compose.material.icons.outlined.Storefront
import androidx.compose.material3.Icon
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import pw.appbank.signum.data.catalog.StoreSearch
import pw.appbank.signum.data.models.AppStoreSearchHit
import pw.appbank.signum.data.models.StoreDeveloper
import pw.appbank.signum.ui.theme.Brand600
import pw.appbank.signum.data.catalog.CatalogResolve
import pw.appbank.signum.data.catalog.CatalogSearch
import pw.appbank.signum.data.catalog.detectBankAlt
import pw.appbank.signum.data.catalog.displayName
import pw.appbank.signum.data.models.AccountRestoreScan
import pw.appbank.signum.data.models.AccountRestoreVersion
import pw.appbank.signum.data.models.AppStoreVersion
import pw.appbank.signum.data.models.CatalogApp
import pw.appbank.signum.data.models.CatalogInstallOffer
import pw.appbank.signum.ui.SignumViewModel
import androidx.compose.ui.graphics.Brush
import pw.appbank.signum.data.models.CatalogBanner
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import pw.appbank.signum.ui.theme.Brand400
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.TestFlightSheet
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.PromoBanner
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.SignumField
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.theme.Brand300
import pw.appbank.signum.ui.theme.Brand500
import pw.appbank.signum.ui.theme.Space800
import pw.appbank.signum.ui.theme.signumColors

@Composable
fun CatalogScreen(vm: SignumViewModel, goInstalls: () -> Unit, goAccount: () -> Unit = {}) {
    val s = vm.state.collectAsState().value
    val c = signumColors
    val apps = vm.visibleApps()
    val sections = vm.catalogSections()
    var catalogSort by rememberSaveable { mutableStateOf("default") }
    var compatibility by rememberSaveable { mutableStateOf("all") }
    val deviceIos = vm.deviceIos()
    val displayedApps = remember(apps, catalogSort, compatibility, deviceIos) {
        // Без подключённого iPhone фильтр совместимости не действует — как на ПК.
        val compatible = if (deviceIos.isNullOrBlank()) apps else when (compatibility) {
            "yes" -> apps.filter { it.min_ios.isNullOrBlank() || CatalogResolve.compareIos(deviceIos, it.min_ios!!) >= 0 }
            "no" -> apps.filter { !it.min_ios.isNullOrBlank() && CatalogResolve.compareIos(deviceIos, it.min_ios!!) < 0 }
            else -> apps
        }
        when (catalogSort) {
            "name" -> compatible.sortedWith(compareBy(RU_COLLATOR) { it.title })
            // Без минимальной iOS — первыми, дальше по номеру, а не по строке:
            // иначе «10.0» встаёт раньше «9.0».
            "ios" -> compatible.sortedWith(Comparator { a, b ->
                val x = a.min_ios?.trim().orEmpty()
                val y = b.min_ios?.trim().orEmpty()
                when {
                    x.isEmpty() && y.isEmpty() -> 0
                    x.isEmpty() -> -1
                    y.isEmpty() -> 1
                    else -> CatalogResolve.compareIos(x, y)
                }
            })
            else -> compatible
        }
    }
    val displayedSections = if (catalogSort == "default" && (compatibility == "all" || deviceIos == null)) sections else null
    // Android 10+ exports to Downloads/Signum through MediaStore. Older
    // versions need a user-selected folder before the first export.
    var exportPending by rememberSaveable { mutableStateOf(false) }
    var devExportPending by rememberSaveable { mutableStateOf(false) }
    val folderPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        if (uri != null) {
            vm.engine.settings.setDownloadsTree(uri)
            if (exportPending && vm.queueOffer(false)) goInstalls()
            if (devExportPending) vm.queueDeveloperPicked(goInstalls)
        }
        exportPending = false
        devExportPending = false
    }
    val tfApp = s.testFlightApp
    if (tfApp != null && s.testFlightUrl.isNotBlank()) {
        // Про TestFlight на телефоне знаем, только если iPhone уже опрошен:
        // пустой список значит «не проверяли», а не «не установлен».
        val onPhone = if (s.installed.isEmpty()) null
        else s.installed.any { it.bundleId == "com.apple.TestFlight" }
        TestFlightSheet(
            title = tfApp.title,
            inviteUrl = s.testFlightUrl,
            testFlightOnPhone = onPhone,
            onClose = vm::closeTestFlight,
        )
    }
    val startExport: () -> Unit = {
        if (!vm.engine.settings.canExportToDownloads()) {
            exportPending = true
            folderPicker.launch(null)
        } else if (vm.queueOffer(false)) {
            goInstalls()
        }
    }
    val favorites = s.settings.favoriteAppIds.toSet()
    val typedQuery = s.search.trim().length >= 2
    // Одно приложение — одна карточка: результат App Store с тем же ID, что у
    // приложения каталога, превращается в карточку каталога, а если она уже
    // показана выше — не дублируется.
    val shownCatalogIds = remember(displayedSections, displayedApps) {
        (displayedSections?.flatMap { it.apps } ?: displayedApps).mapTo(HashSet()) { it.id }
    }
    val mergedStore = remember(s.storeHits, s.catalog, shownCatalogIds) {
        StoreSearch.mergeWithCatalog(s.storeHits, s.catalog, shownCatalogIds)
    }
    val onCatalogCard: (CatalogApp) -> Unit = { app ->
        if (s.selectMode) vm.toggleSelected(app.id) else vm.openOffer(app)
    }
    LazyVerticalGrid(
        columns = GridCells.Adaptive(164.dp),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(18.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Баннеры — первым делом, как на ПК: предупреждение должно попасться
        // на глаза раньше, чем человек полезет ставить приложение.
        if (s.banners.isNotEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    s.banners.forEach { banner -> CatalogBannerCard(banner) }
                    Spacer(Modifier.height(2.dp))
                }
            }
        }
        item(span = { GridItemSpan(maxLineSpan) }) {
            Column {
                PageHeader(
                    title = "Каталог",
                    subtitle = deviceIos?.let { "Банки и важные приложения · iPhone iOS $it" }
                        ?: "Банки и важные приложения для вашего iPhone",
                ) {
                    SecondaryButton("Обновить", onClick = { vm.loadCatalog() })
                }
                Spacer(Modifier.height(14.dp))
                PromoBanner(
                    title = "iPhone по проводу",
                    text = "Подключите кабель, разблокируйте iPhone и нажмите «Доверять».",
                )
                Spacer(Modifier.height(12.dp))
                SignumField(
                    s.search,
                    vm::setSearch,
                    placeholder = "Название, сокращение, ссылка App Store, номер или разработчик",
                )
                if (s.groups.isNotEmpty()) {
                    Spacer(Modifier.height(10.dp))
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip("Все", s.groupFilter == "all") { vm.setGroup("all") }
                        s.groups.sortedByDescending { it.priority ?: 0 }.forEach { g ->
                            FilterChip(g.title, s.groupFilter == g.id) { vm.setGroup(g.id) }
                        }
                    }
                }
                Spacer(Modifier.height(8.dp))
                Row(
                    Modifier.horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Сортировка", color = c.muted, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
                    FilterChip("По каталогу", catalogSort == "default") { catalogSort = "default" }
                    FilterChip("По названию", catalogSort == "name") { catalogSort = "name" }
                    FilterChip("По минимальной iOS", catalogSort == "ios") { catalogSort = "ios" }
                }
                if (deviceIos != null) {
                    Spacer(Modifier.height(8.dp))
                    Row(
                        Modifier.horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("Совместимость", color = c.muted, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
                        FilterChip("Все", compatibility == "all") { compatibility = "all" }
                        FilterChip("Подходит для iOS $deviceIos", compatibility == "yes", tone = ChipTone.Good) { compatibility = "yes" }
                        FilterChip("Не подходит", compatibility == "no", tone = ChipTone.Warn) { compatibility = "no" }
                    }
                }
                StatusLine(s.catalogError, error = true)
                if (s.regionNotice.isNotBlank()) {
                    Spacer(Modifier.height(8.dp))
                    Panel {
                        Text("Страна магазина", color = Color(0xFFFCD34D), fontWeight = FontWeight.Bold, fontSize = 13.sp)
                        Spacer(Modifier.height(6.dp))
                        Text(s.regionNotice, color = c.text, fontSize = 12.sp, lineHeight = 17.sp)
                        Spacer(Modifier.height(10.dp))
                        Text(
                            "Понятно",
                            color = Brand300,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.clickable(onClick = vm::dismissRegionNotice),
                        )
                    }
                }
                Spacer(Modifier.height(10.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (s.selectMode) {
                        Text(
                            if (s.bulkBusy) "Ставим…"
                            else "Установить выбранные" + if (s.selectedAppIds.isEmpty()) "" else " (${s.selectedAppIds.size})",
                            color = if (s.selectedAppIds.isEmpty() || s.bulkBusy) c.muted else Brand300,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.clickable(
                                enabled = s.selectedAppIds.isNotEmpty() && !s.bulkBusy,
                                onClick = { vm.installSelected(goInstalls) },
                            ),
                        )
                        Spacer(Modifier.weight(1f))
                        Text(
                            "Отмена",
                            color = c.muted,
                            fontSize = 12.sp,
                            modifier = Modifier.clickable { vm.setSelectMode(false) },
                        )
                    } else {
                        Text(
                            "Выбрать несколько",
                            color = Brand300,
                            fontSize = 12.sp,
                            modifier = Modifier.clickable { vm.setSelectMode(true) },
                        )
                    }
                }
            }
        }
        when {
            s.catalogLoading && s.catalog.isEmpty() -> {
                // Скелет вместо одинокого спиннера: сетка не прыгает, когда
                // карточки приходят.
                items(6, key = { "skeleton-$it" }) { SkeletonCard() }
            }
            s.catalog.isEmpty() && s.catalogError.isBlank() -> {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    EmptyState(title = "Каталог пока пуст", text = "Нажмите «Обновить», чтобы загрузить его снова.")
                }
            }
            s.catalog.isNotEmpty() && displayedApps.isEmpty() && displayedSections == null -> {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    val bank = detectBankAlt(s.search)
                    EmptyState(
                        title = "Ничего не найдено",
                        text = "Попробуйте полное название, сокращение или английский вариант.",
                    ) {
                        Spacer(Modifier.height(12.dp))
                        Text(
                            "В каталоге собраны проверенные приложения и специальные сборки. " +
                                "Для остальных названий результаты App Store появятся ниже автоматически.",
                            color = c.muted,
                            fontSize = 12.sp,
                            lineHeight = 17.sp,
                            textAlign = TextAlign.Center,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(14.dp))
                                .border(1.dp, c.border, RoundedCornerShape(14.dp))
                                .padding(12.dp),
                        )
                        if (bank != null) {
                            Spacer(Modifier.height(12.dp))
                            Text(
                                "В каталоге нет подходящей версии ${bank.displayName()}. Можно установить другим способом — по номеру телефона.",
                                color = c.text,
                                fontSize = 13.sp,
                                lineHeight = 18.sp,
                                textAlign = TextAlign.Center,
                            )
                            Spacer(Modifier.height(10.dp))
                            PrimaryButton("Альтернативный способ установки", onClick = vm::openBankAlt, modifier = Modifier.fillMaxWidth())
                        }
                    }
                }
            }
        }
        val favoriteApps = if (s.search.isBlank() && catalogSort == "default" && compatibility == "all") {
            s.catalog.filter { it.id in favorites }
        } else emptyList()
        if (favoriteApps.isNotEmpty()) {
            item(key = "g-favorites", span = { GridItemSpan(maxLineSpan) }) {
                SectionHeader("Избранное", favoriteApps.size, star = true)
            }
            items(favoriteApps, key = { "fav-${it.id}" }) { app ->
                CatalogAppCard(
                    app = app,
                    favorite = true,
                    selectable = s.selectMode,
                    selected = app.id in s.selectedAppIds,
                    onToggleFavorite = { vm.toggleFavorite(app.id) },
                    onClick = { onCatalogCard(app) },
                )
            }
        }
        if (displayedSections != null) {
            displayedSections.forEach { section ->
                item(key = "g-${section.id}", span = { GridItemSpan(maxLineSpan) }) {
                    SectionHeader(section.title, section.apps.size)
                }
                items(section.apps, key = { it.id }) { app ->
                    CatalogAppCard(
                        app = app,
                        favorite = app.id in favorites,
                        selectable = s.selectMode,
                        selected = app.id in s.selectedAppIds,
                        onToggleFavorite = { vm.toggleFavorite(app.id) },
                        onClick = { onCatalogCard(app) },
                    )
                }
            }
        } else {
            items(displayedApps, key = { it.id }) { app ->
                CatalogAppCard(
                    app = app,
                    favorite = app.id in favorites,
                    selectable = s.selectMode,
                    selected = app.id in s.selectedAppIds,
                    onToggleFavorite = { vm.toggleFavorite(app.id) },
                    onClick = { onCatalogCard(app) },
                )
            }
        }
        // ── Результаты App Store ─────────────────────────────────────────
        // Раздел есть всегда, когда введено от двух символов, — как на ПК:
        // со спиннером «Ищем…», ошибкой или пустым состоянием.
        if (typedQuery) {
            item(key = "store-title", span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.fillMaxWidth().padding(top = 14.dp)) {
                    Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
                    Spacer(Modifier.height(18.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            "Результаты App Store",
                            color = c.text,
                            fontSize = 18.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.weight(1f),
                        )
                        if (s.storeSearching || s.devSearching) {
                            CircularProgressIndicator(Modifier.size(14.dp), color = Brand300, strokeWidth = 2.dp)
                            Spacer(Modifier.width(6.dp))
                            Text("Ищем…", color = c.muted, fontSize = 12.sp)
                        } else if (mergedStore.isNotEmpty()) {
                            Text("${mergedStore.size}", color = c.muted, fontSize = 11.sp)
                        }
                    }
                    if (s.storeSearchError.isNotBlank()) {
                        Spacer(Modifier.height(10.dp))
                        ErrorBox(s.storeSearchError)
                    }
                }
            }
            if (s.storeSearching && s.storeHits.isEmpty()) {
                items(4, key = { "store-skeleton-$it" }) { SkeletonCard() }
            }
            if (!s.storeSearching && s.storeHits.isNotEmpty() && mergedStore.isEmpty()) {
                item(key = "store-all-merged", span = { GridItemSpan(maxLineSpan) }) {
                    Panel {
                        Text(
                            "Всё, что нашлось в App Store, уже есть в каталоге выше.",
                            color = c.muted,
                            fontSize = 12.sp,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
            }
            if (!s.storeSearching && !s.devSearching && s.storeHits.isEmpty() &&
                s.devHits.isEmpty() && s.storeSearchError.isBlank()
            ) {
                item(key = "store-empty", span = { GridItemSpan(maxLineSpan) }) {
                    EmptyState(
                        title = "Ничего не найдено",
                        text = "Попробуйте другое название, ссылку из App Store или номер.",
                    )
                }
            }
            items(mergedStore, key = { m -> m.catalog?.let { "sc-${it.id}" } ?: "s${m.hit.adamId}" }) { m ->
                val catalogApp = m.catalog
                if (catalogApp != null) {
                    CatalogAppCard(
                        app = catalogApp,
                        favorite = catalogApp.id in favorites,
                        onAccount = m.hit.foundOnAccount,
                        onToggleFavorite = { vm.toggleFavorite(catalogApp.id) },
                        onClick = { vm.openOffer(catalogApp) },
                    )
                } else {
                    StoreHitCard(m.hit) { vm.openStoreHit(m.hit) }
                }
            }
            // Разработчики — под приложениями, как на ПК: сначала то, что
            // искали, потом «а кто это сделал и что ещё у него есть».
            if (s.devHits.isNotEmpty()) {
                item(key = "dev-title", span = { GridItemSpan(maxLineSpan) }) {
                    Row(Modifier.fillMaxWidth().padding(top = 12.dp), verticalAlignment = Alignment.Bottom) {
                        Text("Разработчики", color = c.text, fontSize = 16.sp, fontWeight = FontWeight.Bold)
                        Spacer(Modifier.width(8.dp))
                        Text("${s.devHits.size} ${pluralDevs(s.devHits.size)}", color = c.muted, fontSize = 12.sp)
                    }
                }
                items(s.devHits, key = { "dev-${it.artistId}" }, span = { GridItemSpan(maxLineSpan) }) { dev ->
                    DeveloperRow(dev) { vm.openDeveloper(dev) }
                }
            }
        }
    }


    if (s.offerLoading) {
        Dialog(onDismissRequest = {}) {
            SolidCard {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    CircularProgressIndicator(Modifier.size(22.dp), color = Brand300, strokeWidth = 2.dp)
                    Text("Открываем приложение…", color = Color(0xFFF8FAFC), fontSize = 14.sp)
                }
            }
        }
    }

    if (s.offerError.isNotBlank() && s.offer == null && !s.offerLoading) {
        Dialog(onDismissRequest = vm::closeOfferError) {
            SolidCard {
                Text("Не удалось подобрать версию", color = Color(0xFFFECACA), fontWeight = FontWeight.Bold, fontSize = 17.sp)
                Spacer(Modifier.height(10.dp))
                Text(s.offerError, color = Color(0xFFE2E8F0), fontSize = 14.sp, lineHeight = 20.sp)
                Spacer(Modifier.height(14.dp))
                SecondaryButton("Закрыть", onClick = vm::closeOfferError, modifier = Modifier.fillMaxWidth())
            }
        }
    }

    s.offer?.let { offer ->
        Dialog(
            onDismissRequest = vm::closeOffer,
            properties = DialogProperties(usePlatformDefaultWidth = true),
        ) {
            val offerActivation = remember(offer.appId, s.catalog) { vm.activationFor(offer.appId) }
            OfferDialog(
                offer = offer,
                versions = s.versions,
                versionsLoading = s.versionsLoading,
                versionsError = s.versionsError,
                versionsSearch = s.versionsSearch,
                versionsVisibleCount = s.versionsVisibleCount,
                chosenStore = s.chosenVersion,
                iosPending = s.versionsIosPending,
                error = s.offerError,
                showRestore = offer.showAccountVersions,
                onChooseStore = vm::chooseVersion,
                onSearchVersions = vm::setVersionsSearch,
                onMoreVersions = vm::showMoreVersions,
                onRefreshVersions = vm::refreshStoreVersions,
                onPickForDevice = vm::pickVersionForDevice,
                versionFitBusy = s.versionFitBusy,
                onInstall = { if (vm.queueOffer(true)) goInstalls() },
                onExport = startExport,
                onBankAlt = vm::openBankAlt,
                activationCount = offerActivation?.items?.size ?: 0,
                onActivation = { offerActivation?.let { vm.openActivation(offer.appTitle, it) } },
                onOtherVersions = vm::openRestoreVersions,
                onAccount = goAccount,
                onClose = vm::closeOffer,
            )
        }
    }

    if (s.restoreOpen && s.offer != null) {
        Dialog(
            onDismissRequest = { if (!s.restoreLoading) vm.closeRestoreVersions() },
            properties = DialogProperties(usePlatformDefaultWidth = true),
        ) {
            RestoreDialog(
                appTitle = s.offer.appTitle,
                scan = s.restoreScan,
                loading = s.restoreLoading,
                search = s.restoreSearch,
                visibleCount = s.restoreVisibleCount,
                onSearch = vm::setRestoreSearch,
                onMore = vm::showMoreRestore,
                onRefresh = vm::refreshRestoreVersions,
                onPick = vm::chooseRestoreItem,
                onClose = vm::closeRestoreVersions,
            )
        }
    }

    val openedDev = s.openedDeveloper
    if (openedDev != null) {
        Dialog(
            onDismissRequest = { if (!s.developerBatchBusy) vm.closeDeveloper() },
            properties = DialogProperties(usePlatformDefaultWidth = true),
        ) {
            DeveloperDialog(
                dev = openedDev,
                apps = s.developerApps,
                loading = s.developerLoading,
                error = s.developerError,
                picked = s.developerPicked,
                batchBusy = s.developerBatchBusy,
                onTogglePick = vm::toggleDeveloperPick,
                onToggleAll = vm::toggleDeveloperPickAll,
                onDownloadPicked = {
                    if (!vm.engine.settings.canExportToDownloads()) {
                        devExportPending = true
                        folderPicker.launch(null)
                    } else {
                        vm.queueDeveloperPicked(goInstalls)
                    }
                },
                onOpen = { hit ->
                    vm.closeDeveloper()
                    vm.openStoreHit(hit)
                },
                onClose = vm::closeDeveloper,
            )
        }
    }
    if (s.bankAltOpen) {
        val bank = detectBankAlt(
            s.offer?.appId,
            s.offer?.appTitle,
            s.offer?.title,
            *(s.offer?.aliases?.toTypedArray() ?: emptyArray()),
            s.search,
        )
        Dialog(onDismissRequest = vm::closeBankAlt) {
            SolidCard {
                Text(
                    "Альтернативный способ" + (bank?.let { " · ${it.displayName()}" } ?: ""),
                    color = Color(0xFFF8FAFC),
                    fontWeight = FontWeight.Bold,
                    fontSize = 17.sp,
                )
                Spacer(Modifier.height(10.dp))
                if (bank == null) {
                    Text(
                        "Этот способ есть только у Совкомбанка и Рокетбанка. Для остальных выберите " +
                            "другую версию в списке или поставьте приложение из покупок Apple ID.",
                        color = Color(0xFFE2E8F0),
                        fontSize = 14.sp,
                        lineHeight = 20.sp,
                    )
                    Spacer(Modifier.height(14.dp))
                    SecondaryButton("Понятно", onClick = vm::closeBankAlt, modifier = Modifier.fillMaxWidth())
                } else {
                    when (s.bankAltStep) {
                        "phone" -> {
                            Text(
                                "Банк пришлёт SMS и выдаст свой Apple ID на полчаса. Введите номер, " +
                                    "на который оформлена карта.",
                                color = Color(0xFFE2E8F0),
                                fontSize = 14.sp,
                                lineHeight = 20.sp,
                            )
                            Spacer(Modifier.height(10.dp))
                            SignumField(
                                s.bankAltPhone,
                                vm::setBankAltPhone,
                                placeholder = "+7 999 123-45-67",
                                keyboard = KeyboardOptions(keyboardType = KeyboardType.Phone),
                            )
                            Spacer(Modifier.height(10.dp))
                            PrimaryButton(
                                "Получить код",
                                onClick = { vm.bankAltSendSms(bank.name.lowercase()) },
                                loading = s.bankAltBusy,
                                modifier = Modifier.fillMaxWidth(),
                            )
                        }
                        "code" -> {
                            Text(
                                "Введите код из SMS.",
                                color = Color(0xFFE2E8F0),
                                fontSize = 14.sp,
                            )
                            Spacer(Modifier.height(10.dp))
                            SignumField(
                                s.bankAltCode,
                                vm::setBankAltCode,
                                placeholder = "Код из SMS",
                                keyboard = KeyboardOptions(keyboardType = KeyboardType.Number),
                            )
                            Spacer(Modifier.height(10.dp))
                            PrimaryButton(
                                "Подтвердить",
                                onClick = vm::bankAltConfirm,
                                loading = s.bankAltBusy,
                                modifier = Modifier.fillMaxWidth(),
                            )
                        }
                        else -> {
                            // Логин и пароль показываем крупно и с возможностью
                            // скопировать: их придётся вводить на самом iPhone.
                            Text(
                                "Войдите этим Apple ID на iPhone: App Store → аккаунт → выйти → войти.",
                                color = Color(0xFFE2E8F0),
                                fontSize = 14.sp,
                                lineHeight = 20.sp,
                            )
                            Spacer(Modifier.height(10.dp))
                            CopyRow("Логин", s.bankAltLogin)
                            Spacer(Modifier.height(6.dp))
                            CopyRow("Пароль", s.bankAltPassword)
                            if (s.bankAltLink.isNotBlank()) {
                                Spacer(Modifier.height(6.dp))
                                CopyRow("Ссылка", s.bankAltLink)
                            }
                        }
                    }
                    if (s.bankAltMessage.isNotBlank()) {
                        Spacer(Modifier.height(10.dp))
                        Text(s.bankAltMessage, color = Color(0xFF94A3B8), fontSize = 12.sp, lineHeight = 17.sp)
                    }
                    Spacer(Modifier.height(10.dp))
                    SecondaryButton("Закрыть", onClick = vm::closeBankAlt, modifier = Modifier.fillMaxWidth())
                }
            }
        }
    }
}

@Composable
private fun OfferDialog(
    offer: CatalogInstallOffer,
    versions: List<AppStoreVersion>,
    versionsLoading: Boolean,
    versionsError: String,
    versionsSearch: String,
    versionsVisibleCount: Int,
    chosenStore: AppStoreVersion?,
    iosPending: Set<String>,
    error: String,
    showRestore: Boolean,
    onChooseStore: (AppStoreVersion) -> Unit,
    onSearchVersions: (String) -> Unit,
    onMoreVersions: () -> Unit,
    onRefreshVersions: () -> Unit,
    onPickForDevice: () -> Unit,
    versionFitBusy: Boolean,
    onInstall: () -> Unit,
    onExport: () -> Unit,
    onBankAlt: () -> Unit,
    activationCount: Int,
    onActivation: () -> Unit,
    onOtherVersions: () -> Unit,
    onAccount: () -> Unit,
    onClose: () -> Unit,
) {
    val body = Color(0xFFE2E8F0)
    val titleColor = Color(0xFFF8FAFC)
    val bank = detectBankAlt(offer.appId, offer.appTitle, offer.title, *offer.aliases.toTypedArray())
    val filtered = versions.filter {
        CatalogResolve.versionMatchesSearch(it.version, it.externalVersionId, versionsSearch)
    }
    SolidCard {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (!offer.iconUrl.isNullOrBlank()) {
                AsyncImage(
                    offer.iconUrl,
                    contentDescription = null,
                    modifier = Modifier.size(56.dp).clip(RoundedCornerShape(14.dp)),
                    contentScale = ContentScale.Crop,
                )
            } else {
                Box(
                    Modifier.size(56.dp).clip(RoundedCornerShape(14.dp)).background(Brand500),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(offer.appTitle.take(1), color = Color.White, fontWeight = FontWeight.Black, fontSize = 22.sp)
                }
            }
            Spacer(Modifier.size(12.dp))
            Column(Modifier.weight(1f)) {
                Text(offer.appTitle, color = titleColor, fontWeight = FontWeight.Bold, fontSize = 18.sp, maxLines = 2)
                Text(
                    listOfNotNull(
                        offer.title.takeIf { it.isNotBlank() && it != offer.appTitle },
                        CatalogSearch.formatVersion(offer.version, offer.minIos),
                    ).joinToString(" · "),
                    color = body,
                    fontSize = 12.sp,
                    maxLines = 2,
                )
            }
        }
        Spacer(Modifier.height(12.dp))
        val store = offer.source == "app_store"
        Box(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(12.dp))
                .background(if (store) Color(0x3322C55E) else Color(0x33F59E0B))
                .padding(12.dp),
        ) {
            Column {
                Text(
                    if (store) "App Store" else "Восстановление из покупок",
                    color = titleColor,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 14.sp,
                )
                Spacer(Modifier.height(4.dp))
                Text(offer.message, color = body, fontSize = 13.sp, lineHeight = 18.sp)
                if (!offer.minIos.isNullOrBlank()) {
                    Spacer(Modifier.height(4.dp))
                    Text("Минимальная система: iOS ${offer.minIos}", color = body, fontSize = 12.sp)
                }
            }
        }
        if (offer.deviceCompatible == false) {
            Spacer(Modifier.height(10.dp))
            Box(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(Color(0x33F59E0B))
                    .padding(12.dp),
            ) {
                Text(
                    "iPhone не поддерживает это приложение. Нужна iOS ${offer.minIos}${offer.deviceIos?.let { ", у вас $it" } ?: ""}. Установить можно, но приложение не запустится.",
                    color = Color(0xFFFDE68A),
                    fontSize = 13.sp,
                    lineHeight = 18.sp,
                )
            }
        }
        if (error.isNotBlank()) {
            Spacer(Modifier.height(10.dp))
            Text(error, color = Color(0xFFFECACA), fontSize = 13.sp, lineHeight = 18.sp)
        }
        Spacer(Modifier.height(14.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(12.dp))
                .border(1.dp, Color(0x22FFFFFF), RoundedCornerShape(12.dp))
                .padding(12.dp),
        ) {
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        buildString {
                            append("Версия для скачивания")
                            if (!versionsLoading && versions.isNotEmpty()) {
                                val shown = minOf(versionsVisibleCount, filtered.size)
                                append("  ")
                                append(
                                    if (versionsSearch.isNotBlank()) "$shown/${filtered.size} (из ${versions.size})"
                                    else "$shown/${versions.size}",
                                )
                            }
                        },
                        color = titleColor,
                        fontWeight = FontWeight.SemiBold,
                        fontSize = 13.sp,
                        modifier = Modifier.weight(1f),
                    )
                    if (!offer.deviceIos.isNullOrBlank() && !offer.appStoreId.isNullOrBlank()) {
                        val fitEnabled = !versionFitBusy && !versionsLoading
                        Text(
                            if (versionFitBusy) "Подбираем…" else "Подобрать",
                            color = if (fitEnabled) Brand300 else Color(0xFF64748B),
                            fontSize = 11.sp,
                            modifier = Modifier
                                .clickable(enabled = fitEnabled, onClick = onPickForDevice)
                                .padding(end = 12.dp),
                        )
                    }
                    Text(
                        "Обновить",
                        color = if (versionsLoading || offer.appStoreId.isNullOrBlank()) Color(0xFF64748B) else Brand300,
                        fontSize = 11.sp,
                        modifier = Modifier.clickable(enabled = !versionsLoading && !offer.appStoreId.isNullOrBlank(), onClick = onRefreshVersions),
                    )
                }
                when {
                    versionsLoading -> {
                        Spacer(Modifier.height(10.dp))
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            CircularProgressIndicator(Modifier.size(14.dp), color = Brand300, strokeWidth = 2.dp)
                            Text("Загружаем список версий…", color = body, fontSize = 12.sp)
                        }
                    }
                    versionsError.isNotBlank() -> {
                        Spacer(Modifier.height(8.dp))
                        Text(versionsError, color = Color(0xFFFECACA), fontSize = 12.sp, lineHeight = 17.sp)
                        if (versionsError.contains("войдите", true) || versionsError.contains("аккаунт", true) || versionsError.contains("сессия", true) || versionsError.contains("token", true)) {
                            Spacer(Modifier.height(6.dp))
                            Text("Открыть «Аккаунт»", color = Brand300, fontSize = 12.sp, modifier = Modifier.clickable(onClick = onAccount))
                        }
                    }
                    versions.isEmpty() -> {
                        Spacer(Modifier.height(8.dp))
                        Text(
                            "Приложения ещё нет в покупках этого Apple ID — история версий появится после установки. Сейчас будет скачана последняя версия. Если покупали на другом Apple ID, войдите в него на странице «Аккаунт» и обновите список.",
                            color = body,
                            fontSize = 12.sp,
                            lineHeight = 17.sp,
                        )
                    }
                    else -> {
                        Spacer(Modifier.height(8.dp))
                        SignumField(versionsSearch, onSearchVersions, placeholder = "Поиск")
                        val page = filtered.take(versionsVisibleCount)
                        if (page.isEmpty()) {
                            Spacer(Modifier.height(6.dp))
                            Text("Ничего не найдено по «${versionsSearch.trim()}».", color = body, fontSize = 12.sp)
                        } else {
                            page.forEach { version ->
                                VersionRow(
                                    title = CatalogResolve.formatAppStoreVersionLabel(version.version, version.externalVersionId),
                                    subtitle = buildString {
                                        append(version.externalVersionId)
                                        if (version.isLatest) append(" · Последняя")
                                        // «В покупках» построчно не пишем: покупка на аккаунте
                                        // одна на все версии сразу, и метка ничего не значила.
                                        version.minIos?.trim()?.takeIf { it.isNotEmpty() }?.let { need ->
                                            append(" · iOS $need+")
                                            val have = offer.deviceIos?.trim().orEmpty()
                                            if (have.isNotEmpty() && CatalogResolve.compareIos(have, need) < 0) {
                                                append(" · не запустится")
                                            }
                                        }
                                    },
                                    selected = chosenStore?.externalVersionId == version.externalVersionId,
                                    iosLoading = version.minIos.isNullOrBlank() && version.externalVersionId in iosPending,
                                    onClick = { onChooseStore(version) },
                                )
                            }
                            if (versionsVisibleCount < filtered.size) {
                                Spacer(Modifier.height(6.dp))
                                SecondaryButton(
                                    "Ещё ${minOf(12, filtered.size - versionsVisibleCount)} из ${filtered.size - versionsVisibleCount}",
                                    onClick = onMoreVersions,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                            }
                        }
                    }
                }
            }
        }
        Spacer(Modifier.height(14.dp))
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            PrimaryButton(
                if (offer.installedOnDevice) "Переустановить на iPhone" else "Установить на iPhone",
                onClick = onInstall,
                modifier = Modifier.fillMaxWidth(),
            )
            SecondaryButton("Скачать файл", onClick = onExport, modifier = Modifier.fillMaxWidth())
            if (bank != null) {
                SecondaryButton(
                    "Альтернативный способ · ${bank.displayName()}",
                    onClick = onBankAlt,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            if (activationCount > 0) {
                SecondaryButton(
                    if (activationCount == 1) "Активация · QR" else "Активация · все QR ($activationCount)",
                    onClick = onActivation,
                    icon = Icons.Outlined.QrCode2,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            if (showRestore) {
                SecondaryButton("Выбрать другую версию", onClick = onOtherVersions, modifier = Modifier.fillMaxWidth())
            }
            SecondaryButton("Отмена", onClick = onClose, modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun RestoreDialog(
    appTitle: String,
    scan: AccountRestoreScan?,
    loading: Boolean,
    search: String,
    visibleCount: Int,
    onSearch: (String) -> Unit,
    onMore: () -> Unit,
    onRefresh: () -> Unit,
    onPick: (AccountRestoreVersion) -> Unit,
    onClose: () -> Unit,
) {
    val titleColor = Color(0xFFF8FAFC)
    val body = Color(0xFFE2E8F0)
    val items = scan?.items.orEmpty()
    val filtered = items.filter { item ->
        val q = search.trim().lowercase()
        if (q.isEmpty()) true
        else listOf(item.version, item.appStoreId, item.bundleId, item.title, item.description)
            .filterNotNull()
            .joinToString(" ")
            .lowercase()
            .contains(q) || (item.appStoreId ?: "").contains(q.replace(" ", ""))
    }
    SolidCard {
        Text("Другие версии · $appTitle", color = titleColor, fontWeight = FontWeight.Bold, fontSize = 18.sp)
        Spacer(Modifier.height(4.dp))
        Text(
            when {
                loading -> "Ищем версии в покупках…"
                !scan?.deviceIos.isNullOrBlank() -> "Только версии из покупок этого Apple ID · iPhone iOS ${scan?.deviceIos}"
                else -> "Только версии из покупок этого Apple ID. Подключите iPhone, чтобы проверить совместимость."
            },
            color = body,
            fontSize = 12.sp,
            lineHeight = 17.sp,
        )
        if (!loading) {
            Spacer(Modifier.height(4.dp))
            Text("Обновить список", color = Brand300, fontSize = 12.sp, modifier = Modifier.clickable(onClick = onRefresh))
        }
        Spacer(Modifier.height(12.dp))
        if (loading) {
            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                CircularProgressIndicator(color = Brand300)
                Spacer(Modifier.height(10.dp))
                Text("Ищем версии в покупках…", color = body, fontSize = 13.sp)
            }
        } else if (items.isEmpty()) {
            Text(
                scan?.message?.trim().orEmpty().ifEmpty { "Других версий этого приложения в покупках Apple ID не найдено." },
                color = body,
                fontSize = 13.sp,
                lineHeight = 18.sp,
            )
        } else {
            SignumField(search, onSearch, placeholder = "Поиск")
            Spacer(Modifier.height(6.dp))
            Text(
                if (filtered.isEmpty()) "0" else "${minOf(visibleCount, filtered.size)} из ${filtered.size}",
                color = Color(0xFF94A3B8),
                fontSize = 11.sp,
            )
            if (filtered.isEmpty()) {
                Spacer(Modifier.height(8.dp))
                Text("Ничего не найдено по «${search.trim()}».", color = body, fontSize = 13.sp)
            } else {
                filtered.take(visibleCount).forEach { item ->
                    RestoreItemRow(item, scan?.deviceIos, onPick)
                }
                if (visibleCount < filtered.size) {
                    Spacer(Modifier.height(8.dp))
                    SecondaryButton(
                        "Ещё ${minOf(12, filtered.size - visibleCount)} из ${filtered.size - visibleCount}",
                        onClick = onMore,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        SecondaryButton("Назад", onClick = onClose, modifier = Modifier.fillMaxWidth(), enabled = !loading)
    }
}

@Composable
private fun RestoreItemRow(
    item: AccountRestoreVersion,
    deviceIos: String?,
    onPick: (AccountRestoreVersion) -> Unit,
) {
    val titleColor = Color(0xFFF8FAFC)
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(Color(0x14FFFFFF))
            .clickable { onPick(item) }
            .padding(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (!item.iconUrl.isNullOrBlank()) {
            AsyncImage(
                item.iconUrl,
                contentDescription = null,
                modifier = Modifier.size(44.dp).clip(RoundedCornerShape(10.dp)),
                contentScale = ContentScale.Crop,
            )
        } else {
            Box(
                Modifier.size(44.dp).clip(RoundedCornerShape(10.dp)).background(Brand500),
                contentAlignment = Alignment.Center,
            ) {
                Text(item.appTitle.take(1), color = Color.White, fontWeight = FontWeight.Black)
            }
        }
        Spacer(Modifier.size(10.dp))
        Column(Modifier.weight(1f)) {
            Text(item.title, color = titleColor, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(CatalogSearch.formatVersion(item.version, item.minIos), color = Brand300, fontSize = 12.sp)
            if (item.installedOnDevice) {
                Text("Уже на iPhone" + (item.installedVersion?.let { " · v$it" } ?: ""), color = Color(0xFF7DD3FC), fontSize = 11.sp)
            }
            if (item.deviceCompatible == false) {
                Text("Не запустится на iOS ${deviceIos ?: "устройства"}", color = Color(0xFFFCD34D), fontSize = 11.sp)
            }
        }
        Column(horizontalAlignment = Alignment.End) {
            Badge(if (item.inStore) "App Store" else "Не в Store", if (item.inStore) Color(0xFF6EE7B7) else Color(0xFF94A3B8))
            if (item.installedOnDevice) Badge("Установлено", Color(0xFF7DD3FC))
            if (item.deviceCompatible == true) Badge("Подходит", Color(0xFF6EE7B7))
            if (item.deviceCompatible == false) Badge("Не запустится", Color(0xFFFCD34D))
        }
    }
}

@Composable
private fun Badge(text: String, color: Color) {
    Text(
        text,
        color = color,
        fontSize = 9.sp,
        fontWeight = FontWeight.Bold,
        modifier = Modifier.padding(top = 2.dp),
    )
}

@Composable
private fun VersionRow(title: String, subtitle: String, selected: Boolean, iosLoading: Boolean = false, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .background(if (selected) Brand500.copy(0.18f) else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(18.dp)
                .border(2.dp, if (selected) Brand300 else Color(0xFF94A3B8), CircleShape)
                .padding(3.dp),
        ) {
            if (selected) {
                Box(Modifier.fillMaxSize().clip(CircleShape).background(Brand300))
            }
        }
        Spacer(Modifier.size(10.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = Color(0xFFF8FAFC), fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(subtitle, color = Color(0xFFCBD5E1), fontSize = 11.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        if (iosLoading) {
            // Совместимость ещё уточняется — как «iOS…» на ПК.
            Spacer(Modifier.size(8.dp))
            CircularProgressIndicator(Modifier.size(12.dp), color = Color(0xFF94A3B8), strokeWidth = 1.5.dp)
            Spacer(Modifier.size(4.dp))
            Text("iOS…", color = Color(0xFF94A3B8), fontSize = 11.sp)
        }
    }
}

@Composable
private fun SolidCard(content: @Composable () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(max = 640.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(Space800)
            .border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(20.dp))
            .padding(18.dp)
            .verticalScroll(rememberScrollState()),
    ) { content() }
}

private val RU_COLLATOR: java.text.Collator = java.text.Collator.getInstance(java.util.Locale.forLanguageTag("ru"))

/** Тон чипа фильтра: обычный, «подходит» (зелёный), «не подходит» (янтарный) — как на ПК. */
private enum class ChipTone { Brand, Good, Warn }

@Composable
private fun toneColor(tone: ChipTone): Color {
    val dark = signumColors.isDark
    return when (tone) {
        ChipTone.Brand -> if (dark) Brand300 else Brand600
        ChipTone.Good -> if (dark) Color(0xFF6EE7B7) else Color(0xFF047857)
        ChipTone.Warn -> if (dark) Color(0xFFFCD34D) else Color(0xFFB45309)
    }
}

@Composable
private fun FilterChip(label: String, selected: Boolean, tone: ChipTone = ChipTone.Brand, onClick: () -> Unit) {
    val c = signumColors
    val shape = RoundedCornerShape(12.dp)
    val accent = toneColor(tone)
    Text(
        label,
        color = if (selected) accent else c.muted,
        fontSize = 12.sp,
        fontWeight = FontWeight.SemiBold,
        maxLines = 1,
        modifier = Modifier
            .clip(shape)
            .background(if (selected) accent.copy(alpha = 0.15f) else c.text.copy(alpha = 0.04f))
            .border(1.dp, if (selected) accent.copy(alpha = 0.3f) else Color.Transparent, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
    )
}

@Composable
private fun SectionHeader(title: String, count: Int, star: Boolean = false) {
    val c = signumColors
    Row(
        Modifier.fillMaxWidth().padding(top = 10.dp, bottom = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (star) {
            Icon(Icons.Filled.Star, contentDescription = null, tint = Color(0xFFFCD34D), modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
        }
        Text(title, color = c.text, fontSize = 18.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
        Text("$count", color = c.muted, fontSize = 11.sp)
    }
}

/** Заглушка карточки, пока грузится каталог или идёт поиск. */
@Composable
private fun SkeletonCard() {
    val c = signumColors
    val pulse = rememberInfiniteTransition(label = "skeleton")
    val alpha by pulse.animateFloat(
        initialValue = 0.05f,
        targetValue = 0.12f,
        animationSpec = infiniteRepeatable(tween(850), RepeatMode.Reverse),
        label = "skeleton-alpha",
    )
    val block = c.text.copy(alpha = alpha)
    Panel {
        Column(Modifier.fillMaxWidth().heightIn(min = CARD_MIN_HEIGHT)) {
            Box(Modifier.size(ICON_SIZE).clip(RoundedCornerShape(ICON_CORNER)).background(block))
            Spacer(Modifier.height(22.dp))
            Box(Modifier.fillMaxWidth(0.8f).height(14.dp).clip(RoundedCornerShape(6.dp)).background(block))
            Spacer(Modifier.height(8.dp))
            Box(Modifier.fillMaxWidth(0.5f).height(10.dp).clip(RoundedCornerShape(6.dp)).background(block))
        }
    }
}

@Composable
private fun EmptyState(title: String, text: String, extra: @Composable ColumnScope.() -> Unit = {}) {
    val c = signumColors
    Panel {
        Column(Modifier.fillMaxWidth().padding(vertical = 14.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Icon(Icons.Outlined.Search, contentDescription = null, tint = c.muted.copy(alpha = 0.7f), modifier = Modifier.size(28.dp))
            Spacer(Modifier.height(10.dp))
            Text(title, color = c.text, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, textAlign = TextAlign.Center)
            Spacer(Modifier.height(4.dp))
            Text(text, color = c.muted, fontSize = 12.sp, lineHeight = 17.sp, textAlign = TextAlign.Center)
            extra()
        }
    }
}

@Composable
private fun ErrorBox(text: String) {
    val color = if (signumColors.isDark) Color(0xFFFDA4AF) else Color(0xFFBE123C)
    Text(
        text,
        color = color,
        fontSize = 12.sp,
        lineHeight = 17.sp,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(color.copy(alpha = 0.08f))
            .border(1.dp, color.copy(alpha = 0.2f), RoundedCornerShape(12.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp),
    )
}

private val ICON_SIZE = 72.dp
private val ICON_CORNER = 18.dp
private val CARD_MIN_HEIGHT = 196.dp

/** `#5b6cf0` из каталога → цвет; без цвета — фирменный, как на ПК. */
private fun accentColor(raw: String?): Color {
    val hex = raw?.trim()?.removePrefix("#").orEmpty()
    val value = when (hex.length) {
        6 -> hex.toLongOrNull(16)?.let { 0xFF000000 or it }
        8 -> hex.toLongOrNull(16)
        else -> null
    } ?: return Brand500
    return Color(value)
}

/**
 * Значок приложения со скруглением и мягкой подсветкой цвета приложения.
 * Не загрузился — первая буква на фирменном градиенте, как на ПК.
 */
@Composable
private fun AppIcon(url: String?, title: String, accent: Color, size: Dp = ICON_SIZE, corner: Dp = ICON_CORNER, glow: Boolean = true) {
    val c = signumColors
    var broken by remember(url) { mutableStateOf(false) }
    Box(contentAlignment = Alignment.Center) {
        if (glow) {
            Box(
                Modifier
                    .size(size + 18.dp)
                    .background(Brush.radialGradient(listOf(accent.copy(alpha = 0.35f), Color.Transparent))),
            )
        }
        val shape = RoundedCornerShape(corner)
        if (url.isNullOrBlank() || broken) {
            Box(
                Modifier.size(size).clip(shape).background(c.brandBrush),
                contentAlignment = Alignment.Center,
            ) {
                Text(title.take(1), color = Color.White, fontWeight = FontWeight.Black, fontSize = (size.value * 0.36f).sp)
            }
        } else {
            AsyncImage(
                url,
                contentDescription = null,
                onError = { broken = true },
                modifier = Modifier
                    .size(size)
                    .clip(shape)
                    .border(1.dp, Color.White.copy(alpha = 0.16f), shape),
                contentScale = ContentScale.Crop,
            )
        }
    }
}

private enum class BadgeTone { Good, Warn, Violet }

@Composable
private fun CardBadge(text: String, icon: ImageVector, tone: BadgeTone) {
    val dark = signumColors.isDark
    val color = when (tone) {
        BadgeTone.Good -> if (dark) Color(0xFF6EE7B7) else Color(0xFF047857)
        BadgeTone.Warn -> if (dark) Color(0xFFFCD34D) else Color(0xFFB45309)
        BadgeTone.Violet -> if (dark) Color(0xFFC4B5FD) else Color(0xFF6D28D9)
    }
    val shape = RoundedCornerShape(8.dp)
    Row(
        Modifier
            .clip(shape)
            .background(color.copy(alpha = 0.12f))
            .border(1.dp, color.copy(alpha = 0.22f), shape)
            .padding(horizontal = 7.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = color, modifier = Modifier.size(11.dp))
        Spacer(Modifier.width(4.dp))
        Text(
            text.uppercase(),
            color = color,
            fontSize = 9.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = 0.6.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** Квадратная кнопка в углу карточки: избранное, выбор, «открыть». */
@Composable
private fun CornerButton(
    icon: ImageVector,
    tint: Color,
    background: Color,
    contentDescription: String?,
    onClick: (() -> Unit)? = null,
) {
    Box(
        Modifier
            .size(32.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(background)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = contentDescription, tint = tint, modifier = Modifier.size(16.dp))
    }
}

/**
 * Карточка приложения каталога — та же, что `AppCard` на ПК: значок с
 * подсветкой, метка «App Store» / «Из покупок», «Активация», название,
 * версия, звезда избранного и стрелка (или отметка в режиме выбора).
 */
@Composable
private fun CatalogAppCard(
    app: CatalogApp,
    favorite: Boolean = false,
    selectable: Boolean = false,
    selected: Boolean = false,
    onAccount: Boolean = false,
    onToggleFavorite: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val c = signumColors
    val store = app.in_store == true
    val accent = remember(app.color) { accentColor(app.color) }
    val ring = if (selected) Modifier.border(2.dp, Brand400, RoundedCornerShape(18.dp)) else Modifier
    Panel(modifier = ring, onClick = onClick) {
        Column(Modifier.fillMaxWidth().heightIn(min = CARD_MIN_HEIGHT)) {
            Row(verticalAlignment = Alignment.Top) {
                AppIcon(app.icon_url, app.title, accent)
                Spacer(Modifier.weight(1f))
                Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(5.dp)) {
                    if (store) CardBadge("App Store", Icons.Outlined.Storefront, BadgeTone.Good)
                    else CardBadge("Из покупок", Icons.Outlined.Restore, BadgeTone.Warn)
                    if (app.activation != null) CardBadge("Активация", Icons.Outlined.QrCode2, BadgeTone.Violet)
                    if (onAccount) CardBadge("Есть на Apple ID", Icons.Outlined.Restore, BadgeTone.Good)
                }
            }
            Spacer(Modifier.height(16.dp))
            Text(
                app.title,
                color = c.text,
                fontWeight = FontWeight.Bold,
                fontSize = 16.sp,
                lineHeight = 20.sp,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(3.dp))
            Text(
                CatalogSearch.formatVersion(app.version, app.min_ios),
                color = c.muted,
                fontSize = 11.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.weight(1f))
            Spacer(Modifier.height(14.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (onToggleFavorite != null) {
                    CornerButton(
                        icon = if (favorite) Icons.Filled.Star else Icons.Outlined.StarBorder,
                        tint = if (favorite) Color(0xFFFCD34D) else c.muted,
                        background = if (favorite) Color(0x26FBBF24) else c.text.copy(alpha = 0.05f),
                        contentDescription = if (favorite) "Убрать из избранного" else "В избранное",
                        onClick = onToggleFavorite,
                    )
                }
                Spacer(Modifier.weight(1f))
                if (selectable) {
                    CornerButton(
                        icon = if (selected) Icons.Filled.Check else Icons.Outlined.CheckBoxOutlineBlank,
                        tint = if (selected) Color.White else c.muted,
                        background = if (selected) Brand500 else c.text.copy(alpha = 0.05f),
                        contentDescription = if (selected) "Выбрано" else "Выбрать",
                    )
                } else {
                    CornerButton(
                        icon = Icons.Outlined.NorthEast,
                        tint = if (c.isDark) Brand300 else Brand600,
                        background = c.text.copy(alpha = 0.05f),
                        contentDescription = null,
                    )
                }
            }
        }
    }
}

/**
 * Результат App Store без карточки в каталоге — как `StoreHitCard` на ПК:
 * значок, «Есть на Apple ID» / «Нет в App Store», название и разработчик.
 */
@Composable
private fun StoreHitCard(hit: AppStoreSearchHit, onClick: () -> Unit) {
    val c = signumColors
    Panel(onClick = onClick) {
        Column(Modifier.fillMaxWidth().heightIn(min = CARD_MIN_HEIGHT)) {
            Row(verticalAlignment = Alignment.Top) {
                AppIcon(hit.artwork, hit.name, Brand500)
                Spacer(Modifier.weight(1f))
                when {
                    hit.foundOnAccount -> CardBadge("Есть на Apple ID", Icons.Outlined.Restore, BadgeTone.Good)
                    hit.removedFromStore -> CardBadge("Нет в App Store", Icons.Outlined.Storefront, BadgeTone.Warn)
                }
            }
            Spacer(Modifier.height(16.dp))
            Text(
                hit.name,
                color = c.text,
                fontWeight = FontWeight.Bold,
                fontSize = 16.sp,
                lineHeight = 20.sp,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            val sub = hit.artist?.trim().orEmpty()
            if (sub.isNotEmpty()) {
                Spacer(Modifier.height(3.dp))
                Text(sub, color = c.muted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            val meta = listOfNotNull(
                hit.version?.takeIf { it.isNotBlank() }?.let { "v$it" },
                hit.minIos?.takeIf { it.isNotBlank() }?.let { "iOS $it+" },
            ).joinToString(" · ")
            if (meta.isNotEmpty()) {
                Text(meta, color = c.muted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Spacer(Modifier.weight(1f))
            Spacer(Modifier.height(14.dp))
            Row {
                Spacer(Modifier.weight(1f))
                CornerButton(
                    icon = Icons.Outlined.NorthEast,
                    tint = if (c.isDark) Brand300 else Brand600,
                    background = c.text.copy(alpha = 0.05f),
                    contentDescription = null,
                )
            }
        }
    }
}

/** Три значка приложений разработчика внахлёст. */
@Composable
private fun StackedIcons(icons: List<String>, name: String, size: Dp = 32.dp) {
    val list = icons.ifEmpty { listOf("") }.take(3)
    Box(Modifier.width(size + (size * 0.6f) * (list.size - 1)).height(size)) {
        list.forEachIndexed { index, icon ->
            Box(
                Modifier
                    .padding(start = (size * 0.6f) * index)
                    .clip(RoundedCornerShape(9.dp))
                    .border(2.dp, signumColors.surface, RoundedCornerShape(9.dp)),
            ) {
                AppIcon(icon.ifBlank { null }, name, Brand500, size = size, corner = 9.dp, glow = false)
            }
        }
    }
}

@Composable
private fun DeveloperRow(dev: StoreDeveloper, onClick: () -> Unit) {
    val c = signumColors
    Panel(onClick = onClick) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            StackedIcons(dev.icons, dev.name)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(dev.name, color = c.text, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text("${dev.appCount} ${pluralApps(dev.appCount)} в App Store", color = c.muted, fontSize = 11.sp)
            }
            Icon(Icons.Outlined.ChevronRight, contentDescription = null, tint = c.muted)
        }
    }
}

/**
 * Страница разработчика: все его приложения, отметка для пакетной загрузки
 * и «Скачать отмеченные» — как на ПК.
 */
@Composable
private fun DeveloperDialog(
    dev: StoreDeveloper,
    apps: List<AppStoreSearchHit>,
    loading: Boolean,
    error: String,
    picked: Set<String>,
    batchBusy: Boolean,
    onTogglePick: (String) -> Unit,
    onToggleAll: () -> Unit,
    onDownloadPicked: () -> Unit,
    onOpen: (AppStoreSearchHit) -> Unit,
    onClose: () -> Unit,
) {
    val titleColor = Color(0xFFF8FAFC)
    val muted = Color(0xFF94A3B8)
    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(max = 680.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(Space800)
            .border(1.dp, Color(0x33FFFFFF), RoundedCornerShape(20.dp))
            .padding(18.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            StackedIcons(dev.icons.ifEmpty { apps.mapNotNull { it.artwork }.take(3) }, dev.name, size = 36.dp)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(dev.name, color = titleColor, fontWeight = FontWeight.Bold, fontSize = 17.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(
                    if (loading) "Спрашиваем Apple…" else "${apps.size} ${pluralApps(apps.size)} в App Store",
                    color = muted,
                    fontSize = 12.sp,
                )
            }
        }
        if (error.isNotBlank()) {
            Spacer(Modifier.height(10.dp))
            Text(error, color = Color(0xFFFDA4AF), fontSize = 12.sp, lineHeight = 17.sp)
        }
        if (!loading && apps.isNotEmpty()) {
            Spacer(Modifier.height(12.dp))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton(if (picked.size == apps.size) "Снять все" else "Выбрать все", onClick = onToggleAll)
                PrimaryButton(
                    if (batchBusy) "Ставим в очередь…" else "Скачать отмеченные" + if (picked.isEmpty()) "" else " · ${picked.size}",
                    onClick = onDownloadPicked,
                    loading = batchBusy,
                    enabled = picked.isNotEmpty() && !batchBusy,
                    modifier = Modifier.weight(1f),
                )
            }
            Spacer(Modifier.height(6.dp))
            Text(
                "Apple отдаёт только то, что сейчас в магазине. Снятые с продажи ищите по названию или номеру.",
                color = muted,
                fontSize = 11.sp,
                lineHeight = 15.sp,
            )
        }
        Spacer(Modifier.height(12.dp))
        if (loading) {
            Row(Modifier.padding(vertical = 18.dp), verticalAlignment = Alignment.CenterVertically) {
                CircularProgressIndicator(Modifier.size(16.dp), color = Brand300, strokeWidth = 2.dp)
                Spacer(Modifier.width(10.dp))
                Text("Спрашиваем Apple…", color = muted, fontSize = 13.sp)
            }
        }
        // Высота ограничена: у крупной студии сотня приложений.
        LazyColumn(Modifier.weight(1f, fill = false), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            items(apps, key = { "d${it.adamId}" }) { hit ->
                val on = hit.adamId in picked
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(12.dp))
                        .background(if (on) Brand500.copy(alpha = 0.14f) else Color(0x0DFFFFFF))
                        .border(1.dp, if (on) Brand400.copy(alpha = 0.4f) else Color.Transparent, RoundedCornerShape(12.dp))
                        .padding(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // Отметка отдельной кнопкой: нажатие на строку открывает
                    // приложение, а не молча его отмечает.
                    Box(
                        Modifier
                            .size(36.dp)
                            .clip(RoundedCornerShape(8.dp))
                            .clickable { onTogglePick(hit.adamId) },
                        contentAlignment = Alignment.Center,
                    ) {
                        Box(
                            Modifier
                                .size(20.dp)
                                .clip(RoundedCornerShape(6.dp))
                                .background(if (on) Brand500 else Color.Transparent)
                                .border(1.5.dp, if (on) Brand400 else Color(0x55FFFFFF), RoundedCornerShape(6.dp)),
                            contentAlignment = Alignment.Center,
                        ) {
                            if (on) Icon(Icons.Filled.Check, contentDescription = "Отмечено", tint = Color.White, modifier = Modifier.size(14.dp))
                        }
                    }
                    Row(
                        Modifier.weight(1f).clickable { onOpen(hit) },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        AppIcon(hit.artwork, hit.name, Brand500, size = 44.dp, corner = 11.dp, glow = false)
                        Spacer(Modifier.width(10.dp))
                        Column(Modifier.weight(1f)) {
                            Text(hit.name, color = Color(0xFFE2E8F0), fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(
                                listOfNotNull(
                                    hit.version?.let { "v$it" },
                                    hit.minIos?.let { "iOS $it+" },
                                ).joinToString(" · ").ifEmpty { hit.adamId },
                                color = muted,
                                fontSize = 11.sp,
                                maxLines = 1,
                            )
                        }
                        Icon(Icons.Outlined.Download, contentDescription = null, tint = muted, modifier = Modifier.size(18.dp))
                    }
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        SecondaryButton("Закрыть", onClick = onClose, modifier = Modifier.fillMaxWidth(), enabled = !batchBusy)
    }
}

/** «1 разработчик», «2 разработчика», «5 разработчиков». */
private fun pluralDevs(count: Int): String = plural(count, "разработчик", "разработчика", "разработчиков")

/** «1 приложение», «2 приложения», «5 приложений». */
private fun pluralApps(count: Int): String = plural(count, "приложение", "приложения", "приложений")

private fun plural(count: Int, one: String, few: String, many: String): String {
    val tail = count % 100
    if (tail in 11..14) return many
    return when (count % 10) {
        1 -> one
        2, 3, 4 -> few
        else -> many
    }
}

/** Строка «подпись — значение» с копированием: вводить это придётся на iPhone. */
@Composable
private fun CopyRow(label: String, value: String) {
    val copy = pw.appbank.signum.ui.components.rememberCopyText()
    Row(
        Modifier
            .fillMaxWidth()
            .clickable { copy(value) },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(label, color = Color(0xFF94A3B8), fontSize = 11.sp)
            Text(
                value,
                color = Color(0xFFF8FAFC),
                fontSize = 15.sp,
                fontWeight = FontWeight.SemiBold,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text("копировать", color = Brand400, fontSize = 11.sp, fontWeight = FontWeight.Bold)
    }
}

/**
 * Баннер каталога.
 *
 * Оттенки те же, что на ПК: янтарный — предупреждение, фиолетовый — новость,
 * синий по умолчанию.
 */
@Composable
private fun CatalogBannerCard(banner: CatalogBanner) {
    val brush = when (banner.style?.lowercase()) {
        "amber" -> Brush.linearGradient(listOf(Color(0xFFF5B72F), Color(0xFFDF7D14)))
        "violet" -> Brush.linearGradient(listOf(Color(0xFF7C3AED), Color(0xFF4338CA)))
        else -> Brush.linearGradient(listOf(Color(0xFF1686FF), Color(0xFF1556D7)))
    }
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .background(brush)
            .padding(16.dp),
    ) {
        Text(
            banner.title,
            color = Color.White,
            fontWeight = FontWeight.Bold,
            fontSize = 15.sp,
        )
        val text = banner.text?.trim().orEmpty()
        if (text.isNotEmpty()) {
            Spacer(Modifier.height(4.dp))
            Text(
                text,
                color = Color(0xCCFFFFFF),
                fontSize = 13.sp,
                lineHeight = 18.sp,
            )
        }
    }
}
