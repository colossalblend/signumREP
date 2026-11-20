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
import androidx.compose.ui.platform.LocalClipboardManager
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
    var catalogSort by remember { mutableStateOf("default") }
    var compatibility by remember { mutableStateOf("all") }
    val deviceIos = vm.deviceIos()
    val displayedApps = remember(apps, catalogSort, compatibility, deviceIos) {
        val compatible = when (compatibility) {
            "yes" -> apps.filter { it.min_ios.isNullOrBlank() || deviceIos.isNullOrBlank() || CatalogResolve.compareIos(deviceIos, it.min_ios!!) >= 0 }
            "no" -> apps.filter { !it.min_ios.isNullOrBlank() && !deviceIos.isNullOrBlank() && CatalogResolve.compareIos(deviceIos, it.min_ios!!) < 0 }
            else -> apps
        }
        when (catalogSort) {
            "name" -> compatible.sortedBy { it.title.lowercase() }
            "ios" -> compatible.sortedWith(compareBy(nullsFirst()) { it.min_ios })
            else -> compatible
        }
    }
    val displayedSections = if (catalogSort == "default" && compatibility == "all") sections else null
    // Android 10+ exports to Downloads/Signum through MediaStore. Older
    // versions need a user-selected folder before the first export.
    var exportPending by remember { mutableStateOf(false) }
    val folderPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        if (uri != null) {
            vm.engine.settings.setDownloadsTree(uri)
            if (exportPending && vm.queueOffer(false)) goInstalls()
        }
        exportPending = false
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
    LazyVerticalGrid(
        columns = GridCells.Adaptive(160.dp),
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
                    subtitle = s.selectedDevice?.productVersion?.let { "Банки и важные приложения · iPhone iOS $it" }
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
                SignumField(s.search, vm::setSearch, placeholder = "Поиск в каталоге и App Store")
                if (s.groups.isNotEmpty()) {
                    Spacer(Modifier.height(10.dp))
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip("Все", s.groupFilter == "all") { vm.setGroup("all") }
                        s.groups.sortedByDescending { it.priority ?: 0 }.forEach { g ->
                            FilterChip(g.title, s.groupFilter == g.id) { vm.setGroup(g.id) }
                        }
                    }
                }
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Сортировка", color = c.muted, fontSize = 11.sp)
                    FilterChip("По каталогу", catalogSort == "default") { catalogSort = "default" }
                    FilterChip("По названию", catalogSort == "name") { catalogSort = "name" }
                    FilterChip("По iOS", catalogSort == "ios") { catalogSort = "ios" }
                }
                if (deviceIos != null) {
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip("Любая совместимость", compatibility == "all") { compatibility = "all" }
                        FilterChip("Подходит · iOS $deviceIos", compatibility == "yes") { compatibility = "yes" }
                        FilterChip("Не подходит", compatibility == "no") { compatibility = "no" }
                    }
                }
                if (s.search.isNotBlank()) {
                    Text(
                        "Найдено: ${apps.size}",
                        color = c.muted,
                        fontSize = 11.sp,
                        modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                    )
                }
                StatusLine(s.catalogError, error = true)
                StatusLine(s.storeSearchError, error = true)
                if (s.storeSearching) {
                    Text("Ищем приложение…", color = c.muted, fontSize = 12.sp)
                }
                if (s.regionNotice.isNotBlank()) {
                    Spacer(Modifier.height(8.dp))
                    Panel {
                        Text("Страна магазина", color = Color(0xFFFCD34D), fontWeight = FontWeight.Bold, fontSize = 13.sp)
                        Spacer(Modifier.height(6.dp))
                        Text(s.regionNotice, color = Color(0xFFE2E8F0), fontSize = 12.sp, lineHeight = 17.sp)
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
                Spacer(Modifier.height(8.dp))
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
        if (s.catalogLoading) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Box(Modifier.fillMaxWidth().height(180.dp), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = Brand400Safe)
                }
            }
        }
        if (!s.catalogLoading && s.catalog.isEmpty() && s.catalogError.isBlank()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Panel {
                    Text("Каталог пока пуст", color = c.text, fontWeight = FontWeight.Bold)
                }
            }
        }
        if (!s.catalogLoading && !s.storeSearching && s.storeSearchError.isBlank() && s.storeHits.isEmpty() && s.catalog.isNotEmpty() && apps.isEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                val bank = detectBankAlt(s.search)
                Panel {
                    Text("Ничего не найдено", color = c.text, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(6.dp))
                    Text(
                        "Попробуйте полное название, сокращение или английский вариант.",
                        color = c.muted,
                        fontSize = 13.sp,
                        lineHeight = 18.sp,
                    )
                    if (bank != null) {
                        Spacer(Modifier.height(12.dp))
                        Text(
                            "В каталоге нет подходящей версии ${bank.displayName()}. Можно установить другим способом — по номеру телефона.",
                            color = Color(0xFFE2E8F0),
                            fontSize = 13.sp,
                            lineHeight = 18.sp,
                        )
                        Spacer(Modifier.height(10.dp))
                        SecondaryButton("Альтернативный способ установки", onClick = vm::openBankAlt, modifier = Modifier.fillMaxWidth())
                    }
                }
            }
        }
        if (s.storeHits.isNotEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Text("Результаты поиска", color = c.text, fontSize = 16.sp, fontWeight = FontWeight.Bold)
            }
            items(s.storeHits, key = { "s${it.adamId}" }) { hit ->
                Panel(onClick = { vm.openStoreHit(hit) }) {
                    Column {
                        if (hit.artwork.isNullOrBlank()) {
                            Box(
                                Modifier.size(64.dp).clip(RoundedCornerShape(16.dp)).background(Brand400.copy(alpha = 0.15f)),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(hit.name.take(1), color = Brand300, fontWeight = FontWeight.Bold, fontSize = 24.sp)
                            }
                        } else AsyncImage(
                            hit.artwork,
                            contentDescription = null,
                            modifier = Modifier.size(64.dp).clip(RoundedCornerShape(16.dp)),
                            contentScale = ContentScale.Crop,
                        )
                        Spacer(Modifier.height(10.dp))
                        Text(hit.name, color = c.text, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Text(hit.artist ?: hit.bundleId ?: "", color = c.muted, fontSize = 11.sp, maxLines = 1)
                        if (hit.foundOnAccount) {
                            Text("Есть на Apple ID", color = Brand300, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
        }
        // Разработчики — под приложениями, как на ПК: сначала то, что искали,
        // потом «а кто это сделал и что ещё у него есть».
        if (s.devHits.isNotEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Column {
                    Spacer(Modifier.height(6.dp))
                    Text("Разработчики", color = c.text, fontSize = 16.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(8.dp))
                    s.devHits.forEach { dev ->
                        Panel(onClick = { vm.openDeveloper(dev) }) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Column(Modifier.weight(1f)) {
                                    Text(
                                        dev.name,
                                        color = c.text,
                                        fontWeight = FontWeight.SemiBold,
                                        fontSize = 14.sp,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                    Text(
                                        "приложений в поиске: ${dev.appCount}",
                                        color = c.muted,
                                        fontSize = 11.sp,
                                    )
                                }
                                Text("Открыть", color = Brand400, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                            }
                        }
                        Spacer(Modifier.height(8.dp))
                    }
                }
            }
        }
        val favorites = s.settings.favoriteAppIds.toSet()
        val favoriteApps = if (s.search.isBlank() && catalogSort == "default" && compatibility == "all") apps.filter { it.id in favorites } else emptyList()
        if (favoriteApps.isNotEmpty()) {
            item(key = "g-favorites", span = { GridItemSpan(maxLineSpan) }) {
                Row(
                    Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 2.dp),
                    verticalAlignment = Alignment.Bottom,
                ) {
                    Text("Избранное", color = c.text, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.weight(1f))
                    Text("${favoriteApps.size}", color = c.muted, fontSize = 11.sp)
                }
            }
            items(favoriteApps, key = { "fav-${it.id}" }) { app ->
                AppCard(
                    app = app,
                    favorite = true,
                    selectable = s.selectMode,
                    selected = app.id in s.selectedAppIds,
                    onToggleFavorite = { vm.toggleFavorite(app.id) },
                ) { if (s.selectMode) vm.toggleSelected(app.id) else vm.openOffer(app) }
            }
        }
        if (displayedSections != null) {
            displayedSections.forEach { section ->
                item(key = "g-${section.id}", span = { GridItemSpan(maxLineSpan) }) {
                    Row(
                        Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 2.dp),
                        verticalAlignment = Alignment.Bottom,
                    ) {
                        Text(section.title, color = c.text, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                        Spacer(Modifier.weight(1f))
                        Text("${section.apps.size}", color = c.muted, fontSize = 11.sp)
                    }
                }
                items(section.apps, key = { it.id }) { app ->
                    AppCard(
                        app = app,
                        favorite = app.id in favorites,
                        selectable = s.selectMode,
                        selected = app.id in s.selectedAppIds,
                        onToggleFavorite = { vm.toggleFavorite(app.id) },
                    ) { if (s.selectMode) vm.toggleSelected(app.id) else vm.openOffer(app) }
                }
            }
        } else {
            items(displayedApps, key = { it.id }) { app ->
                AppCard(
                    app = app,
                    favorite = app.id in favorites,
                    selectable = s.selectMode,
                    selected = app.id in s.selectedAppIds,
                    onToggleFavorite = { vm.toggleFavorite(app.id) },
                ) { if (s.selectMode) vm.toggleSelected(app.id) else vm.openOffer(app) }
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
            OfferDialog(
                offer = offer,
                versions = s.versions,
                versionsLoading = s.versionsLoading,
                versionsError = s.versionsError,
                versionsSearch = s.versionsSearch,
                versionsVisibleCount = s.versionsVisibleCount,
                chosenStore = s.chosenVersion,
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
        Dialog(onDismissRequest = vm::closeDeveloper) {
            SolidCard {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            openedDev.name,
                            color = Color(0xFFF8FAFC),
                            fontWeight = FontWeight.Bold,
                            fontSize = 17.sp,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            when {
                                s.developerLoading -> "Смотрим, что есть в магазине…"
                                s.developerApps.isEmpty() -> "В App Store ничего не нашлось"
                                else -> "Приложений: ${s.developerApps.size}"
                            },
                            color = Color(0xFF94A3B8),
                            fontSize = 12.sp,
                        )
                    }
                }
                Spacer(Modifier.height(12.dp))
                // Ограничиваем высоту: у крупной студии сотня приложений, и
                // без предела окно уезжает за края экрана.
                LazyColumn(Modifier.heightIn(max = 420.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(s.developerApps, key = { "d${it.adamId}" }) { hit ->
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .clickable {
                                    vm.closeDeveloper()
                                    vm.openStoreHit(hit)
                                },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            AsyncImage(
                                hit.artwork,
                                contentDescription = null,
                                modifier = Modifier.size(40.dp).clip(RoundedCornerShape(10.dp)),
                                contentScale = ContentScale.Crop,
                            )
                            Spacer(Modifier.width(10.dp))
                            Column(Modifier.weight(1f)) {
                                Text(
                                    hit.name,
                                    color = Color(0xFFE2E8F0),
                                    fontSize = 14.sp,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Text(
                                    listOfNotNull(
                                        hit.version?.let { "v$it" },
                                        hit.minIos?.let { "iOS $it+" },
                                    ).joinToString(" · "),
                                    color = Color(0xFF94A3B8),
                                    fontSize = 11.sp,
                                )
                            }
                        }
                    }
                }
                Spacer(Modifier.height(12.dp))
                SecondaryButton("Закрыть", onClick = vm::closeDeveloper, modifier = Modifier.fillMaxWidth())
            }
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
                            "На этом Apple ID приложение не покупалось, поэтому выбирать не из чего — будет скачана последняя доступная версия. Если покупали на другом Apple ID, войдите в него на странице «Аккаунт» и обновите список.",
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
private fun VersionRow(title: String, subtitle: String, selected: Boolean, onClick: () -> Unit) {
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

// Тот же оттенок, что Brand400 в палитре: отдельная копия осталась со старого
// сиреневого и после перекраски стала бы расходиться с остальным интерфейсом.
private val Brand400Safe = Brand400

@Composable
private fun FilterChip(label: String, selected: Boolean, onClick: () -> Unit) {
    val c = signumColors
    val shape = RoundedCornerShape(20.dp)
    Text(
        label,
        color = if (selected) Brand300 else c.muted,
        fontSize = 12.sp,
        fontWeight = FontWeight.Medium,
        modifier = Modifier
            .clip(shape)
            .background(if (selected) Brand500.copy(0.15f) else Color.Transparent)
            .border(1.dp, if (selected) Brand500.copy(0.25f) else c.border, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 6.dp),
    )
}

@Composable
private fun AppCard(
    app: CatalogApp,
    favorite: Boolean = false,
    selectable: Boolean = false,
    selected: Boolean = false,
    onToggleFavorite: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val c = signumColors
    val store = app.in_store == true
    Panel(onClick = onClick) {
        Column(Modifier.fillMaxWidth().animateContentSize()) {
            if (!app.icon_url.isNullOrBlank()) {
                AsyncImage(
                    app.icon_url,
                    contentDescription = null,
                    modifier = Modifier.size(64.dp).clip(RoundedCornerShape(16.dp)),
                    contentScale = ContentScale.Crop,
                )
            } else {
                Box(
                    Modifier
                        .size(64.dp)
                        .clip(RoundedCornerShape(16.dp))
                        .background(Brand500),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(app.title.take(1), color = Color.White, fontWeight = FontWeight.Black, fontSize = 22.sp)
                }
            }
            Spacer(Modifier.height(8.dp))
            Box(
                Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (store) Color(0x334ADE80) else Color(0x33FBBF24))
                    .padding(horizontal = 8.dp, vertical = 4.dp),
            ) {
                Text(
                    if (store) "App Store" else "Из покупок",
                    color = if (store) Color(0xFF6EE7B7) else Color(0xFFFCD34D),
                    fontSize = 9.sp,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.height(10.dp))
            Text(app.title, color = c.text, fontWeight = FontWeight.Bold, fontSize = 15.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(
                CatalogSearch.formatVersion(app.version, app.min_ios),
                color = c.muted,
                fontSize = 11.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (selectable || onToggleFavorite != null) {
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (onToggleFavorite != null) {
                        Text(
                            if (favorite) "★ В избранном" else "☆ В избранное",
                            color = if (favorite) Color(0xFFFCD34D) else c.muted,
                            fontSize = 11.sp,
                            modifier = Modifier.clickable(onClick = onToggleFavorite),
                        )
                    }
                    if (selectable) {
                        Spacer(Modifier.weight(1f))
                        Text(
                            if (selected) "Выбрано" else "Выбрать",
                            color = if (selected) Brand300 else c.muted,
                            fontSize = 11.sp,
                            fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                        )
                    }
                }
            }
        }
    }
}

/** Строка «подпись — значение» с копированием: вводить это придётся на iPhone. */
@Composable
private fun CopyRow(label: String, value: String) {
    val clipboard = LocalClipboardManager.current
    Row(
        Modifier
            .fillMaxWidth()
            .clickable { clipboard.setText(AnnotatedString(value)) },
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
