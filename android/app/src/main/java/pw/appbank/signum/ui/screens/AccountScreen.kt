package pw.appbank.signum.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.InstallMobile
import androidx.compose.material.icons.outlined.Logout
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Search
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.compose.AsyncImage
import pw.appbank.signum.data.models.AppleAccountInfo
import pw.appbank.signum.data.models.HISTORY_SOURCE_LIBRARY
import pw.appbank.signum.data.models.PurchaseHistoryApp
import pw.appbank.signum.data.models.SessionState
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.UiState
import pw.appbank.signum.ui.components.CheckBox
import pw.appbank.signum.ui.components.ConfirmDialog
import pw.appbank.signum.ui.components.DangerButton
import pw.appbank.signum.ui.components.EmptyState
import pw.appbank.signum.ui.components.Notice
import pw.appbank.signum.ui.components.NoticeTone
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.PrimaryButton
import pw.appbank.signum.ui.components.RowDivider
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.SectionHeader
import pw.appbank.signum.ui.components.SectionTitle
import pw.appbank.signum.ui.components.SignumField
import pw.appbank.signum.ui.components.SkeletonRow
import pw.appbank.signum.ui.components.StatusChip
import pw.appbank.signum.ui.components.StatusLine
import pw.appbank.signum.ui.components.TextAction
import pw.appbank.signum.ui.theme.signumColors
import java.text.Normalizer

@Composable
fun AccountScreen(vm: SignumViewModel, goInstalls: () -> Unit) {
    val s by vm.state.collectAsState()
    val c = signumColors
    var confirmLogout by remember { mutableStateOf(false) }
    var confirmRemove by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(s.auth.authenticated, s.auth.email) {
        if (s.auth.authenticated && s.history.isEmpty()) vm.loadHistory(false)
    }
    // Вход в добавленный аккаунт завершился — форму закрываем.
    LaunchedEffect(s.auth.email, s.loginStep, s.loginBusy) { vm.finishAddAccountIfDone() }

    val showLogin = !s.auth.authenticated || s.addingAccount
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 18.dp, end = 18.dp, top = 12.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            PageHeader(
                "Аккаунт",
                if (s.auth.authenticated) "Apple ID, история покупок и сохранённые входы" else "Вход в Apple ID",
            )
        }
        if (s.auth.authenticated) {
            item {
                ActiveAccountCard(
                    s = s,
                    onRefresh = { vm.loadHistory(true) },
                    onAdd = vm::startAddAccount,
                    onLogout = { confirmLogout = true },
                )
            }
            val others = s.auth.accounts.filter { !it.active }
            if (others.isNotEmpty()) {
                item {
                    Panel {
                        SectionTitle("Другие Apple ID")
                        Spacer(Modifier.height(6.dp))
                        others.forEachIndexed { index, acc ->
                            if (index > 0) RowDivider()
                            OtherAccountRow(
                                acc = acc,
                                onSelect = { vm.switchAccount(acc.email) },
                                onRelogin = { vm.startRelogin(acc.email) },
                                onRemove = { confirmRemove = acc.email },
                            )
                        }
                    }
                }
            }
        }
        if (showLogin) {
            item { LoginCard(vm, s) }
        } else if (s.loginMessage.isNotBlank()) {
            item { StatusLine(s.loginMessage) }
        }
        if (s.auth.authenticated && !s.addingAccount) {
            historySection(vm, s, goInstalls)
        }
    }

    if (confirmLogout) {
        ConfirmDialog(
            title = "Выйти из Apple ID?",
            text = "Сохранённый вход будет удалён. Аккаунт останется в списке — на него можно вернуться, введя пароль.",
            confirm = "Выйти",
            onConfirm = {
                confirmLogout = false
                vm.logout()
            },
            onDismiss = { confirmLogout = false },
        )
    }
    confirmRemove?.let { email ->
        ConfirmDialog(
            title = "Убрать $email?",
            text = "Сохранённый вход, история покупок и настройки этого Apple ID будут удалены с телефона.",
            confirm = "Убрать",
            onConfirm = {
                confirmRemove = null
                vm.removeAccount(email)
            },
            onDismiss = { confirmRemove = null },
        )
    }
}

@Composable
private fun ActiveAccountCard(s: UiState, onRefresh: () -> Unit, onAdd: () -> Unit, onLogout: () -> Unit) {
    val c = signumColors
    val email = s.auth.email.orEmpty()
    val state = s.auth.accounts.firstOrNull { it.active }?.sessionState
    Panel {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Avatar(email, 48)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    email,
                    color = c.text,
                    fontWeight = FontWeight.Bold,
                    fontSize = 16.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(4.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    StatusChip("Активный", c.success)
                    if (state != null && state != SessionState.active && state != SessionState.saved) {
                        Spacer(Modifier.width(6.dp))
                        StatusChip(sessionLabel(state), c.warning)
                    }
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        Text(
            when {
                s.historyLoading -> "Загружаем историю покупок…"
                s.history.isNotEmpty() -> "В истории покупок: ${s.history.size}" + if (s.historyHasMore) "+" else ""
                s.auth.historySessionSaved -> "Вход в историю покупок сохранён"
                else -> "Историю покупок можно загрузить ниже"
            },
            color = c.muted,
            fontSize = 13.sp,
        )
        Spacer(Modifier.height(12.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SecondaryButton(
                "Обновить",
                onClick = onRefresh,
                icon = Icons.Outlined.Refresh,
                loading = s.historyLoading,
                modifier = Modifier.weight(1f),
            )
            SecondaryButton("Добавить", onClick = onAdd, icon = Icons.Outlined.Add, modifier = Modifier.weight(1f))
        }
        Spacer(Modifier.height(8.dp))
        DangerButton("Выйти", onClick = onLogout, icon = Icons.Outlined.Logout, modifier = Modifier.fillMaxWidth())
    }
}

@Composable
private fun OtherAccountRow(acc: AppleAccountInfo, onSelect: () -> Unit, onRelogin: () -> Unit, onRemove: () -> Unit) {
    val c = signumColors
    val ready = acc.sessionSaved && (acc.sessionState == SessionState.active || acc.sessionState == SessionState.saved)
    Row(
        Modifier.fillMaxWidth().padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Avatar(acc.email, 36)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(acc.email, color = c.text, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                if (ready) "Готов к переключению" else sessionLabel(acc.sessionState),
                color = if (ready) c.muted else c.warning,
                fontSize = 11.sp,
            )
        }
        if (ready) {
            TextAction("Выбрать", onClick = onSelect)
        } else {
            TextAction("Войти", onClick = onRelogin)
        }
        TextAction("Убрать", onClick = onRemove, color = c.muted)
    }
}

@Composable
private fun Avatar(email: String, size: Int) {
    val c = signumColors
    Box(
        Modifier
            .size(size.dp)
            .clip(CircleShape)
            .background(c.brandBrush),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            email.trim().firstOrNull()?.uppercaseChar()?.toString() ?: "?",
            color = androidx.compose.ui.graphics.Color.White,
            fontWeight = FontWeight.Bold,
            fontSize = (size * 0.42f).sp,
        )
    }
}

@Composable
private fun LoginCard(vm: SignumViewModel, s: UiState) {
    val c = signumColors
    var showPw by rememberSaveable { mutableStateOf(false) }
    Panel {
        if (s.loginStep == "two_factor") {
            SectionTitle("Код подтверждения Apple")
            Spacer(Modifier.height(6.dp))
            Text(
                "Введите 6-значный код с доверенного iPhone, iPad или Mac. Если устройств нет — код придёт в SMS.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(10.dp))
            SignumField(
                s.loginCode,
                vm::setLoginCode,
                placeholder = "6 цифр",
                keyboard = KeyboardOptions(keyboardType = KeyboardType.NumberPassword, imeAction = ImeAction.Done),
            )
            Spacer(Modifier.height(10.dp))
            PrimaryButton(
                "Подтвердить",
                onClick = vm::submitLogin,
                loading = s.loginBusy,
                enabled = s.loginCode.length == 6,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            SecondaryButton("Запросить новый код", onClick = vm::resendLoginCode, enabled = !s.loginBusy, modifier = Modifier.fillMaxWidth())
        } else {
            SectionTitle(if (s.addingAccount) "Добавить Apple ID" else "Войти в Apple ID")
            Spacer(Modifier.height(4.dp))
            Text(
                "Пароль хранится только на этом телефоне в зашифрованном виде.",
                color = c.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(10.dp))
            SignumField(
                s.loginEmail,
                vm::setLoginEmail,
                placeholder = "email@icloud.com",
                keyboard = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next),
            )
            Spacer(Modifier.height(8.dp))
            SignumField(
                s.loginPassword,
                vm::setLoginPassword,
                placeholder = "Пароль",
                visual = if (showPw) VisualTransformation.None else PasswordVisualTransformation(),
                keyboard = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
                trailing = {
                    IconButton(onClick = { showPw = !showPw }) {
                        Icon(
                            if (showPw) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility,
                            contentDescription = if (showPw) "Скрыть пароль" else "Показать пароль",
                            tint = c.muted,
                        )
                    }
                },
            )
            Spacer(Modifier.height(12.dp))
            PrimaryButton(
                "Войти",
                onClick = vm::submitLogin,
                loading = s.loginBusy,
                enabled = s.loginEmail.isNotBlank() && s.loginPassword.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            )
        }
        if (s.addingAccount) {
            Spacer(Modifier.height(8.dp))
            SecondaryButton("Отмена", onClick = vm::cancelAddAccount, enabled = !s.loginBusy, modifier = Modifier.fillMaxWidth())
        }
        StatusLine(s.loginMessage)
    }
}

private fun androidx.compose.foundation.lazy.LazyListScope.historySection(
    vm: SignumViewModel,
    s: UiState,
    goInstalls: () -> Unit,
) {
    item {
        SectionHeader("История покупок") {
            if (s.history.isNotEmpty()) {
                Text("${s.history.size}", color = signumColors.muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
            }
        }
    }
    item {
        SignumField(
            s.historySearch,
            vm::setHistorySearch,
            placeholder = "Название, разработчик или bundle id",
            leading = { Icon(Icons.Outlined.Search, contentDescription = null) },
        )
    }
    if (s.historyNeeds2fa) {
        item {
            Panel {
                SectionTitle("Код для истории покупок")
                Spacer(Modifier.height(4.dp))
                Text(
                    "Полная история берётся с reportaproblem.apple.com, Apple просит код. Нет кода под рукой — попробуйте без кода.",
                    color = signumColors.muted,
                    fontSize = 12.sp,
                    lineHeight = 17.sp,
                )
                Spacer(Modifier.height(8.dp))
                SignumField(
                    s.history2fa,
                    vm::setHistory2fa,
                    placeholder = "6 цифр",
                    keyboard = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                )
                Spacer(Modifier.height(8.dp))
                PrimaryButton(
                    "Загрузить историю",
                    onClick = { vm.loadHistory(true) },
                    loading = s.historyLoading,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                SecondaryButton(
                    "Попробовать без кода",
                    onClick = vm::loadHistoryViaLibrary,
                    loading = s.historyLibraryLoading,
                    enabled = !s.historyLoading,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
    if (s.historyMessage.isNotBlank()) {
        item { StatusLine(s.historyMessage) }
    }
    if (!s.historyNeeds2fa && s.historyMessage.isNotBlank() && s.history.isEmpty() &&
        s.historySource != HISTORY_SOURCE_LIBRARY && !s.historyLoading
    ) {
        item {
            SecondaryButton(
                "Попробовать без кода",
                onClick = vm::loadHistoryViaLibrary,
                loading = s.historyLibraryLoading,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
    if (!s.historyNeeds2fa && s.historySource == HISTORY_SOURCE_LIBRARY && s.history.isNotEmpty()) {
        item {
            Notice(
                title = "Не все приложения могут отображаться в истории покупок",
                text = "Список получен без кода подтверждения. Ставить и скачивать отсюда можно, но полную историю лучше открыть с кодом.",
                tone = NoticeTone.Warning,
            ) {
                SecondaryButton(
                    "Повторить с кодом",
                    onClick = vm::retryHistoryWithCode,
                    loading = s.historyLoading,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
    if (s.historyLoading && s.history.isEmpty()) {
        items(4) { SkeletonRow(reduceMotion = s.settings.liteReduceMotion) }
        return
    }
    // Дубли по adamId бывали в ответе Apple (повторная покупка) — с ними
    // LazyColumn падал с «Key was already used».
    val all = s.history.distinctBy { it.adamId }
    val rows = all.filter { historyMatches(it, s.historySearch) }
    if (all.isEmpty() && !s.historyNeeds2fa && s.historyMessage.isBlank()) {
        item {
            EmptyState(
                icon = Icons.Outlined.History,
                title = "История пока пуста",
                text = "Нажмите «Обновить», чтобы загрузить покупки этого Apple ID.",
            )
        }
        return
    }
    if (all.isNotEmpty() && rows.isEmpty()) {
        item { Text("Ничего не найдено", color = signumColors.muted, fontSize = 13.sp) }
    }
    val visibleIds = rows.map { it.adamId }
    if (rows.isNotEmpty()) {
        item {
            val allSelected = visibleIds.isNotEmpty() && visibleIds.all { s.historySelected.contains(it) }
            Panel {
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(10.dp))
                        .clickable { vm.toggleHistorySelectAll(visibleIds) },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CheckBox(allSelected)
                    Spacer(Modifier.width(10.dp))
                    Text(
                        if (s.historySelected.isEmpty()) "Выбрать все" else "Выбрано: ${s.historySelected.size}",
                        color = signumColors.text,
                        fontSize = 14.sp,
                        modifier = Modifier.weight(1f),
                    )
                    if (s.historySelected.isNotEmpty()) {
                        TextAction("Снять", onClick = vm::clearHistorySelection, color = signumColors.muted)
                    }
                }
                if (s.historySelected.isNotEmpty()) {
                    Spacer(Modifier.height(10.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        PrimaryButton(
                            "На iPhone",
                            onClick = {
                                vm.enqueueSelectedHistory(true)
                                goInstalls()
                            },
                            icon = Icons.Outlined.InstallMobile,
                            loading = s.historyBusy,
                            modifier = Modifier.weight(1f),
                        )
                        SecondaryButton(
                            "Скачать",
                            onClick = { vm.enqueueSelectedHistory(false) },
                            icon = Icons.Outlined.Download,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
    items(rows, key = { "h-" + it.adamId }) { item ->
        HistoryRow(
            item = item,
            selected = s.historySelected.contains(item.adamId),
            onToggle = { vm.toggleHistorySelected(item.adamId) },
            onInstall = { vm.installFromHistory(item, true) },
            onExport = { vm.installFromHistory(item, false) },
        )
    }
    if (s.historyHasMore && s.historySearch.isBlank()) {
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton(
                    "Загрузить ещё",
                    onClick = { vm.loadMoreHistory(false) },
                    loading = s.historyLoadingMore,
                    modifier = Modifier.weight(1f),
                )
                SecondaryButton(
                    "До конца",
                    onClick = { vm.loadMoreHistory(true) },
                    enabled = !s.historyLoadingMore,
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

@Composable
private fun HistoryRow(
    item: PurchaseHistoryApp,
    selected: Boolean,
    onToggle: () -> Unit,
    onInstall: () -> Unit,
    onExport: () -> Unit,
) {
    val c = signumColors
    Panel {
        Row(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .clickable(onClick = onToggle),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CheckBox(selected)
            Spacer(Modifier.width(12.dp))
            if (!item.artwork.isNullOrBlank()) {
                AsyncImage(
                    model = item.artwork,
                    contentDescription = null,
                    modifier = Modifier.size(44.dp).clip(RoundedCornerShape(11.dp)),
                )
            } else {
                Box(
                    Modifier
                        .size(44.dp)
                        .clip(RoundedCornerShape(11.dp))
                        .background(c.track),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(item.name.firstOrNull()?.toString() ?: "?", color = c.muted, fontWeight = FontWeight.Bold)
                }
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    item.name,
                    color = c.text,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 14.sp,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                val meta = listOfNotNull(
                    item.artist?.takeIf { it.isNotBlank() },
                    item.date?.takeIf { it.isNotBlank() }?.take(10),
                ).joinToString(" · ").ifBlank { item.adamId }
                Text(meta, color = c.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.height(10.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PrimaryButton("Установить", onClick = onInstall, modifier = Modifier.weight(1f))
            SecondaryButton("Только файл", onClick = onExport, modifier = Modifier.weight(1f))
        }
    }
}

private fun sessionLabel(state: SessionState): String = when (state) {
    SessionState.active, SessionState.saved -> "Готов к восстановлению"
    SessionState.expired -> "Вход истёк"
    SessionState.needs_2fa -> "Нужен код"
    SessionState.damaged -> "Вход повреждён"
    SessionState.not_saved -> "Нужно подключить"
}

private fun normalize(value: String): String =
    Normalizer.normalize(value, Normalizer.Form.NFKD)
        .replace(Regex("\\p{M}+"), "")
        .lowercase()

/** Поиск как на ПК: все слова запроса, без учёта регистра и диакритики. */
private fun historyMatches(item: PurchaseHistoryApp, query: String): Boolean {
    val q = normalize(query).trim()
    if (q.isEmpty()) return true
    val hay = normalize(listOfNotNull(item.name, item.artist, item.bundleId, item.adamId).joinToString(" "))
    return q.split(Regex("\\s+")).filter { it.isNotEmpty() }.all { hay.contains(it) }
}
