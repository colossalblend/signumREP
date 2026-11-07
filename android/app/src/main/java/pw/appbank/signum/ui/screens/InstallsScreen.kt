package pw.appbank.signum.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
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
import pw.appbank.signum.data.models.JobKind
import pw.appbank.signum.data.models.JobStatus
import pw.appbank.signum.ui.SignumViewModel
import pw.appbank.signum.ui.components.PageHeader
import pw.appbank.signum.ui.components.Panel
import pw.appbank.signum.ui.components.SecondaryButton
import pw.appbank.signum.ui.theme.Brand500
import pw.appbank.signum.ui.theme.signumColors

@Composable
fun InstallsScreen(vm: SignumViewModel) {
    val jobs = vm.engine.jobs.jobsFlow.collectAsState().value
    val history = vm.engine.jobs.historyFlow.collectAsState().value
    val s = vm.state.collectAsState().value
    val updates = vm.availableUpdates()
    val c = signumColors
    var filter by remember { mutableStateOf("all") }
    val visibleJobs = jobs.filter { job -> when (filter) {
        "active" -> job.status == JobStatus.queued || job.status == JobStatus.running
        "failed" -> job.status == JobStatus.failed
        "done" -> job.status == JobStatus.done || job.status == JobStatus.cancelled
        else -> true
    } }
    LazyColumn(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            PageHeader(
                "Установки",
                if (updates.isEmpty()) "Текущие и недавние установки"
                else "Можно обновить приложений: ${updates.size}",
            )
        }
        // Обновления живут здесь же, а не отдельным пунктом меню: поставленное
        // в очередь тут же появляется ниже, без перехода между экранами.
        if (updates.isNotEmpty()) {
            item {
                Panel {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("Доступны обновления", color = c.text, fontWeight = FontWeight.Bold)
                        Spacer(Modifier.weight(1f))
                        Text(
                            if (s.bulkBusy) "Ставим…" else "Обновить всё",
                            color = if (s.bulkBusy) c.muted else Brand500,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.clickable(enabled = !s.bulkBusy) { vm.queueAllUpdates {} },
                        )
                    }
                }
            }
            items(updates, key = { "upd-${it.first.id}" }) { (app, have) ->
                Panel {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(app.title, color = c.text, fontWeight = FontWeight.Bold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Spacer(Modifier.height(2.dp))
                            Text(
                                "v${have.version} → v${app.version}",
                                color = c.muted,
                                fontSize = 12.sp,
                            )
                        }
                        Spacer(Modifier.width(10.dp))
                        Text(
                            "Обновить",
                            color = if (s.bulkBusy) c.muted else Brand500,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.clickable(enabled = !s.bulkBusy) { vm.queueUpdate(app) {} },
                        )
                    }
                }
            }
        }
        if (jobs.isEmpty()) {
            item {
                Panel {
                    Text("Очередь пуста", color = c.text, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(4.dp))
                    Text(
                        "Установки из каталога, покупок и своего файла появятся здесь.",
                        color = c.muted,
                        fontSize = 13.sp,
                    )
                }
            }
        }
        if (jobs.isNotEmpty()) {
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    listOf("all" to "Все", "active" to "Активные", "failed" to "Ошибки", "done" to "Завершённые").forEach { (value, title) ->
                        Text(title, color = if (filter == value) Brand500 else c.muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.clickable { filter = value }.padding(vertical = 8.dp))
                    }
                    Spacer(Modifier.weight(1f))
                    if (jobs.any { it.status == JobStatus.done || it.status == JobStatus.failed || it.status == JobStatus.cancelled }) {
                        Text("Очистить завершённые", color = c.muted, fontSize = 11.sp, modifier = Modifier.clickable { vm.engine.jobs.clearFinished() }.padding(vertical = 8.dp))
                    }
                }
            }
        }
        items(visibleJobs, key = { it.id }) { job ->
            val canStop = job.status == JobStatus.queued ||
                (job.status == JobStatus.running && job.phase != "cancel")
            Panel {
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        job.title,
                        color = c.text,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.weight(1f),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    if (canStop) {
                        Spacer(Modifier.width(8.dp))
                        SecondaryButton("Стоп", onClick = { vm.cancelJob(job.id) })
                    }
                }
                Text(job.subtitle, color = c.muted, fontSize = 12.sp)
                if (job.status == JobStatus.running || job.status == JobStatus.queued) {
                    Spacer(Modifier.height(8.dp))
                    Text(
                        "${job.percent.toInt()}%",
                        color = c.text,
                        fontWeight = FontWeight.Bold,
                        fontSize = 22.sp,
                    )
                    Spacer(Modifier.height(6.dp))
                    LinearProgressIndicator(
                        progress = { (job.percent / 100f).coerceIn(0f, 1f) },
                        modifier = Modifier.fillMaxWidth(),
                        color = Brand500,
                    )
                    Spacer(Modifier.height(6.dp))
                }
                Text(
                    statusLabel(job.status, job.kind) + " · " + job.message,
                    color = c.muted,
                    fontSize = 12.sp,
                    maxLines = 4,
                    overflow = TextOverflow.Ellipsis,
                )
                job.error?.let {
                    Text(it, color = androidx.compose.ui.graphics.Color(0xFFF87171), fontSize = 12.sp, maxLines = 6, overflow = TextOverflow.Ellipsis)
                    Text(errorHint(it), color = c.muted, fontSize = 11.sp, lineHeight = 15.sp)
                }
            }
        }
        if (history.isNotEmpty()) {
            item { Text("История", color = c.muted, fontWeight = FontWeight.SemiBold) }
            items(history, key = { "h${it.id}${it.finishedAt}" }) { h ->
                Panel {
                    Text(h.title, color = c.text, fontWeight = FontWeight.SemiBold)
                    Text("${h.subtitle} · ${h.status}", color = c.muted, fontSize = 12.sp)
                }
            }
        }
    }
}

private fun errorHint(message: String): String {
    val value = message.lowercase()
    val next = when {
        "iphone" in value || "устройств" in value || "подключ" in value -> "Разблокируйте iPhone, проверьте кабель и нажмите «Доверять»."
        "apple id" in value || "сесс" in value || "войдите" in value || "2fa" in value -> "Откройте «Аккаунт», обновите вход и повторите задачу."
        "мест" in value || "space" in value || "disk" in value -> "Освободите место на телефоне или компьютере и повторите."
        "сеть" in value || "network" in value || "timeout" in value -> "Проверьте интернет и повторите задачу."
        "верси" in value || "ios" in value || "совместим" in value -> "Проверьте минимальную версию iOS приложения."
        else -> "Повторите задачу. Если ошибка останется, скопируйте её текст для поддержки."
    }
    return "Что сделать: $next"
}

private fun statusLabel(status: JobStatus, kind: JobKind): String {
    val export = kind == JobKind.export
    return when (status) {
        JobStatus.queued -> "В очереди"
        JobStatus.running -> if (export) "Загрузка" else "Выполняется"
        JobStatus.done -> if (export) "Сохранено" else "Установлено"
        JobStatus.failed -> "Ошибка"
        JobStatus.cancelled -> "Отменено"
    }
}
