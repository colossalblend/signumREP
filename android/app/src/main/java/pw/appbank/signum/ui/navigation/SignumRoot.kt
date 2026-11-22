package pw.appbank.signum.ui.navigation

import androidx.compose.animation.core.tween
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.ime
import androidx.compose.material.icons.filled.AccountCircle
import androidx.compose.material.icons.filled.Apps
import androidx.compose.material.icons.filled.Downloading
import androidx.compose.material.icons.filled.FileOpen
import androidx.compose.material.icons.filled.PhoneIphone
import androidx.compose.material.icons.outlined.AccountCircle
import androidx.compose.material.icons.outlined.Downloading
import androidx.compose.material.icons.outlined.FileOpen
import androidx.compose.material.icons.outlined.WarningAmber
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.style.TextAlign
import androidx.core.view.WindowCompat
import pw.appbank.signum.data.models.JobStatus
import pw.appbank.signum.ui.components.ActivationSheet
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.screens.AboutScreen
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Apps
import androidx.compose.material.icons.outlined.PhoneIphone
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Text
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import pw.appbank.signum.ui.screens.OnboardingScreen
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.appbank.signum.R
import pw.appbank.signum.data.engine.SignumEngine
import pw.appbank.signum.data.jobs.QueueService
import pw.appbank.signum.data.models.APP_VERSION
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.screens.AccountScreen
import pw.appbank.signum.ui.screens.CatalogScreen
import pw.appbank.signum.ui.screens.DevicesScreen
import pw.appbank.signum.ui.screens.InstallsScreen
import pw.appbank.signum.ui.screens.SettingsScreen
import pw.appbank.signum.ui.screens.SigningScreen
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.theme.Brand300
import pw.appbank.signum.ui.theme.Brand500
import pw.appbank.signum.ui.theme.SignumTheme
import pw.appbank.signum.ui.theme.signumColors

private data class Dest(
    val route: String,
    val label: String,
    val icon: ImageVector,
    val selectedIcon: ImageVector,
)

// Подписи короткие и однотипные: в пять колонок длинное слово обрезалось.
// Выбранная вкладка рисуется залитым значком — так её видно и без цвета.
private val dests = listOf(
    Dest("catalog", "Каталог", Icons.Outlined.Apps, Icons.Filled.Apps),
    Dest("signing", "Файл", Icons.Outlined.FileOpen, Icons.Filled.FileOpen),
    Dest("devices", "iPhone", Icons.Outlined.PhoneIphone, Icons.Filled.PhoneIphone),
    Dest("account", "Аккаунт", Icons.Outlined.AccountCircle, Icons.Filled.AccountCircle),
    Dest("installs", "Очередь", Icons.Outlined.Downloading, Icons.Filled.Downloading),
)

@Composable
fun SignumRoot(engine: SignumEngine) {
    val vm: SignumViewModel = viewModel(factory = SignumViewModel.Factory(engine))
    val state by vm.state.collectAsState()
    SignumTheme(themePref = state.settings.theme) {
        val c = signumColors
        SystemBarsAppearance(dark = c.isDark)
        // rememberSaveable: после поворота экрана незаконченное обучение не
        // должно ни пропасть, ни открыться заново поверх уже пройденного.
        var showOnboarding by rememberSaveable { mutableStateOf(!engine.settings.onboardingSeen()) }
        val reduceMotion = state.settings.motionReduced
        Box(Modifier.fillMaxSize()) {
        val nav = rememberNavController()
        val back by nav.currentBackStackEntryAsState()
        val route = back?.destination?.route ?: "catalog"
        val density = LocalDensity.current
        val imeVisible = WindowInsets.ime.getBottom(density) > 0
        Column(
            Modifier
                .fillMaxSize()
                .then(if (state.settings.flatBackground) Modifier.background(c.background) else Modifier.background(c.spaceBrush))
                .statusBarsPadding()
                .imePadding(),
        ) {
            Row(
                Modifier.padding(start = 18.dp, end = 6.dp, top = 8.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Image(
                    painterResource(R.drawable.signum_icon),
                    contentDescription = null,
                    modifier = Modifier
                        .size(36.dp)
                        .clip(RoundedCornerShape(10.dp)),
                )
                Spacer(Modifier.width(10.dp))
                Column {
                    Text("Signum", color = c.text, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                    Text(APP_VERSION, color = c.muted, fontSize = 10.sp, fontWeight = FontWeight.Medium)
                }
                Spacer(Modifier.weight(1f))
                val inSettings = route == "settings" || route == "about"
                IconButton(onClick = {
                    if (!inSettings) nav.navigate("settings") { launchSingleTop = true }
                }) {
                    Icon(
                        if (inSettings) Icons.Filled.Settings else Icons.Outlined.Settings,
                        contentDescription = "Настройки",
                        tint = if (inSettings) c.brandSoft else c.muted,
                    )
                }
            }
            // Баннер обновления над содержимым — там же, где он на ПК.
            //
            // Установку запускает экран, а не модель: системному установщику
            // нужен Context и разрешение, которого может не быть.
            val ctx = LocalContext.current
            AnimatedVisibility(
                visible = state.ota != null && state.clientOutdated == null,
                enter = fadeIn(tween(220)) + expandVertically(tween(260)),
                exit = fadeOut(tween(160)) + shrinkVertically(tween(220)),
            ) {
                UpdateBanner(
                    version = state.ota?.version.orEmpty(),
                    notes = state.ota?.notes.orEmpty(),
                    busy = state.otaBusy,
                    percent = state.otaPercent,
                    message = state.otaMessage,
                    mandatory = state.ota?.mandatory == true,
                    onAction = vm::downloadOta,
                    onDismiss = vm::dismissOta,
                )
            }
            // Файл скачан — зовём системный установщик.
            LaunchedEffect(state.otaFile) {
                val path = state.otaFile
                if (path.isBlank()) return@LaunchedEffect
                val error = installApk(ctx, java.io.File(path))
                if (error == null) vm.otaInstallStarted() else vm.otaInstallFailed(error)
            }

            fun goTab(target: String) {
                nav.navigate(target) {
                    popUpTo(nav.graph.findStartDestination().id) {
                        saveState = false
                    }
                    launchSingleTop = true
                    restoreState = false
                }
            }
            NavHost(
                navController = nav,
                startDestination = "catalog",
                modifier = Modifier
                    .weight(1f)
                    .clipToBounds(),
                enterTransition = {
                    if (reduceMotion) EnterTransition.None
                    else fadeIn(tween(220)) + slideInHorizontally(initialOffsetX = { it / 4 }, animationSpec = tween(260))
                },
                exitTransition = {
                    if (reduceMotion) ExitTransition.None
                    else fadeOut(tween(180)) + slideOutHorizontally(targetOffsetX = { -it / 4 }, animationSpec = tween(220))
                },
                popEnterTransition = {
                    if (reduceMotion) EnterTransition.None
                    else fadeIn(tween(220)) + slideInHorizontally(initialOffsetX = { -it / 4 }, animationSpec = tween(260))
                },
                popExitTransition = {
                    if (reduceMotion) ExitTransition.None
                    else fadeOut(tween(180)) + slideOutHorizontally(targetOffsetX = { it / 4 }, animationSpec = tween(220))
                },
            ) {
                composable("catalog") { CatalogScreen(vm, { goTab("installs") }, { goTab("account") }) }
                composable("signing") { SigningScreen(vm) { goTab("installs") } }
                composable("devices") { DevicesScreen(vm) }
                composable("account") { AccountScreen(vm) { goTab("installs") } }
                composable("installs") { InstallsScreen(vm) }
                composable("settings") {
                    SettingsScreen(
                        vm,
                        onOpenAbout = { nav.navigate("about") { launchSingleTop = true } },
                        onReplayOnboarding = {
                            engine.settings.resetOnboarding()
                            goTab("catalog")
                            showOnboarding = true
                        },
                    )
                }
                composable("about") { AboutScreen(onBack = { nav.popBackStack() }) }
            }
            // Нажали на уведомление о ходе установки — показываем очередь.
            val openQueue by QueueService.openQueueRequested.collectAsState()
            LaunchedEffect(openQueue) {
                if (openQueue) {
                    QueueService.openQueueRequested.value = false
                    goTab("installs")
                }
            }
            val jobs by engine.jobs.jobsFlow.collectAsState()
            val activeJobs = jobs.count { it.status == JobStatus.running || it.status == JobStatus.queued }
            // Пока открыта клавиатура, нижнюю панель прячем: иначе она
            // поднималась над клавиатурой и съедала половину экрана с полем.
            if (!imeVisible) {
                SignumBottomBar(
                    route = route,
                    activeJobs = activeJobs,
                    onSelect = { target -> if (target != route) goTab(target) },
                )
            }
        }
        state.activation?.let { view ->
            ActivationSheet(
                view = view,
                onRefresh = vm::refreshActivation,
                onClose = vm::closeActivation,
            )
        }
        if (showOnboarding) {
            OnboardingScreen(
                reduceMotion = reduceMotion,
                onFinish = {
                    engine.settings.markOnboardingSeen()
                    showOnboarding = false
                },
            )
        }
        state.clientOutdated?.let { info ->
            ClientOutdatedGate(
                message = info.message,
                minVersion = info.minClientVersion,
                latestVersion = state.ota?.version,
                busy = state.otaBusy || state.otaChecking,
                percent = state.otaPercent,
                error = state.otaMessage,
                onUpdate = vm::updateFromOutdatedGate,
            )
        }
        }
    }
}

/**
 * Нижняя панель вкладок.
 *
 * Отступ под системную панель жестов даёт сам NavigationBar (windowInsets);
 * раньше сверху добавлялся ещё navigationBarsPadding, и на телефонах с
 * кнопками панель стояла на двойной подушке.
 */
@Composable
private fun SignumBottomBar(route: String, activeJobs: Int, onSelect: (String) -> Unit) {
    val c = signumColors
    Column {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.divider))
        NavigationBar(
            containerColor = c.surfaceStrong,
            tonalElevation = 0.dp,
        ) {
            dests.forEach { d ->
                val selected = route == d.route
                NavigationBarItem(
                    selected = selected,
                    onClick = { onSelect(d.route) },
                    icon = {
                        val icon = if (selected) d.selectedIcon else d.icon
                        if (d.route == "installs" && activeJobs > 0) {
                            BadgedBox(badge = {
                                Badge(containerColor = c.brand, contentColor = Color.White) {
                                    Text(if (activeJobs > 9) "9+" else activeJobs.toString(), fontSize = 9.sp)
                                }
                            }) {
                                Icon(icon, contentDescription = null, modifier = Modifier.size(22.dp))
                            }
                        } else {
                            Icon(icon, contentDescription = null, modifier = Modifier.size(22.dp))
                        }
                    },
                    alwaysShowLabel = true,
                    label = {
                        Text(
                            d.label,
                            fontSize = 11.sp,
                            maxLines = 1,
                            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium,
                        )
                    },
                    colors = NavigationBarItemDefaults.colors(
                        selectedIconColor = c.brandSoft,
                        selectedTextColor = c.brandSoft,
                        indicatorColor = c.brand.copy(alpha = if (c.isDark) 0.18f else 0.12f),
                        unselectedIconColor = c.muted,
                        unselectedTextColor = c.muted,
                    ),
                )
            }
        }
    }
}

/**
 * Цвет значков в строке состояния и панели навигации под текущую тему.
 * Раньше активность раз и навсегда ставила светлые значки, и в светлой теме
 * часы и батарея пропадали на белом фоне.
 */
@Composable
private fun SystemBarsAppearance(dark: Boolean) {
    val view = LocalView.current
    if (view.isInEditMode) return
    SideEffect {
        val window = (view.context as? android.app.Activity)?.window ?: return@SideEffect
        WindowCompat.getInsetsController(window, view).apply {
            isAppearanceLightStatusBars = !dark
            isAppearanceLightNavigationBars = !dark
        }
    }
}

/**
 * Сервер больше не принимает эту версию — как ClientOutdatedGate на ПК.
 * Окно не закрывается: без обновления любой шаг упрётся в отказ.
 */
@Composable
private fun ClientOutdatedGate(
    message: String,
    minVersion: String?,
    latestVersion: String?,
    busy: Boolean,
    percent: Float,
    error: String,
    onUpdate: () -> Unit,
) {
    val c = signumColors
    Box(
        Modifier
            .fillMaxSize()
            .background(c.background.copy(alpha = 0.97f))
            // Касания не должны проходить к экрану под окном.
            .pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent() } }
            .statusBarsPadding()
            .navigationBarsPadding()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Panel {
            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                Box(
                    Modifier.size(56.dp).clip(RoundedCornerShape(16.dp)).background(c.danger.copy(alpha = 0.12f)),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Outlined.WarningAmber, contentDescription = null, tint = c.danger, modifier = Modifier.size(28.dp))
                }
                Spacer(Modifier.height(16.dp))
                Text("Приложение недоступно", color = c.text, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.height(8.dp))
                Text(message, color = c.muted, fontSize = 14.sp, lineHeight = 20.sp, textAlign = TextAlign.Center)
                val versions = listOfNotNull(
                    minVersion?.takeIf { it.isNotBlank() }?.let { "Нужна версия $it или новее" },
                    latestVersion?.takeIf { it.isNotBlank() }?.let { "Актуальная: $it" },
                ).joinToString(" · ")
                if (versions.isNotEmpty()) {
                    Spacer(Modifier.height(8.dp))
                    Text(versions, color = c.muted, fontSize = 12.sp, textAlign = TextAlign.Center)
                }
                Spacer(Modifier.height(10.dp))
                Text(
                    "Управление приложением заблокировано, пока Signum не обновлён.",
                    color = c.muted.copy(alpha = 0.8f),
                    fontSize = 12.sp,
                    lineHeight = 17.sp,
                    textAlign = TextAlign.Center,
                )
                StatusLine(error, error = true)
                Spacer(Modifier.height(16.dp))
                PrimaryButton(
                    if (busy && percent > 0f) "Загрузка… ${percent.toInt()}%" else "Обновить Signum",
                    onClick = onUpdate,
                    loading = busy,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

/**
 * Баннер «вышла новая версия».
 *
 * Обязательное обновление нельзя закрыть: старый клиент API уже не примет,
 * и без обновления человек упрётся в отказ на каждом шаге.
 */
@Composable
private fun UpdateBanner(
    version: String,
    notes: String,
    busy: Boolean,
    percent: Float,
    message: String,
    mandatory: Boolean,
    onAction: () -> Unit,
    onDismiss: () -> Unit,
) {
    val c = signumColors
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 14.dp, vertical = 6.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(Brand500.copy(alpha = 0.14f))
            .padding(14.dp),
    ) {
        Text(
            if (version.isBlank()) "Доступна новая версия Signum" else "Доступна новая версия Signum $version",
            color = c.text,
            fontWeight = FontWeight.Bold,
            fontSize = 14.sp,
        )
        if (notes.isNotBlank()) {
            Spacer(Modifier.height(4.dp))
            Text(notes, color = c.muted, fontSize = 12.sp, lineHeight = 17.sp, maxLines = 4)
        }
        if (message.isNotBlank()) {
            Spacer(Modifier.height(6.dp))
            Text(message, color = c.danger, fontSize = 12.sp)
        }
        Spacer(Modifier.height(10.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            PrimaryButton(
                if (busy) "Загрузка… ${percent.toInt()}%" else "Обновить",
                onClick = onAction,
                loading = busy,
                modifier = Modifier.weight(1f),
            )
            if (!mandatory && !busy) {
                Spacer(Modifier.width(8.dp))
                SecondaryButton("Позже", onClick = onDismiss)
            }
        }
    }
}

/**
 * Открыть системный установщик для скачанного файла.
 *
 * Возвращает текст ошибки или null. Отдельно ведём человека в настройки, если
 * установка из этого источника ещё не разрешена: без этого системное окно
 * просто не появится, и выглядит это как «кнопка не работает».
 */
private fun installApk(ctx: android.content.Context, file: java.io.File): String? {
    if (!file.isFile) return "Файл обновления не найден. Повторите загрузку."
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O &&
        !ctx.packageManager.canRequestPackageInstalls()
    ) {
        return runCatching {
            ctx.startActivity(
                android.content.Intent(
                    android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    android.net.Uri.parse("package:" + ctx.packageName),
                ).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK),
            )
            "Разрешите установку из Signum и нажмите «Обновить» ещё раз."
        }.getOrElse { "Разрешите установку приложений из Signum в настройках телефона." }
    }
    val uri = runCatching {
        androidx.core.content.FileProvider.getUriForFile(ctx, ctx.packageName + ".files", file)
    }.getOrNull() ?: return "Не удалось открыть файл обновления."
    return runCatching {
        ctx.startActivity(
            android.content.Intent(android.content.Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(
                    android.content.Intent.FLAG_ACTIVITY_NEW_TASK or
                        android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION,
                ),
        )
        null
    }.getOrElse { "Не удалось запустить установку обновления." }
}
