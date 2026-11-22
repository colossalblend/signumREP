package pw.appbank.signum.data.jobs

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.flow.first
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import pw.appbank.signum.data.models.JobStatus
import java.util.Collections

class JobQueueTest {

    private fun local(queue: JobQueue, name: String) = queue.enqueueLocal("/tmp/$name.ipa", name, false, true)

    @Test
    fun runsInOrderAndRespectsMove() = runBlocking {
        val gate = CompletableDeferred<Unit>()
        val ran = Collections.synchronizedList(mutableListOf<String>())
        val queue = JobQueue { work, _ ->
            val id = (work as QueuedWork.LocalIpa).path
            if (ran.isEmpty()) gate.await()
            ran.add(id)
        }
        local(queue, "a")
        val b = local(queue, "b")
        val c = local(queue, "c")
        // a уже взято в работу и ждёт; переставляем c выше b.
        withTimeout(5_000) { queue.jobsFlow.first { list -> list.any { it.status == JobStatus.running } } }
        queue.move(c, -1)
        assertEquals(0, queue.pendingIndex(c))
        assertEquals(1, queue.pendingIndex(b))
        gate.complete(Unit)
        withTimeout(5_000) { queue.jobsFlow.first { list -> list.all { it.status == JobStatus.done } } }
        assertEquals(listOf("/tmp/a.ipa", "/tmp/c.ipa", "/tmp/b.ipa"), ran.toList())
    }

    @Test
    fun failedJobCanBeRetried() = runBlocking {
        var attempts = 0
        val queue = JobQueue { _, _ ->
            attempts++
            // Префикс 2FA: Errors.localize не отправляет такое в журнал, тест не ходит в сеть.
            if (attempts == 1) error("SIGNUM_2FA_REQUIRED:test")
        }
        val id = local(queue, "x")
        withTimeout(5_000) { queue.jobsFlow.first { list -> list.any { it.id == id && it.status == JobStatus.failed } } }
        assertTrue(queue.jobsFlow.value.first { it.id == id }.retryable)
        assertTrue(queue.retry(id))
        withTimeout(5_000) { queue.jobsFlow.first { list -> list.any { it.id == id && it.status == JobStatus.done } } }
        assertEquals(2, attempts)
    }

    @Test
    fun pauseHoldsPendingAndCancelPendingDropsThem() = runBlocking {
        val queue = JobQueue { _, _ -> }
        queue.setPaused(true)
        val a = local(queue, "a")
        val b = local(queue, "b")
        assertEquals(2, queue.pendingCount())
        assertTrue(!queue.hasActiveWork())
        queue.cancelPending()
        assertEquals(0, queue.pendingCount())
        val statuses = queue.jobsFlow.value.associate { it.id to it.status }
        assertEquals(JobStatus.cancelled, statuses[a])
        assertEquals(JobStatus.cancelled, statuses[b])
        queue.setPaused(false)
    }
}
