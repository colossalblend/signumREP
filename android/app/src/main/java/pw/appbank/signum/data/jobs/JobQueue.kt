package pw.appbank.signum.data.jobs

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
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
    // Сигнал «в очереди что-то появилось». Порядок дел держим сами в pending:
    // из Channel нельзя ни переставить дело выше, ни повторить упавшее.
    private val wake = Channel<Unit>(Channel.CONFLATED)
    private val lock = Any()
    private val pending = ArrayList<String>()
    private val works = ConcurrentHashMap<String, QueuedWork>()
    private val jobs = ConcurrentHashMap<String, InstallJob>()
    private val cancelledIds = ConcurrentHashMap.newKeySet<String>()
    // Пишут в историю из рабочего потока, читают с главного — под lock.
    private val history = mutableListOf<ActionHistoryEntry>()
    private val _jobsFlow = MutableStateFlow<List<InstallJob>>(emptyList())
    val jobsFlow: StateFlow<List<InstallJob>> = _jobsFlow
    private val _historyFlow = MutableStateFlow<List<ActionHistoryEntry>>(emptyList())
    val historyFlow: StateFlow<List<ActionHistoryEntry>> = _historyFlow
    private val _paused = MutableStateFlow(false)

    /** Пауза после текущей: идущее дело доделывается, следующие ждут. */
    val paused: StateFlow<Boolean> = _paused

    init {
        scope.launch {
            while (true) {
                _paused.first { !it }
                val id = synchronized(lock) { if (pending.isEmpty()) null else pending.removeAt(0) }
                if (id == null) {
                    wake.receive()
                    continue
                }
                val work = works[id] ?: continue
                if (cancelledIds.contains(id) || jobs[id]?.status == JobStatus.cancelled) {
                    finishCancelled(id)
                    continue
                }
                run(id, work)
            }
        }
    }

    private suspend fun run(id: String, work: QueuedWork) {
        update(id) { it.copy(status = JobStatus.running, phase = "run", message = "Запуск…", error = null) }
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
                update(id) {
                    it.copy(
                        status = JobStatus.done,
                        percent = 100f,
                        message = if (it.phase == "done") it.message else "Готово",
                        phase = "done",
                    )
                }
                pushHistory(id, "done")
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Throwable) {
            // Throwable, а не Exception: OutOfMemoryError на большом IPA раньше
            // убивал рабочую корутину, и очередь молча вставала навсегда.
            if (e is JobCancelledException || cancelledIds.contains(id) || e.message == "Отменено") {
                finishCancelled(id)
            } else {
                val fail = Errors.localize(e.message ?: e.javaClass.simpleName.ifBlank { "Ошибка" })
                update(id) {
                    it.copy(
                        status = JobStatus.failed,
                        error = fail,
                        message = fail,
                        phase = "failed",
                        retryable = true,
                    )
                }
                pushHistory(id, "failed")
            }
        }
    }

    private fun submit(job: InstallJob, work: QueuedWork) {
        jobs[job.id] = job
        works[job.id] = work
        synchronized(lock) { pending.add(job.id) }
        publish()
        wake.trySend(Unit)
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
            message = if (_paused.value) PAUSED_LABEL else QUEUED_LABEL,
            status = JobStatus.queued,
            createdAt = System.currentTimeMillis(),
            kind = if (install) JobKind.install else JobKind.export,
        )
        submit(
            job,
            QueuedWork.CatalogInstall(
                id, title, adamId, bundleId, inStore, alreadyOwned, version, externalVersionId, install,
            ),
        )
        return id
    }

    fun enqueueLocal(path: String, title: String, sign: Boolean, install: Boolean): String {
        val id = UUID.randomUUID().toString()
        val job = InstallJob(
            id = id,
            title = title,
            subtitle = if (sign) "Подготовка и установка" else "Свой файл",
            phase = "queue",
            percent = 0f,
            message = if (_paused.value) PAUSED_LABEL else QUEUED_LABEL,
            status = JobStatus.queued,
            createdAt = System.currentTimeMillis(),
            kind = if (sign) JobKind.sign else JobKind.install,
        )
        submit(job, QueuedWork.LocalIpa(id, path, sign, install))
        return id
    }

    fun cancel(id: String) {
        val cur = jobs[id] ?: return
        if (cur.status != JobStatus.queued && cur.status != JobStatus.running) return
        cancelledIds.add(id)
        if (cur.status == JobStatus.queued) {
            synchronized(lock) { pending.remove(id) }
            finishCancelled(id)
        } else {
            update(id) { it.copy(message = "Останавливаем…", phase = "cancel") }
        }
    }

    /** «Отменить ожидающие»: всё, что ещё не началось. Идущее дело не трогаем. */
    fun cancelPending() {
        val ids = synchronized(lock) { pending.toList().also { pending.clear() } }
        ids.forEach {
            cancelledIds.add(it)
            finishCancelled(it)
        }
    }

    /**
     * Повторить упавшее или отменённое дело с тем же заданием.
     *
     * Номер дела сохраняем: файл загрузки привязан к нему, и докачка
     * продолжится с того места, где оборвалась.
     */
    fun retry(id: String): Boolean {
        val cur = jobs[id] ?: return false
        if (cur.status != JobStatus.failed && cur.status != JobStatus.cancelled) return false
        if (works[id] == null) return false
        cancelledIds.remove(id)
        update(id) {
            it.copy(
                status = JobStatus.queued,
                phase = "queue",
                percent = 0f,
                message = if (_paused.value) PAUSED_LABEL else QUEUED_LABEL,
                error = null,
                retryable = false,
            )
        }
        synchronized(lock) { if (id !in pending) pending.add(id) }
        publish()
        wake.trySend(Unit)
        return true
    }

    /** «Повторить все ошибки». Возвращает, сколько дел снова в очереди. */
    fun retryFailed(): Int =
        jobs.values.filter { it.status == JobStatus.failed }
            .sortedBy { it.createdAt }
            .count { retry(it.id) }

    /** Переставить ожидающее дело на позицию выше (-1) или ниже (+1). */
    fun move(id: String, direction: Int) {
        val moved = synchronized(lock) {
            val from = pending.indexOf(id)
            val to = from + direction.coerceIn(-1, 1)
            if (from < 0 || to < 0 || to >= pending.size || from == to) {
                false
            } else {
                pending.removeAt(from)
                pending.add(to, id)
                true
            }
        }
        if (moved) publish()
    }

    fun setPaused(value: Boolean) {
        if (_paused.value == value) return
        _paused.value = value
        // Подпись у ожидающих меняем вместе с флагом: так список заметно
        // меняется, и подписчики (служба, приложение) узнают о паузе из
        // того же jobsFlow, без отдельной подписки.
        val label = if (value) PAUSED_LABEL else QUEUED_LABEL
        jobs.keys.toList().forEach { id ->
            jobs.computeIfPresent(id) { _, cur ->
                if (cur.status == JobStatus.queued) cur.copy(message = label) else cur
            }
        }
        publish()
    }

    /** Есть ли работа, ради которой процесс должен жить в фоне. */
    fun hasActiveWork(list: List<InstallJob> = _jobsFlow.value): Boolean =
        list.any { it.status == JobStatus.running } ||
            (!_paused.value && list.any { it.status == JobStatus.queued })

    fun isCancelled(id: String): Boolean = cancelledIds.contains(id)

    fun throwIfCancelled(id: String) {
        if (cancelledIds.contains(id)) throw JobCancelledException()
    }

    /**
     * Снимок для экрана: сначала идущее, затем ожидающие в порядке очереди,
     * затем завершённые — свежие выше.
     */
    fun snapshot(): List<InstallJob> {
        val order = synchronized(lock) { pending.withIndex().associate { (i, id) -> id to i } }
        return jobs.values.sortedWith(
            compareBy<InstallJob>(
                {
                    when (it.status) {
                        JobStatus.running -> 0
                        JobStatus.queued -> 1
                        else -> 2
                    }
                },
                { order[it.id] ?: Int.MAX_VALUE },
                { -it.createdAt },
            ),
        )
    }

    /** Позиция дела среди ожидающих (0 — следующее) или -1. */
    fun pendingIndex(id: String): Int = synchronized(lock) { pending.indexOf(id) }

    fun pendingCount(): Int = synchronized(lock) { pending.size }

    fun historySnapshot(): List<ActionHistoryEntry> = synchronized(lock) { history.toList() }

    fun clearFinished() {
        val removed = jobs.entries
            .filter { it.value.status != JobStatus.queued && it.value.status != JobStatus.running }
            .map { it.key }
        removed.forEach {
            jobs.remove(it)
            works.remove(it)
            cancelledIds.remove(it)
        }
        publish()
    }

    fun clearHistory() {
        synchronized(lock) { history.clear() }
        _historyFlow.value = emptyList()
    }

    private fun update(id: String, transform: (InstallJob) -> InstallJob) {
        // computeIfPresent атомарен: раньше «прочитать-изменить-записать» из
        // рабочего потока и с главного (кнопка «Стоп») затирали друг друга.
        jobs.computeIfPresent(id) { _, cur -> transform(cur) } ?: return
        publish()
    }

    private fun publish() {
        _jobsFlow.value = snapshot()
    }

    private fun finishCancelled(id: String) {
        update(id) {
            it.copy(
                status = JobStatus.cancelled,
                message = "Остановлено",
                phase = "cancelled",
                error = null,
                retryable = true,
            )
        }
        pushHistory(id, "cancelled")
    }

    private fun pushHistory(id: String, status: String) {
        val job = jobs[id] ?: return
        val next = synchronized(lock) {
            // Одно и то же завершение не пишем дважды подряд.
            val top = history.firstOrNull()
            if (top != null && top.id == id && top.status == status) return
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
            history.toList()
        }
        _historyFlow.value = next
    }
}

private const val QUEUED_LABEL = "В очереди"
private const val PAUSED_LABEL = "Очередь на паузе"
