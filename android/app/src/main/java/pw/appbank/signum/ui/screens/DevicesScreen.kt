package pw.appbank.signum.ui.screens

import androidx.compose.foundation.clickable

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.material3.OutlinedTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.theme.Brand400
import pw.appbank.signum.ui.theme.signumColors

@Composable
fun DevicesScreen(vm: SignumViewModel) {
    val s = vm.state.collectAsState().value
    val c = signumColors
    var appSearch by remember { mutableStateOf("") }
    var showOffloadedOnly by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { vm.refreshDevices() }
    LazyColumn(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            PageHeader("Устройства", "Подключите iPhone кабелем и нажмите «Доверять».") {
                SecondaryButton("Обновить", onClick = { vm.refreshDevices() })
            }
        }
        item {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                PrimaryButton("Доверять", onClick = vm::trust, modifier = Modifier.fillMaxWidth())
                SecondaryButton("Разрешить доступ", onClick = vm::requestUsb, modifier = Modifier.fillMaxWidth())
            }
            StatusLine(s.deviceMessage)
        }
        if (s.devicesLoading) {
            item {
                Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally) {
                    CircularProgressIndicator(color = Brand400)
                }
            }
        } else if (s.devices.isEmpty()) {
            item {
                Panel {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text("iPhone не найден", color = c.text, fontWeight = FontWeight.Bold)
                        Spacer(Modifier.height(6.dp))
                        Text(
                            "Подключите разблокированный iPhone кабелем. Подтвердите доступ на этом телефоне, на iPhone нажмите «Доверять».",
                            color = c.muted,
                            fontSize = 13.sp,
                        )
                    }
                }
            }
        }
        items(s.devices, key = { it.udid }) { d ->
            Panel(onClick = { vm.selectDevice(d) }) {
                Text(d.deviceName ?: "iPhone", color = c.text, fontWeight = FontWeight.Bold, fontSize = 18.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Spacer(Modifier.height(4.dp))
                Text(
                    listOfNotNull(d.productVersion?.let { "iOS $it" }, "кабель").joinToString(" · "),
                    color = c.muted,
                    fontSize = 13.sp,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(6.dp))
                Text(
                    if (d.paired) "Доверие есть" else "Доверия нет — разблокируйте iPhone и нажмите «Доверять» здесь",
                    color = if (d.paired) Brand400 else c.muted,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 12.sp,
                )
                if (s.selectedDevice?.udid == d.udid) {
                    Spacer(Modifier.height(4.dp))
                    Text("Выбрано", color = Brand400, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
                }
            }
        }
        item {
            SecondaryButton("Приложения на iPhone", onClick = vm::loadInstalled, modifier = Modifier.fillMaxWidth())
        }
        if (s.installed.isNotEmpty()) {
            item {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(
                        value = appSearch,
                        onValueChange = { appSearch = it },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        label = { Text("Поиск приложений") },
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                        FilterChip(
                            selected = showOffloadedOnly,
                            onClick = { showOffloadedOnly = !showOffloadedOnly },
                            label = { Text("Сгруженные · ${s.installed.count { it.isPlaceholder }}") },
                        )
                        val offloaded = s.installed.filter { it.isPlaceholder }
                        if (offloaded.isNotEmpty()) {
                            SecondaryButton("Восстановить все", onClick = {
                                offloaded.forEach(vm::restoreOffloaded)
                            })
                        }
                    }
                }
            }
        }
        items(s.installed.filter { app ->
            val q = appSearch.trim().lowercase()
            (!showOffloadedOnly || app.isPlaceholder) &&
                (q.isEmpty() || "${app.name.orEmpty()} ${app.bundleId} ${app.version.orEmpty()} ${app.appStoreId.orEmpty()}".lowercase().contains(q))
        }, key = { it.bundleId }) { app ->
            Panel {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Column(Modifier.weight(1f)) {
                        Text(app.name ?: "Приложение", color = c.text, fontWeight = FontWeight.SemiBold)
                        Text(app.bundleId, color = c.muted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        app.version?.let { Text("Версия $it", color = c.muted, fontSize = 12.sp) }
                    }
                    if (app.isPlaceholder) {
                        Column(horizontalAlignment = Alignment.End) {
                            Text("Сгружено", color = androidx.compose.ui.graphics.Color(0xFFFBBF24), fontSize = 11.sp)
                            Text("Восстановить", color = Brand400, fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
                                modifier = Modifier.padding(top = 6.dp).then(Modifier).clickable { vm.restoreOffloaded(app) })
                        }
                    }
                }
            }
        }
    }
}
