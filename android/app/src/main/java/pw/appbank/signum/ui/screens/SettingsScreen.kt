package pw.appbank.signum.ui.screens

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CleaningServices
import androidx.compose.material.icons.outlined.FolderOpen
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.School
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.data.models.APP_BUILD
import pw.appbank.signum.data.models.APP_VERSION
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.NavRow
import pw.appbank.signum.ui.components.Notice
import pw.appbank.signum.ui.components.NoticeTone
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PickerDialog
import pw.appbank.signum.ui.components.PickerOption
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.RowDivider
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.SectionTitle
import pw.appbank.signum.ui.components.SegmentedControl
import pw.appbank.signum.ui.components.SelectField
import pw.appbank.signum.ui.components.SettingsTextField
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.components.ToggleRow
import pw.appbank.signum.ui.theme.signumColors

/** Сколько нажатий на версию открывают меню разработчика — как на ПК. */
private const val DEV_MENU_TAPS = 7

@Composable
fun SettingsScreen(
    vm: SignumViewModel,
    onOpenAbout: () -> Unit,
    onReplayOnboarding: () -> Unit,
) {
    val full by vm.state.collectAsState()
    val s = full.settings
    val c = signumColors
    LaunchedEffect(Unit) {
        vm.loadStoreRegion()
        // Размер кэша считаем в фоне: обход папок с IPA на главном потоке
        // подвешивал открытие экрана.
        vm.refreshCacheSize()
    }
    // Читаем через стор, а не через AppSettings: там лежит сырой uri, а показать
    // надо имя папки, и оно же должно исчезнуть, если доступ к папке отозвали.
    var folderLabel by remember { mutableStateOf(vm.engine.settings.downloadsTreeLabel()) }
    val folderPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        if (uri != null) {
            vm.engine.settings.setDownloadsTree(uri)
            folderLabel = vm.engine.settings.downloadsTreeLabel()
        }
    }
    var devUnlocked by rememberSaveable { mutableStateOf(vm.engine.settings.devMenuUnlocked()) }
    var versionTaps by remember { mutableIntStateOf(0) }
    var devNote by remember { mutableStateOf("") }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        PageHeader("Настройки", "Оформление, загрузки и обслуживание")

        Panel {
            SectionTitle("Обучение")
            Spacer(Modifier.height(6.dp))
            Text(
                "Покажем, как пользоваться Signum: где найти нужное приложение и как поставить его " +
                    "на iPhone. Ничего менять не придётся — просто пройдём по шагам.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(10.dp))
            SecondaryButton(
                "Пройти заново",
                onClick = onReplayOnboarding,
                icon = Icons.Outlined.School,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        Panel {
            SectionTitle("Оформление")
            Spacer(Modifier.height(10.dp))
            SegmentedControl(
                options = listOf("light" to "Светлая", "dark" to "Тёмная", "system" to "Системная"),
                selected = s.theme,
                onSelect = { vm.saveSettings(s.copy(theme = it)) },
            )
            Spacer(Modifier.height(6.dp))
            ToggleRow(
                "Упрощённое оформление",
                s.liteDesign,
                hint = "Меньше анимаций и эффектов — быстрее на слабых телефонах. Перезапуск не нужен.",
            ) { vm.saveSettings(s.copy(liteDesign = it)) }
            if (s.liteDesign) {
                RowDivider()
                ToggleRow(
                    "Без анимаций",
                    s.liteReduceMotion,
                    hint = "Экраны и окна открываются сразу, без плавных переходов.",
                ) { vm.saveSettings(s.copy(liteReduceMotion = it)) }
                ToggleRow(
                    "Статичные заглушки",
                    s.liteStaticLoaders,
                    hint = "Серые блоки на время загрузки не мерцают.",
                ) { vm.saveSettings(s.copy(liteStaticLoaders = it)) }
                ToggleRow(
                    "Однотонный фон",
                    s.liteFlatBackgrounds,
                    hint = "Ровный цвет вместо мягкого перехода.",
                ) { vm.saveSettings(s.copy(liteFlatBackgrounds = it)) }
            }
        }

        Panel {
            SectionTitle("Обновления Signum")
            Spacer(Modifier.height(2.dp))
            ToggleRow(
                "Автопроверка обновлений",
                s.autoUpdate,
                hint = "При запуске Signum сам узнает о новой версии и покажет баннер.",
            ) { vm.saveSettings(s.copy(autoUpdate = it)) }
            Spacer(Modifier.height(6.dp))
            SecondaryButton(
                "Проверить сейчас",
                onClick = vm::checkUpdatesNow,
                icon = Icons.Outlined.Refresh,
                loading = full.otaChecking,
                modifier = Modifier.fillMaxWidth(),
            )
            StatusLine(full.otaCheckMessage)
        }

        Panel {
            SectionTitle("Вход и загрузка приложений")
            Spacer(Modifier.height(6.dp))
            Text(
                "Рекомендуем «Авто»: Signum сам выбирает безопасную схему входа. " +
                    "«V6» принудительно включает профиль Apple Configurator — попробуйте его, " +
                    "если не удаётся войти или загрузить приложение.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(10.dp))
            SegmentedControl(
                options = listOf("auto" to "Авто", "v6" to "V6"),
                selected = s.ipatoolEngine,
                onSelect = { vm.saveSettings(s.copy(ipatoolEngine = it)) },
            )
            Spacer(Modifier.height(8.dp))
            RowDivider()
            ToggleRow(
                "Прокси для Apple",
                s.proxyEnabled,
                hint = "Нужен там, где Apple недоступна напрямую. Работает и для загрузок — " +
                    "VPN на весь телефон включать не требуется.",
            ) { vm.saveSettings(s.copy(proxyEnabled = it)) }
            if (s.proxyEnabled) {
                Spacer(Modifier.height(8.dp))
                SegmentedControl(
                    options = listOf("socks5" to "SOCKS5", "socks4" to "SOCKS4", "http" to "HTTP"),
                    selected = s.proxyKind,
                    onSelect = { vm.saveSettings(s.copy(proxyKind = it)) },
                )
                Spacer(Modifier.height(8.dp))
                SettingsTextField(
                    value = s.proxyHost,
                    onCommit = { vm.saveSettings(vm.state.value.settings.copy(proxyHost = it.trim())) },
                    label = "Адрес",
                    placeholder = "например, 127.0.0.1",
                    keyboard = KeyboardOptions(keyboardType = KeyboardType.Uri),
                )
                Spacer(Modifier.height(6.dp))
                SettingsTextField(
                    value = if (s.proxyPort == 0) "" else s.proxyPort.toString(),
                    onCommit = { text ->
                        vm.saveSettings(vm.state.value.settings.copy(proxyPort = text.toIntOrNull() ?: 0))
                    },
                    label = "Порт",
                    keyboard = KeyboardOptions(keyboardType = KeyboardType.Number),
                    filter = { raw -> raw.filter { it.isDigit() }.take(5) },
                )
                Spacer(Modifier.height(6.dp))
                SettingsTextField(
                    value = s.proxyUser,
                    onCommit = { vm.saveSettings(vm.state.value.settings.copy(proxyUser = it.trim())) },
                    label = "Логин (если нужен)",
                )
                Spacer(Modifier.height(6.dp))
                SettingsTextField(
                    value = s.proxyPassword,
                    onCommit = { vm.saveSettings(vm.state.value.settings.copy(proxyPassword = it)) },
                    label = "Пароль (если нужен)",
                    visual = PasswordVisualTransformation(),
                    keyboard = KeyboardOptions(keyboardType = KeyboardType.Password),
                )
                Spacer(Modifier.height(6.dp))
                Text(
                    "Логин и пароль заполняйте, только если прокси их требует. Для socks5 обычно " +
                        "хватает адреса и порта. Применяется сразу.",
                    color = c.muted,
                    fontSize = 11.sp,
                    lineHeight = 15.sp,
                )
            }
        }

        Panel {
            SectionTitle("Установка на iPhone")
            Spacer(Modifier.height(6.dp))
            SegmentedControl(
                options = listOf("usb" to "USB", "wifi" to "Wi-Fi", "server" to "Сервер"),
                selected = s.installMethod,
                onSelect = { vm.saveSettings(vm.state.value.settings.copy(installMethod = it)) },
            )
            Spacer(Modifier.height(8.dp))
            Text(
                when (s.installMethod) {
                    "wifi" ->
                        "Напрямую по Wi-Fi: Signum подключается к iPhone по сети (порт 27015) и ставит приложение " +
                            "как по кабелю. На iPhone включите «Синхронизацию по Wi-Fi» в iTunes/Finder; удобен " +
                            "первичный спаринг по USB."
                    "server" ->
                        "Wi-Fi сервер: Signum раздаёт приложение по ссылке с QR-кодом, iPhone сам скачивает и " +
                            "устанавливает его через itms-services. Оба устройства — в одной сети, Signum не закрывать."
                    else ->
                        "Классическая установка по кабелю USB. Надёжнее всего и не требует Wi-Fi."
                },
                color = c.muted,
                fontSize = 11.sp,
                lineHeight = 15.sp,
            )
            if (s.installMethod == "wifi") {
                Spacer(Modifier.height(6.dp))
                SettingsTextField(
                    value = s.wifiHost,
                    onCommit = { vm.saveSettings(vm.state.value.settings.copy(wifiHost = it.trim())) },
                    label = "IP iPhone",
                    placeholder = "например, 192.168.1.42",
                    keyboard = KeyboardOptions(keyboardType = KeyboardType.Uri),
                )
                Spacer(Modifier.height(6.dp))
                Text(
                    "IP телефона смотрите в Настройках iPhone → Wi-Fi → «i» рядом с сетью. Порт 27015 используется автоматически.",
                    color = c.muted,
                    fontSize = 11.sp,
                    lineHeight = 15.sp,
                )
            }
        }

        Panel {
            SectionTitle("Страна магазина")
            Spacer(Modifier.height(6.dp))
            Text(
                when {
                    full.regionBusy -> "Проверяем…"
                    full.regionName.isNotBlank() -> "Сейчас: ${full.regionName}"
                    else -> "Войдите в Apple ID, чтобы увидеть страну"
                },
                color = c.muted,
                fontSize = 13.sp,
            )
            if (full.regionMessage.isNotBlank()) {
                Spacer(Modifier.height(6.dp))
                Text(full.regionMessage, color = c.warning, fontSize = 12.sp, lineHeight = 17.sp)
            }
            Spacer(Modifier.height(6.dp))
            Text(
                "Меняйте страну, только если приложение не продаётся в вашей. Если на счету Apple ID есть деньги, сменить не получится.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            if (full.regionPendingRestore) {
                Spacer(Modifier.height(8.dp))
                SecondaryButton(
                    "Вернуть мою страну",
                    onClick = { vm.restoreStoreRegion() },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            Spacer(Modifier.height(8.dp))
            val regions = remember { vm.storeRegions() }
            val currentRegion = regions.firstOrNull { it.code.equals(full.regionCode, ignoreCase = true) }
            var regionsOpen by rememberSaveable { mutableStateOf(false) }
            SelectField(
                value = currentRegion?.name.orEmpty(),
                placeholder = "Выбрать другую страну",
                hint = if (currentRegion != null) "сейчас" else null,
                enabled = !full.regionBusy,
                onClick = { regionsOpen = true },
            )
            if (regionsOpen) {
                PickerDialog(
                    title = "Страна магазина",
                    searchPlaceholder = "Найти страну…",
                    options = regions.map {
                        PickerOption(
                            it.code,
                            it.name,
                            if (it.code == currentRegion?.code) "сейчас" else null,
                        )
                    },
                    selected = currentRegion?.code,
                    onPick = { code ->
                        regionsOpen = false
                        if (!code.equals(full.regionCode, ignoreCase = true)) vm.changeStoreRegion(code)
                    },
                    onDismiss = { regionsOpen = false },
                )
            }
        }

        Panel {
            SectionTitle("Папка для скачанных файлов")
            Spacer(Modifier.height(6.dp))
            Text(
                if (folderLabel.isBlank()) {
                    if (vm.engine.settings.canExportToDownloads()) "Downloads/Signum — папка по умолчанию."
                    else "Выберите папку, чтобы сохранять IPA в файловый менеджер."
                } else {
                    folderLabel
                },
                color = if (folderLabel.isBlank()) c.muted else c.text,
                fontSize = 13.sp,
                lineHeight = 18.sp,
            )
            Spacer(Modifier.height(8.dp))
            SecondaryButton(
                if (folderLabel.isBlank()) "Выбрать папку" else "Другая папка",
                onClick = { folderPicker.launch(null) },
                icon = Icons.Outlined.FolderOpen,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        Panel {
            SectionTitle("Обслуживание")
            Spacer(Modifier.height(6.dp))
            Text("Временные файлы", color = c.muted, fontSize = 12.sp)
            Text(
                if (full.cacheBytes < 0) "Считаем…" else formatBytes(full.cacheBytes),
                color = c.text,
                fontSize = 20.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.height(4.dp))
            Text(
                "Скачанные и подготовленные IPA. Файлы дел, которые сейчас в очереди, не удаляются.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(8.dp))
            SecondaryButton(
                "Очистить",
                onClick = vm::clearCacheNow,
                icon = Icons.Outlined.CleaningServices,
                enabled = full.cacheBytes != 0L,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        Panel {
            SectionTitle("Запись для поддержки")
            Spacer(Modifier.height(2.dp))
            ToggleRow(
                "Отправлять журнал работы",
                full.diagAuto,
                hint = "Запись идёт всё время работы Signum и уходит разработчику, когда вы её закрываете — " +
                    "по ней находят ошибки, которые иначе видны только у вас. Личное вырезается до отправки: " +
                    "Apple ID, имя, пароли, коды из SMS и токены.",
            ) { vm.setDiagAuto(it) }
            ToggleRow(
                "Записывать прямо сейчас",
                full.diagRecording,
                hint = "Включите, повторите проблему и отправьте отчёт.",
            ) { vm.setDiagRecording(it) }
            if (full.diagRecording || full.diagMessage.isNotBlank()) {
                Spacer(Modifier.height(8.dp))
                PrimaryButton(
                    "Отправить отчёт",
                    onClick = vm::sendDiagReport,
                    loading = full.diagBusy,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                SecondaryButton(
                    "Очистить запись",
                    onClick = vm::clearDiagLog,
                    enabled = !full.diagBusy,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            StatusLine(full.diagMessage)
        }

        Panel {
            NavRow(
                icon = Icons.Outlined.Info,
                title = "О приложении",
                subtitle = "Telegram-канал, сайт и библиотеки",
                onClick = onOpenAbout,
            )
            RowDivider()
            Text(
                "Signum $APP_VERSION · build $APP_BUILD",
                color = c.muted.copy(alpha = 0.7f),
                fontSize = 11.sp,
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(
                        interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() },
                        indication = null,
                    ) {
                        if (devUnlocked) return@clickable
                        versionTaps++
                        val left = DEV_MENU_TAPS - versionTaps
                        when {
                            left <= 0 -> {
                                devUnlocked = true
                                vm.engine.settings.setDevMenuUnlocked(true)
                                devNote = "Меню разработчика открыто."
                            }
                            left <= 3 -> devNote = "Ещё $left — и откроется меню разработчика."
                        }
                    }
                    .padding(vertical = 8.dp),
            )
            if (devNote.isNotBlank() && !devUnlocked) StatusLine(devNote)
        }

        if (devUnlocked || s.detailedErrors) {
            Panel {
                SectionTitle("Меню разработчика")
                Spacer(Modifier.height(8.dp))
                Notice(
                    title = "Для диагностики",
                    text = "Подробные ошибки показывают служебные данные. Включайте, только если об этом попросила поддержка.",
                    tone = NoticeTone.Warning,
                )
                Spacer(Modifier.height(4.dp))
                ToggleRow("Подробные ошибки", s.detailedErrors) { vm.saveSettings(s.copy(detailedErrors = it)) }
                Spacer(Modifier.height(4.dp))
                SecondaryButton(
                    "Скрыть меню разработчика",
                    onClick = {
                        if (s.detailedErrors) vm.saveSettings(s.copy(detailedErrors = false))
                        vm.engine.settings.setDevMenuUnlocked(false)
                        devUnlocked = false
                        versionTaps = 0
                        devNote = ""
                    },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

private fun formatBytes(n: Long): String = when {
    n < 1024 -> "$n Б"
    n < 1024 * 1024 -> "${n / 1024} КБ"
    n < 1024L * 1024 * 1024 -> "${n / (1024 * 1024)} МБ"
    else -> String.format(java.util.Locale.forLanguageTag("ru"), "%.1f ГБ", n / (1024.0 * 1024 * 1024))
}
