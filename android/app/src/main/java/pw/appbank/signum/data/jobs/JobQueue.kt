package pw.appbank.signum.data.jobs

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import pw.appbank.signum.data.api.Errors
import pw.appbank.signum.data.models.ActionHistoryEntry
import pw.appbank.signum.data.models.InstallJob
import pw.appbank.signum.data.models.JobKind
import pw.appbank.signum.data.models.JobStatus
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

class JobCancelledException : Exception("Отменено")

sealed class QueuedWork {
    data class CatalogInstall(
        val jobId: String,
        val title: String,
        val adamId: String?,
        val bundleId: String?,
        val inStore: Boolean,
        val alreadyOwned: Boolean,
        val version: String?,
        val externalVersionId: String?,
        val install: Boolean,
    ) : QueuedWork()

    data class LocalIpa(
        val jobId: String,
        val path: String,
        val sign: Boolean,
        val install: Boolean,
    ) : QueuedWork()
}

class JobQueue(
    private val worker: suspend (QueuedWork, (Float, String, String) -> Unit) -> Unit,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val channel = Channel<QueuedWork>(Channel.UNLIMITED)
    private val jobs = ConcurrentHashMap<String, InstallJob>()
    private val cancelledIds = ConcurrentHashMap.newKeySet<String>()
    private val history = mutableListOf<ActionHistoryEntry>()
    private val _jobsFlow = MutableStateFlow<List<InstallJob>>(emptyList())
    val jobsFlow: StateFlow<List<InstallJob>> = _jobsFlow
    private val _historyFlow = MutableStateFlow<List<ActionHistoryEntry>>(emptyList())
    val historyFlow: StateFlow<List<ActionHistoryEntry>> = _historyFlow

    init {
        scope.launch {
            for (work in channel) {
                val id = when (work) {
                    is QueuedWork.CatalogInstall -> work.jobId
                    is QueuedWork.LocalIpa -> work.jobId
                }
                if (cancelledIds.contains(id) || jobs[id]?.status == JobStatus.cancelled) {
                    finishCancelled(id)
                    continue
                }
                update(id) { it.copy(status = JobStatus.running, phase = "run", message = "Запуск…") }
                try {
                    worker(work) { pct, phase, msg ->
                        throwIfCancelled(id)
                        update(id) {
                            if (it.status == JobStatus.cancelled) it
                            else it.copy(percent = pct, phase = phase, message = msg, status = JobStatus.running)
                        }
                    }
                    if (cancelledIds.contains(id)) {
                        finishCancelled(id)
                    } else {
                        update(id) { it.copy(status = JobStatus.done, percent = 100f, message = if (it.phase == "done") it.message else "Готово", phase = "done") }
                        pushHistory(id, "done")
                    }
                } catch (e: Exception) {
                    if (e is JobCancelledException || cancelledIds.contains(id) || e.message == "Отменено") {
                        finishCancelled(id)
                    } else {
                        val fail = Errors.localize(e.message ?: "Ошибка")
                        update(id) {
                            it.copy(
                                status = JobStatus.failed,
                                error = fail,
                                message = fail,
                                phase = "failed",
                            )
                        }
                        pushHistory(id, "failed")
                    }
                }
            }
        }
    }

    fun enqueueCatalog(
        title: String,
        adamId: String?,
        bundleId: String?,
        inStore: Boolean,
        alreadyOwned: Boolean,
        version: String?,
        externalVersionId: String?,
        install: Boolean,
    ): String {
        val id = UUID.randomUUID().toString()
        val job = InstallJob(
            id = id,
            title = title,
            subtitle = if (install) "Установка" else "Скачивание файла",
            phase = "queue",
            percent = 0f,
            message = "В очереди",
            status = JobStatus.queued,
            createdAt = System.currentTimeMillis(),
            kind = if (install) JobKind.install else JobKind.export,
        )
        jobs[id] = job
        publish()
        channel.trySend(
            QueuedWork.CatalogInstall(
                id, title, adamId, bundleId, inStore, alreadyOwned, version, externalVersionId, install,
            ),
        )
        return id
    }

    fun enqueueLocal(path: String, title: String, sign: Boolean, install: Boolean): String {
        val id = UUID.randomUUID().toString()
        jobs[id] = InstallJob(
            id = id,
            title = title,
            subtitle = if (sign) "Подготовка и установка" else "Свой файл",
            phase = "queue",
            percent = 0f,
            message = "В очереди",
            status = JobStatus.queued,
            createdAt = System.currentTimeMillis(),
            kind = if (sign) JobKind.sign else JobKind.install,
        )
        publish()
        channel.trySend(QueuedWork.LocalIpa(id, path, sign, install))
        return id
    }

    fun cancel(id: String) {
        val cur = jobs[id] ?: return
        if (cur.status != JobStatus.queued && cur.status != JobStatus.running) return
        cancelledIds.add(id)
        update(id) {
            if (it.status == JobStatus.queued) {
                it.copy(status = JobStatus.cancelled, message = "Остановлено", phase = "cancelled")
            } else {
                it.copy(message = "Останавливаем…", phase = "cancel")
            }
        }
    }

    fun isCancelled(id: String): Boolean = cancelledIds.contains(id)

    fun throwIfCancelled(id: String) {
        if (cancelledIds.contains(id)) throw JobCancelledException()
    }

    fun snapshot(): List<InstallJob> = jobs.values.sortedByDescending { it.createdAt }

    fun historySnapshot(): List<ActionHistoryEntry> = history.toList()

    fun clearFinished() {
        jobs.entries.removeIf { it.value.status != JobStatus.queued && it.value.status != JobStatus.running }
        publish()
    }

    private fun update(id: String, transform: (InstallJob) -> InstallJob) {
        val cur = jobs[id] ?: return
        jobs[id] = transform(cur)
        publish()
    }

    private fun publish() {
        _jobsFlow.value = snapshot()
    }

    private fun finishCancelled(id: String) {
        update(id) {
            it.copy(status = JobStatus.cancelled, message = "Остановлено", phase = "cancelled", error = null)
        }
        if (history.none { it.id == id && it.status == "cancelled" }) {
            pushHistory(id, "cancelled")
        }
    }

    private fun pushHistory(id: String, status: String) {
        val job = jobs[id] ?: return
        history.add(
            0,
            ActionHistoryEntry(
                id = id,
                title = job.title,
                subtitle = job.subtitle,
                status = status,
                kind = job.kind.name,
                finishedAt = System.currentTimeMillis(),
            ),
        )
        if (history.size > 80) history.removeAt(history.lastIndex)
        _historyFlow.value = history.toList()
    }
}
