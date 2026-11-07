package pw.appbank.signum.ui.navigation

import androidx.compose.animation.core.tween
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
import androidx.compose.material.icons.outlined.Badge
import androidx.compose.material.icons.outlined.InstallMobile
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.outlined.PhoneIphone
import androidx.compose.material.icons.outlined.Settings
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

private data class Dest(val route: String, val label: String, val icon: ImageVector)

private val dests = buildList {
    add(Dest("catalog", "Каталог", Icons.Outlined.Apps))
    add(Dest("signing", "Файл", Icons.Outlined.Badge))
    add(Dest("devices", "iPhone", Icons.Outlined.PhoneIphone))
    add(Dest("account", "Аккаунт", Icons.Outlined.Person))
    add(Dest("installs", "Очередь", Icons.Outlined.InstallMobile))
}

@Composable
fun SignumRoot(engine: SignumEngine) {
    val vm: SignumViewModel = viewModel(factory = SignumViewModel.Factory(engine))
    val state by vm.state.collectAsState()
    SignumTheme(themePref = state.settings.theme) {
        val c = signumColors
        var showOnboarding by remember { mutableStateOf(!engine.settings.onboardingSeen()) }
        Box(Modifier.fillMaxSize()) {
        val nav = rememberNavController()
        val back by nav.currentBackStackEntryAsState()
        val route = back?.destination?.route ?: "catalog"
        Column(
            Modifier
                .fillMaxSize()
                .background(c.spaceBrush)
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
                IconButton(onClick = { nav.navigate("settings") { launchSingleTop = true } }) {
                    Icon(
                        Icons.Outlined.Settings,
                        contentDescription = "Настройки",
                        tint = if (route == "settings") Brand300 else c.muted,
                    )
                }
            }
            // Баннер обновления над содержимым — там же, где он на ПК.
            //
            // Установку запускает экран, а не модель: системному установщику
            // нужен Context и разрешение, которого может не быть.
            val ctx = LocalContext.current
             AnimatedVisibility(
                 visible = state.ota != null,
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
                    fadeIn(tween(220)) + slideInHorizontally(initialOffsetX = { it / 4 }, animationSpec = tween(260))
                },
                exitTransition = {
                    fadeOut(tween(180)) + slideOutHorizontally(targetOffsetX = { -it / 4 }, animationSpec = tween(220))
                },
                popEnterTransition = {
                    fadeIn(tween(220)) + slideInHorizontally(initialOffsetX = { -it / 4 }, animationSpec = tween(260))
                },
                popExitTransition = {
                    fadeOut(tween(180)) + slideOutHorizontally(targetOffsetX = { it / 4 }, animationSpec = tween(220))
                },
            ) {
                composable("catalog") { CatalogScreen(vm, { goTab("installs") }, { goTab("account") }) }
                composable("signing") { SigningScreen(vm) { goTab("installs") } }
                composable("devices") { DevicesScreen(vm) }
                composable("account") { AccountScreen(vm) { goTab("installs") } }
                composable("installs") { InstallsScreen(vm) }
                composable("settings") { SettingsScreen(vm) }
            }
            // Нажали на уведомление о ходе установки — показываем очередь.
            val openQueue by QueueService.openQueueRequested.collectAsState()
            LaunchedEffect(openQueue) {
                if (openQueue) {
                    QueueService.openQueueRequested.value = false
                    goTab("installs")
                }
            }
            NavigationBar(
                containerColor = c.surfaceStrong,
                tonalElevation = 0.dp,
                modifier = Modifier.navigationBarsPadding(),
            ) {
                dests.forEach { d ->
                    val selected = route == d.route
                    NavigationBarItem(
                        selected = selected,
                        onClick = { goTab(d.route) },
                        icon = { Icon(d.icon, contentDescription = d.label, modifier = Modifier.size(20.dp)) },
                        alwaysShowLabel = true,
                        label = {
                            Text(
                                d.label,
                                fontSize = 10.sp,
                                maxLines = 1,
                                fontWeight = FontWeight.Medium,
                            )
                        },
                        colors = NavigationBarItemDefaults.colors(
                            selectedIconColor = Brand300,
                            selectedTextColor = Brand300,
                            indicatorColor = Brand500.copy(0.18f),
                            unselectedIconColor = c.muted,
                            unselectedTextColor = c.muted,
                        ),
                    )
                }
            }
        }
        if (showOnboarding) {
            OnboardingScreen(
                reduceMotion = state.settings.liteReduceMotion,
                onFinish = {
                    engine.settings.markOnboardingSeen()
                    showOnboarding = false
                },
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
            Text(message, color = Color(0xFFFCA5A5), fontSize = 12.sp)
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
