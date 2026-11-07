package pw.appbank.signum.ui.screens

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.foundation.clickable
import pw.appbank.signum.ui.theme.Muted
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.data.models.APP_VERSION
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PickerDialog
import pw.appbank.signum.ui.components.PickerOption
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.components.SectionTitle
import pw.appbank.signum.ui.components.SegmentedControl
import pw.appbank.signum.ui.components.SelectField
import pw.appbank.signum.ui.theme.Brand500
import pw.appbank.signum.ui.theme.signumColors

@Composable
fun SettingsScreen(vm: SignumViewModel) {
    val full = vm.state.collectAsState().value
    val s = full.settings
    val c = signumColors
    LaunchedEffect(Unit) { vm.loadStoreRegion() }
    var cacheBytes by remember { mutableLongStateOf(vm.engine.cacheSize()) }
    LaunchedEffect(Unit) { cacheBytes = vm.engine.cacheSize() }
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
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        PageHeader("Настройки", "Тема, оформление и кэш")
        Panel {
            SectionTitle("Тема")
            Spacer(Modifier.height(10.dp))
            SegmentedControl(
                options = listOf("dark" to "Тёмная", "light" to "Светлая", "system" to "Система"),
                selected = s.theme,
                onSelect = { vm.saveSettings(s.copy(theme = it)) },
            )
        }
        Panel {
            SectionTitle("Интерфейс")
            ToggleRow("Подробные сообщения об ошибках", s.detailedErrors) { vm.saveSettings(s.copy(detailedErrors = it)) }
            ToggleRow("Упрощённый вид", s.liteDesign) { vm.saveSettings(s.copy(liteDesign = it)) }
        }
        Panel {
            SectionTitle("Apple-инструмент")
            Spacer(Modifier.height(6.dp))
            Text(
                "На Android Auto использует V6, затем V5 и V4. V6 работает через Configurator-профиль и после переключения может запросить повторный вход.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(8.dp))
            SegmentedControl(
                options = listOf("auto" to "Auto", "v6" to "V6", "v5" to "V5", "v4" to "V4"),
                selected = s.ipatoolEngine,
                onSelect = { vm.saveSettings(s.copy(ipatoolEngine = it)) },
            )
        }
        Panel {
            SectionTitle("Прокси для Apple")
            Spacer(Modifier.height(4.dp))
            Text(
                "Нужен там, где Apple недоступна напрямую. Работает и для загрузок — " +
                    "VPN на весь телефон включать не требуется.",
                color = Muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(8.dp))
            ToggleRow("Использовать прокси", s.proxyEnabled) {
                vm.saveSettings(s.copy(proxyEnabled = it))
            }
            if (s.proxyEnabled) {
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf("socks5", "socks4", "http").forEach { kind ->
                        val active = s.proxyKind == kind
                        Text(
                            kind,
                            color = if (active) Brand500 else Muted,
                            fontSize = 13.sp,
                            modifier = Modifier
                                .clickable { vm.saveSettings(s.copy(proxyKind = kind)) }
                                .padding(horizontal = 10.dp, vertical = 6.dp),
                        )
                    }
                }
                Spacer(Modifier.height(6.dp))
                OutlinedTextField(
                    value = s.proxyHost,
                    onValueChange = { vm.saveSettings(s.copy(proxyHost = it.trim())) },
                    label = { Text("Адрес") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(6.dp))
                OutlinedTextField(
                    value = if (s.proxyPort == 0) "" else s.proxyPort.toString(),
                    onValueChange = { text ->
                        vm.saveSettings(s.copy(proxyPort = text.filter { it.isDigit() }.take(5).toIntOrNull() ?: 0))
                    },
                    label = { Text("Порт") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(6.dp))
                OutlinedTextField(
                    value = s.proxyUser,
                    onValueChange = { vm.saveSettings(s.copy(proxyUser = it.trim())) },
                    label = { Text("Имя (если нужно)") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(6.dp))
                OutlinedTextField(
                    value = s.proxyPassword,
                    onValueChange = { vm.saveSettings(s.copy(proxyPassword = it)) },
                    label = { Text("Пароль (если нужно)") },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    modifier = Modifier.fillMaxWidth(),
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
                Text(full.regionMessage, color = Color(0xFFFCD34D), fontSize = 12.sp, lineHeight = 17.sp)
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
            val regions = vm.storeRegions()
            val currentRegion = regions.firstOrNull { it.code.equals(full.regionCode, ignoreCase = true) }
            var regionsOpen by remember { mutableStateOf(false) }
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
                modifier = Modifier.fillMaxWidth(),
            )
        }
        Panel {
            SectionTitle("Загруженные файлы")
            Spacer(Modifier.height(6.dp))
            Text(formatBytes(cacheBytes), color = c.muted, fontSize = 13.sp)
            Spacer(Modifier.height(8.dp))
            SecondaryButton(
                "Очистить кэш",
                onClick = {
                    vm.engine.clearCache()
                    cacheBytes = vm.engine.cacheSize()
                },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        Panel {
            SectionTitle("Запись для поддержки")
            Spacer(Modifier.height(6.dp))
            ToggleRow("Отправлять журнал работы", full.diagAuto) { vm.setDiagAuto(it) }
            Text(
                "Запись идёт всё время работы Signum и уходит разработчику, когда вы её закрываете — " +
                    "по ней находят ошибки, которые иначе видны только у вас. Личное вырезается до отправки: " +
                    "Apple ID, имя, пароли, коды из SMS и токены.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(6.dp))
            ToggleRow("Записывать прямо сейчас", full.diagRecording) { vm.setDiagRecording(it) }
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
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            if (full.diagMessage.isNotBlank()) {
                Spacer(Modifier.height(8.dp))
                StatusLine(full.diagMessage)
            }
        }
        Panel {
            SectionTitle("Signum")
            Spacer(Modifier.height(6.dp))
            Text("Версия $APP_VERSION", color = c.muted, fontSize = 13.sp)
        }
    }
}

@Composable
private fun ToggleRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    val c = signumColors
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = c.text, modifier = Modifier.weight(1f))
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            colors = SwitchDefaults.colors(checkedTrackColor = Brand500),
        )
    }
}

private fun formatBytes(n: Long): String = when {
    n < 1024 -> "$n Б"
    n < 1024 * 1024 -> "${n / 1024} КБ"
    else -> "${n / (1024 * 1024)} МБ"
}
