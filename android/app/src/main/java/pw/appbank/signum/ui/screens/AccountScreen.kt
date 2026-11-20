package pw.appbank.signum.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.SecondaryButton
import androidx.compose.foundation.layout.width
import androidx.compose.ui.text.style.TextOverflow
import pw.appbank.signum.ui.theme.Brand400
import pw.appbank.signum.ui.components.SignumField
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.theme.signumColors

@Composable
@Suppress("UNUSED_PARAMETER")
fun AccountScreen(vm: SignumViewModel, goInstalls: () -> Unit) {
    val s = vm.state.collectAsState().value
    val c = signumColors
    var showPw by remember { mutableStateOf(false) }
    LaunchedEffect(s.auth.authenticated) {
        if (s.auth.authenticated && s.history.isEmpty()) vm.loadHistory(false)
    }
    LazyColumn(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            PageHeader("Аккаунт", "Вход в Apple ID")
        }
        item {
            Panel(Modifier.fillMaxWidth()) {
                if (s.auth.authenticated) {
                    Text(s.auth.email ?: "", color = c.text, fontWeight = FontWeight.Bold, fontSize = 18.sp, maxLines = 2)
                    Spacer(Modifier.height(4.dp))
                    Text(
                        if (s.auth.historySessionSaved) "История покупок доступна по запросу" else "Можно загрузить историю покупок",
                        color = c.muted,
                        fontSize = 13.sp,
                    )
                    Spacer(Modifier.height(12.dp))
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        SecondaryButton("Выйти", onClick = vm::logout, modifier = Modifier.fillMaxWidth())
                        SecondaryButton("Обновить историю", onClick = { vm.loadHistory(true) }, modifier = Modifier.fillMaxWidth())
                    }
                    // Список сохранённых Apple ID.
                    //
                    // Данные для него приходили и раньше, но на экран не
                    // попадали: человек не мог вернуться к прежнему аккаунту,
                    // не вводя пароль и код заново.
                    val others = s.auth.accounts.filter { !it.active }
                    if (others.isNotEmpty()) {
                        Spacer(Modifier.height(14.dp))
                        Text("Другие Apple ID", color = c.muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                        Spacer(Modifier.height(6.dp))
                        others.forEach { acc ->
                            Row(
                                Modifier.fillMaxWidth().padding(vertical = 4.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Column(Modifier.weight(1f)) {
                                    Text(
                                        acc.email,
                                        color = c.text,
                                        fontSize = 14.sp,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                    Text(
                                        if (acc.sessionSaved) "вход сохранён" else "нужен вход заново",
                                        color = c.muted,
                                        fontSize = 11.sp,
                                    )
                                }
                                Spacer(Modifier.width(8.dp))
                                Text(
                                    "Выбрать",
                                    color = Brand400,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Bold,
                                    modifier = Modifier.clickable { vm.switchAccount(acc.email) },
                                )
                                Spacer(Modifier.width(14.dp))
                                Text(
                                    "Убрать",
                                    color = c.muted,
                                    fontSize = 12.sp,
                                    modifier = Modifier.clickable { vm.removeAccount(acc.email) },
                                )
                            }
                        }
                    }
                } else if (s.loginStep == "two_factor") {
                    Text("Код подтверждения Apple", color = c.text, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(8.dp))
                    Text("Проверьте доверенные Apple-устройства и SMS на доверенном номере.", color = c.muted, fontSize = 12.sp, lineHeight = 17.sp)
                    Spacer(Modifier.height(8.dp))
                    SignumField(
                        s.loginCode,
                        vm::setLoginCode,
                        placeholder = "6 цифр",
                        keyboard = KeyboardOptions(keyboardType = KeyboardType.Number),
                    )
                    Spacer(Modifier.height(10.dp))
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        PrimaryButton("Подтвердить", onClick = vm::submitLogin, loading = s.loginBusy, modifier = Modifier.fillMaxWidth())
                        SecondaryButton("Новый код", onClick = vm::resendLoginCode, modifier = Modifier.fillMaxWidth())
                    }
                } else {
                    Text("Apple ID", color = c.text, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(8.dp))
                    SignumField(s.loginEmail, vm::setLoginEmail, placeholder = "email@icloud.com")
                    Spacer(Modifier.height(8.dp))
                    SignumField(
                        s.loginPassword,
                        vm::setLoginPassword,
                        placeholder = "Пароль",
                        visual = if (showPw) VisualTransformation.None else PasswordVisualTransformation(),
                        trailing = {
                            IconButton(onClick = { showPw = !showPw }) {
                                Icon(if (showPw) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility, null, tint = c.muted)
                            }
                        },
                    )
                    Spacer(Modifier.height(12.dp))
                    PrimaryButton("Войти", onClick = vm::submitLogin, loading = s.loginBusy, modifier = Modifier.fillMaxWidth())
                }
                StatusLine(s.loginMessage)
            }
        }
        if (s.auth.authenticated) {
            item {
                SignumField(s.historySearch, vm::setHistorySearch, placeholder = "Поиск в покупках")
            }
            if (s.historyNeeds2fa) {
                item {
                    Panel {
                        Text("Код для истории покупок", color = c.text, fontWeight = FontWeight.SemiBold)
                        Spacer(Modifier.height(8.dp))
                        SignumField(s.history2fa, vm::setHistory2fa, placeholder = "6 цифр", keyboard = KeyboardOptions(keyboardType = KeyboardType.Number))
                        Spacer(Modifier.height(8.dp))
                        PrimaryButton("Загрузить историю", onClick = { vm.loadHistory(true) }, loading = s.historyLoading, modifier = Modifier.fillMaxWidth())
                    }
                }
            }
            item { StatusLine(s.historyMessage) }
            val q = s.historySearch.lowercase()
            val rows = s.history.filter {
                q.isBlank() || it.name.lowercase().contains(q) || it.adamId.contains(q) || (it.bundleId?.lowercase()?.contains(q) == true)
            }
            val visibleIds = rows.map { it.adamId }
            val allVisibleSelected = visibleIds.isNotEmpty() && visibleIds.all { s.historySelected.contains(it) }
            if (rows.isNotEmpty()) {
                item {
                    Panel {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Row(
                                Modifier.fillMaxWidth().clickable { vm.toggleHistorySelectAll(visibleIds) },
                                horizontalArrangement = Arrangement.spacedBy(10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                HistoryCheck(allVisibleSelected, c.brand)
                                Text(
                                    if (s.historySelected.isEmpty()) "Выберите приложения" else "Выбрано ${s.historySelected.size}",
                                    color = c.text,
                                    fontSize = 13.sp,
                                )
                            }
                            if (s.historySelected.isNotEmpty()) {
                                PrimaryButton("На iPhone", onClick = { vm.enqueueSelectedHistory(true) }, loading = s.historyBusy, modifier = Modifier.fillMaxWidth())
                                SecondaryButton("Скачать", onClick = { vm.enqueueSelectedHistory(false) }, modifier = Modifier.fillMaxWidth())
                                SecondaryButton("Снять", onClick = vm::clearHistorySelection, modifier = Modifier.fillMaxWidth())
                            }
                        }
                    }
                }
            }
            items(rows, key = { it.adamId + it.name }) { item ->
                val selected = s.historySelected.contains(item.adamId)
                Panel {
                    Column {
                        Row(
                            Modifier.fillMaxWidth().clickable { vm.toggleHistorySelected(item.adamId) },
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            HistoryCheck(selected, c.brand)
                            Column(Modifier.weight(1f)) {
                                Text(item.name, color = c.text, fontWeight = FontWeight.SemiBold)
                                Text(item.artist ?: item.adamId, color = c.muted, fontSize = 12.sp)
                            }
                        }
                        Spacer(Modifier.height(8.dp))
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            PrimaryButton("Установить", onClick = { vm.installFromHistory(item, true) }, modifier = Modifier.fillMaxWidth())
                            SecondaryButton("Только файл", onClick = { vm.installFromHistory(item, false) }, modifier = Modifier.fillMaxWidth())
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun HistoryCheck(checked: Boolean, brand: Color) {
    Box(
        Modifier
            .size(22.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(if (checked) brand else Color.Transparent)
            .border(1.dp, if (checked) brand else Color.White.copy(alpha = 0.2f), RoundedCornerShape(6.dp)),
        contentAlignment = Alignment.Center,
    ) {
        if (checked) {
            Text("✓", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
        }
    }
}
