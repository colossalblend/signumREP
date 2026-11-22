package pw.appbank.signum.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.widget.Toast
import androidx.compose.foundation.background
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.InstallMobile
import androidx.compose.material.icons.outlined.KeyboardArrowDown
import androidx.compose.material.icons.outlined.KeyboardArrowUp
import androidx.compose.material.icons.outlined.PlayArrow
import androidx.compose.material.icons.outlined.Schedule
import androidx.compose.material.icons.outlined.SystemUpdate
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import pw.appbank.signum.data.models.InstallJob
import pw.appbank.signum.data.models.JobKind
import pw.appbank.signum.data.models.JobStatus
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.EmptyState
import pw.appbank.signum.ui.components.FilterPills
import pw.appbank.signum.ui.components.Notice
import pw.appbank.signum.ui.components.NoticeTone
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.ProgressBar
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.components.SectionHeader
import pw.appbank.signum.ui.components.StatusChip
import pw.appbank.signum.ui.components.TextAction
import pw.appbank.signum.ui.theme.SignumColors
import pw.appbank.signum.ui.theme.signumColors
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun InstallsScreen(vm: SignumViewModel) {
    val queue = vm.engine.jobs
    val jobs by queue.jobsFlow.collectAsState()
    val history by queue.historyFlow.collectAsState()
    val paused by queue.paused.collectAsState()
    val s by vm.state.collectAsState()
    val updates = vm.availableUpdates().distinctBy { it.first.id }
    val c = signumColors
    val ctx = LocalContext.current
    var filter by rememberSaveable { mutableStateOf("all") }

    val active = jobs.count { it.status == JobStatus.queued || it.status == JobStatus.running }
    val failed = jobs.count { it.status == JobStatus.failed }
    val finished = jobs.count { it.status == JobStatus.done || it.status == JobStatus.cancelled }
    val pendingCount = jobs.count { it.status == JobStatus.queued }
    val visibleJobs = jobs.filter { job ->
        when (filter) {
            "active" -> job.status == JobStatus.queued || job.status == JobStatus.running
            "failed" -> job.status == JobStatus.failed
            "done" -> job.status == JobStatus.done || job.status == JobStatus.cancelled
            else -> true
        }
    }
    val queuedIds = jobs.filter { it.status == JobStatus.queued }.map { it.id }

    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 18.dp, end = 18.dp, top = 12.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            PageHeader(
                "Установки",
                when {
                    active > 0 -> "В работе и в очереди: $active"
                    updates.isNotEmpty() -> "Можно обновить приложений: ${updates.size}"
                    else -> "Очередь загрузок и установок"
                },
            )
        }

        if (paused) {
            item {
                Notice(
                    title = "Очередь на паузе",
                    text = "Текущая операция завершится, остальные будут ждать нажатия «Продолжить очередь».",
                    tone = NoticeTone.Warning,
                ) {
                    SecondaryButton(
                        "Продолжить очередь",
                        onClick = { queue.setPaused(false) },
                        icon = Icons.Outlined.PlayArrow,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }

        // Обновления живут здесь же, а не отдельным пунктом меню: поставленное
        // в очередь тут же появляется ниже, без перехода между экранами.
        if (updates.isNotEmpty()) {
            item {
                SectionHeader("Доступны обновления · ${updates.size}") {
                    TextAction(
                        if (s.bulkBusy) "Ставим…" else "Обновить всё",
                        onClick = { vm.queueAllUpdates {} },
                        enabled = !s.bulkBusy,
                    )
                }
            }
            items(updates, key = { "upd-${it.first.id}" }) { (app, have) ->
                Panel {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        KindBadge(Icons.Outlined.SystemUpdate, c.brandSoft)
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(
                                app.title,
                                color = c.text,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 14.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Spacer(Modifier.height(2.dp))
                            Text("${have.version} → ${app.version}", color = c.muted, fontSize = 12.sp)
                        }
                        TextAction("Обновить", onClick = { vm.queueUpdate(app) {} }, enabled = !s.bulkBusy)
                    }
                }
            }
        }

        if (jobs.isEmpty()) {
            item {
                EmptyState(
                    icon = Icons.Outlined.InstallMobile,
                    title = "Очередь пуста",
                    text = "Установки из каталога, истории покупок и своего файла появятся здесь. " +
                        "Можно свернуть Signum — работа продолжится в фоне.",
                )
            }
        } else {
            item {
                FilterPills(
                    options = listOf(
                        Triple("all", "Все", null),
                        Triple("active", "Активные", active),
                        Triple("failed", "Ошибки", failed),
                        Triple("done", "Завершённые", finished),
                    ),
                    selected = filter,
                    onSelect = { filter = it },
                )
            }
            // Действия над всей очередью — те же, что на ПК.
            item {
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (active > 0 || paused) {
                        TextAction(
                            if (paused) "Продолжить" else "Пауза после текущей",
                            onClick = { queue.setPaused(!paused) },
                        )
                    }
                    if (pendingCount > 0) {
                        TextAction("Отменить ожидающие", onClick = queue::cancelPending, color = c.danger)
                    }
                    if (failed > 0) {
                        TextAction("Повторить ошибки", onClick = { queue.retryFailed() })
                    }
                    Spacer(Modifier.weight(1f))
                    if (failed + finished > 0) {
                        TextAction("Очистить", onClick = queue::clearFinished, color = c.muted)
                    }
                }
            }
            if (visibleJobs.isEmpty()) {
                item {
                    Text(
                        "Для этого фильтра задач нет.",
                        color = c.muted,
                        fontSize = 13.sp,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }

        items(visibleJobs, key = { it.id }) { job ->
            val position = queuedIds.indexOf(job.id)
            JobCard(
                job = job,
                c = c,
                position = position,
                lastPosition = queuedIds.lastIndex,
                onCancel = { vm.cancelJob(job.id) },
                onRetry = { queue.retry(job.id) },
                onMove = { dir -> queue.move(job.id, dir) },
                onCopy = { copyText(ctx, job.error ?: job.message) },
            )
        }

        if (history.isNotEmpty()) {
            item {
                SectionHeader("История действий") {
                    TextAction("Очистить", onClick = queue::clearHistory, color = c.muted)
                }
            }
            item {
                Panel {
                    history.take(40).forEachIndexed { index, h ->
                        if (index > 0) Spacer(Modifier.height(10.dp))
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            val (icon, tint) = when (h.status) {
                                "done" -> Icons.Outlined.CheckCircle to c.success
                                "failed" -> Icons.Outlined.ErrorOutline to c.danger
                                else -> Icons.Outlined.Close to c.muted
                            }
                            Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(10.dp))
                            Column(Modifier.weight(1f)) {
                                Text(
                                    h.title,
                                    color = c.text,
                                    fontSize = 13.sp,
                                    fontWeight = FontWeight.Medium,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Text(
                                    "${h.subtitle} · ${historyStatus(h.status, h.kind)}",
                                    color = c.muted,
                                    fontSize = 11.sp,
                                )
                            }
                            Text(timeLabel(h.finishedAt), color = c.muted, fontSize = 11.sp)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun JobCard(
    job: InstallJob,
    c: SignumColors,
    position: Int,
    lastPosition: Int,
    onCancel: () -> Unit,
    onRetry: () -> Unit,
    onMove: (Int) -> Unit,
    onCopy: () -> Unit,
) {
    val running = job.status == JobStatus.running
    val queued = job.status == JobStatus.queued
    val canStop = queued || (running && job.phase != "cancel")
    val (statusColor, kindIcon) = when (job.status) {
        JobStatus.done -> c.success to Icons.Outlined.CheckCircle
        JobStatus.failed -> c.danger to Icons.Outlined.ErrorOutline
        JobStatus.cancelled -> c.muted to Icons.Outlined.Close
        JobStatus.queued -> c.muted to Icons.Outlined.Schedule
        JobStatus.running -> c.brandSoft to (if (job.kind == JobKind.export) Icons.Outlined.Download else Icons.Outlined.InstallMobile)
    }
    Panel {
        Row(verticalAlignment = Alignment.CenterVertically) {
            KindBadge(kindIcon, statusColor)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    job.title,
                    color = c.text,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 14.sp,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(3.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    StatusChip(statusLabel(job.status, job.kind), statusColor)
                    Spacer(Modifier.width(6.dp))
                    Text(job.subtitle, color = c.muted, fontSize = 11.sp, maxLines = 1)
                }
            }
            if (queued) {
                IconButton(onClick = { onMove(-1) }, enabled = position > 0) {
                    Icon(Icons.Outlined.KeyboardArrowUp, contentDescription = "Выше", tint = if (position > 0) c.text else c.muted.copy(0.4f))
                }
                IconButton(onClick = { onMove(1) }, enabled = position in 0 until lastPosition) {
                    Icon(
                        Icons.Outlined.KeyboardArrowDown,
                        contentDescription = "Ниже",
                        tint = if (position in 0 until lastPosition) c.text else c.muted.copy(0.4f),
                    )
                }
            }
            if (canStop) {
                IconButton(onClick = onCancel) {
                    Icon(Icons.Outlined.Close, contentDescription = "Отменить", tint = c.muted)
                }
            }
        }
        if (running) {
            Spacer(Modifier.height(12.dp))
            Row(verticalAlignment = Alignment.Bottom) {
                Text(
                    job.message,
                    color = c.muted,
                    fontSize = 12.sp,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Text("${job.percent.toInt()}%", color = c.text, fontWeight = FontWeight.Bold, fontSize = 16.sp)
            }
            Spacer(Modifier.height(8.dp))
            ProgressBar(job.percent / 100f)
        } else if (queued) {
            Spacer(Modifier.height(8.dp))
            Text(
                if (position == 0) "Следующее · ${job.message}" else job.message,
                color = c.muted,
                fontSize = 12.sp,
            )
        } else if (job.status == JobStatus.done && job.message.isNotBlank() && job.message != "Готово") {
            Spacer(Modifier.height(8.dp))
            Text(job.message, color = c.muted, fontSize = 12.sp, maxLines = 3, overflow = TextOverflow.Ellipsis)
        }
        job.error?.let { err ->
            Spacer(Modifier.height(10.dp))
            Box(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(c.danger.copy(alpha = 0.08f))
                    .padding(12.dp),
            ) {
                Column {
                    Text(err, color = c.danger, fontSize = 12.sp, lineHeight = 17.sp, maxLines = 6, overflow = TextOverflow.Ellipsis)
                    Spacer(Modifier.height(4.dp))
                    Text(errorHint(err), color = c.muted, fontSize = 11.sp, lineHeight = 15.sp)
                }
            }
        }
        if (job.status == JobStatus.failed || job.status == JobStatus.cancelled) {
            Spacer(Modifier.height(6.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                if (job.retryable) {
                    TextAction("Повторить", onClick = onRetry)
                }
                if (job.error != null) {
                    TextAction("Копировать", onClick = onCopy, color = c.muted)
                }
            }
        }
    }
}

@Composable
private fun KindBadge(icon: ImageVector, tint: Color) {
    Box(
        Modifier
            .size(40.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(tint.copy(alpha = 0.13f)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(20.dp))
    }
}

private fun copyText(ctx: Context, text: String) {
    val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
    runCatching { cm.setPrimaryClip(ClipData.newPlainText("Signum", text)) }
    Toast.makeText(ctx, "Скопировано", Toast.LENGTH_SHORT).show()
}

private fun timeLabel(ms: Long): String {
    val now = System.currentTimeMillis()
    val pattern = if (now - ms < 20 * 60 * 60 * 1000L) "HH:mm" else "d MMM, HH:mm"
    return SimpleDateFormat(pattern, Locale.forLanguageTag("ru")).format(Date(ms))
}

private fun historyStatus(status: String, kind: String): String = when (status) {
    "done" -> if (kind == JobKind.export.name) "сохранено" else "готово"
    "failed" -> "ошибка"
    else -> "отменено"
}

private fun errorHint(message: String): String {
    val value = message.lowercase()
    val next = when {
        "iphone" in value || "устройств" in value || "подключ" in value -> "Разблокируйте iPhone, проверьте кабель и нажмите «Доверять»."
        "apple id" in value || "сесс" in value || "войдите" in value || "2fa" in value -> "Откройте «Аккаунт», обновите вход и повторите задачу."
        "мест" in value || "space" in value || "disk" in value -> "Освободите место на телефоне или iPhone и повторите."
        "сеть" in value || "network" in value || "timeout" in value || "интернет" in value -> "Проверьте интернет и повторите задачу."
        "верси" in value || "ios" in value || "совместим" in value -> "Проверьте минимальную версию iOS приложения."
        else -> "Повторите задачу. Если ошибка останется, скопируйте её текст для поддержки."
    }
    return "Что сделать: $next"
}

private fun statusLabel(status: JobStatus, kind: JobKind): String {
    val export = kind == JobKind.export
    return when (status) {
        JobStatus.queued -> "В очереди"
        JobStatus.running -> when (kind) {
            JobKind.export -> "Загрузка"
            JobKind.sign -> "Подпись"
            JobKind.install -> "Установка"
        }
        JobStatus.done -> if (export) "Сохранено" else "Установлено"
        JobStatus.failed -> "Ошибка"
        JobStatus.cancelled -> "Отменено"
    }
}
