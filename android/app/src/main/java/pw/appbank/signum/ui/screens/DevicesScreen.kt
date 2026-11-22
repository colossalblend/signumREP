package pw.appbank.signum.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Apps
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.PhoneIphone
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Usb
import androidx.compose.material.icons.outlined.VerifiedUser
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.data.models.DeviceInfo
import pw.appbank.signum.data.models.InstalledApp
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.EmptyState
import pw.appbank.signum.ui.components.FilterPills
import pw.appbank.signum.ui.components.InfoRow
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.SectionHeader
import pw.appbank.signum.ui.components.SignumField
import pw.appbank.signum.ui.components.SkeletonRow
import pw.appbank.signum.ui.components.StatusChip
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.components.TextAction
import pw.appbank.signum.ui.theme.signumColors

@Composable
fun DevicesScreen(vm: SignumViewModel) {
    val s by vm.state.collectAsState()
    val c = signumColors
    var appSearch by rememberSaveable { mutableStateOf("") }
    var appFilter by rememberSaveable { mutableStateOf("all") }
    LaunchedEffect(Unit) { vm.refreshDevices() }

    val offloadedCount = s.installed.count { it.isPlaceholder }
    val apps = s.installed
        .distinctBy { it.bundleId }
        .filter { app ->
            val q = appSearch.trim().lowercase()
            (appFilter != "offloaded" || app.isPlaceholder) &&
                (q.isEmpty() || "${app.name.orEmpty()} ${app.bundleId} ${app.version.orEmpty()} ${app.appStoreId.orEmpty()}".lowercase().contains(q))
        }
        .sortedBy { (it.name ?: it.bundleId).lowercase() }

    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 18.dp, end = 18.dp, top = 12.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            PageHeader("Устройства", "iPhone и iPad, подключённые к телефону кабелем") {
                SecondaryButton(
                    "Обновить",
                    onClick = { vm.refreshDevices() },
                    icon = Icons.Outlined.Refresh,
                    loading = s.devicesLoading,
                )
            }
        }
        if (s.deviceMessage.isNotBlank()) {
            item { StatusLine(s.deviceMessage) }
        }

        if (s.devicesLoading && s.devices.isEmpty()) {
            item { SkeletonRow(reduceMotion = s.settings.staticLoaders) }
        } else if (s.devices.isEmpty()) {
            item {
                EmptyState(
                    icon = Icons.Outlined.Usb,
                    title = "iPhone не найден",
                    text = "Подключите разблокированный iPhone кабелем (USB-C или через переходник OTG). " +
                        "Разрешите Signum доступ к устройству на этом телефоне, затем на iPhone нажмите «Доверять».",
                ) {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        PrimaryButton("Разрешить доступ", onClick = vm::requestUsb, modifier = Modifier.fillMaxWidth())
                        SecondaryButton("Доверять устройству", onClick = vm::trust, modifier = Modifier.fillMaxWidth())
                    }
                }
            }
        } else {
            item { SectionHeader("Подключённые · ${s.devices.size}") }
            items(s.devices.distinctBy { it.udid }, key = { "d-" + it.udid }) { d ->
                DeviceCard(
                    d = d,
                    selected = s.selectedDevice?.udid == d.udid,
                    multiple = s.devices.size > 1,
                    onSelect = { vm.selectDevice(d) },
                    onTrust = vm::trust,
                    onAccess = vm::requestUsb,
                )
            }
        }

        if (s.devices.isNotEmpty()) {
            item {
                SectionHeader("Установленные приложения") {
                    if (s.installed.isNotEmpty()) {
                        TextAction("Обновить", onClick = vm::loadInstalled, enabled = !s.installedLoading)
                    }
                }
            }
            if (s.installed.isEmpty()) {
                item {
                    if (s.installedLoading) {
                        SkeletonRow(reduceMotion = s.settings.staticLoaders)
                    } else {
                        Panel {
                            Text(
                                "Список приложений не загружается автоматически — это ускоряет открытие страницы.",
                                color = c.muted,
                                fontSize = 13.sp,
                                lineHeight = 18.sp,
                            )
                            Spacer(Modifier.height(10.dp))
                            SecondaryButton(
                                "Показать приложения",
                                onClick = vm::loadInstalled,
                                icon = Icons.Outlined.Apps,
                                modifier = Modifier.fillMaxWidth(),
                            )
                        }
                    }
                }
            } else {
                item {
                    SignumField(
                        appSearch,
                        { appSearch = it },
                        placeholder = "Поиск по установленным приложениям",
                        leading = { Icon(Icons.Outlined.Search, contentDescription = null) },
                        trailing = if (appSearch.isNotEmpty()) {
                            {
                                IconButton(onClick = { appSearch = "" }) {
                                    Icon(Icons.Outlined.Close, contentDescription = "Очистить поиск", tint = c.muted)
                                }
                            }
                        } else {
                            null
                        },
                    )
                }
                item {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        FilterPills(
                            options = listOf(
                                Triple("all", "Все", s.installed.size),
                                Triple("offloaded", "Сгруженные", offloadedCount),
                            ),
                            selected = appFilter,
                            onSelect = { appFilter = it },
                            modifier = Modifier.weight(1f),
                        )
                        if (offloadedCount > 0) {
                            TextAction("Восстановить все", onClick = vm::restoreAllOffloaded)
                        }
                    }
                }
                if (apps.isEmpty()) {
                    item { Text("Ничего не найдено", color = c.muted, fontSize = 13.sp) }
                }
                items(apps, key = { "a-" + it.bundleId }) { app ->
                    InstalledRow(app, onRestore = { vm.restoreOffloaded(app) })
                }
            }
        }
    }
}

@Composable
private fun DeviceCard(
    d: DeviceInfo,
    selected: Boolean,
    multiple: Boolean,
    onSelect: () -> Unit,
    onTrust: () -> Unit,
    onAccess: () -> Unit,
) {
    val c = signumColors
    val shape = RoundedCornerShape(18.dp)
    Panel(
        modifier = if (selected && multiple) Modifier.border(1.5.dp, c.brand.copy(alpha = 0.6f), shape) else Modifier,
        onClick = if (multiple) onSelect else null,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier
                    .size(48.dp)
                    .clip(RoundedCornerShape(14.dp))
                    .background(c.brand.copy(alpha = 0.12f)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.Outlined.PhoneIphone, contentDescription = null, tint = c.brandSoft, modifier = Modifier.size(26.dp))
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    d.deviceName ?: "iPhone",
                    color = c.text,
                    fontWeight = FontWeight.Bold,
                    fontSize = 17.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(4.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    StatusChip(if (d.paired) "Доверие есть" else "Нет доверия", if (d.paired) c.success else c.warning)
                    if (selected && multiple) {
                        Spacer(Modifier.width(6.dp))
                        StatusChip("Выбрано", c.brandSoft)
                    }
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        d.productVersion?.let { InfoRow("Система", "iOS $it") }
        d.productType?.takeIf { it.isNotBlank() }?.let { InfoRow("Модель", it) }
        d.battery?.let { InfoRow("Заряд", "$it%") }
        InfoRow("Подключение", if (d.connection == "usb") "Кабель" else d.connection)
        if (!d.paired) {
            Spacer(Modifier.height(10.dp))
            Text(
                "Разблокируйте iPhone и нажмите «Доверять устройству». На iPhone появится запрос — подтвердите его кодом-паролем.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(10.dp))
            PrimaryButton("Доверять устройству", onClick = onTrust, icon = Icons.Outlined.VerifiedUser, modifier = Modifier.fillMaxWidth())
            Spacer(Modifier.height(8.dp))
            SecondaryButton("Разрешить доступ к USB", onClick = onAccess, modifier = Modifier.fillMaxWidth())
        } else {
            Spacer(Modifier.height(4.dp))
            TextAction("Переподключить доверие", onClick = onTrust, color = c.muted)
        }
    }
}

@Composable
private fun InstalledRow(app: InstalledApp, onRestore: () -> Unit) {
    val c = signumColors
    Panel {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier
                    .size(40.dp)
                    .clip(RoundedCornerShape(11.dp))
                    .background(c.track),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    (app.name ?: app.bundleId).firstOrNull()?.uppercaseChar()?.toString() ?: "?",
                    color = c.muted,
                    fontWeight = FontWeight.Bold,
                )
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    app.name ?: app.bundleId,
                    color = c.text,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 14.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    listOfNotNull(app.version?.let { "v$it" }, app.bundleId).joinToString(" · "),
                    color = c.muted,
                    fontSize = 11.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (app.isPlaceholder) {
                Spacer(Modifier.width(8.dp))
                Column(horizontalAlignment = Alignment.End) {
                    StatusChip("Сгружено", c.warning)
                    TextAction("Восстановить", onClick = onRestore)
                }
            }
        }
    }
}
