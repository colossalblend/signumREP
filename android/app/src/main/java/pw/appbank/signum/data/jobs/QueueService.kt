package pw.appbank.signum.data.jobs

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import com.appbank.signum.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import pw.appbank.signum.MainActivity
import pw.appbank.signum.SignumApp
import pw.appbank.signum.data.models.InstallJob
import pw.appbank.signum.data.models.JobKind
import pw.appbank.signum.data.models.JobStatus

/**
 * Служба живёт ровно столько, сколько в очереди есть незаконченные дела.
 * Она нужна, чтобы система не усыпила процесс, когда приложение свернули:
 * загрузка и установка продолжаются, а пользователь видит прогресс в шторке.
 * Как только незаконченных дел не осталось — служба снимает себя сама.
 */
class QueueService : Service() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    // Прошлый снимок очереди: по нему ловим момент, когда дело только что завершилось,
    // чтобы показать короткое уведомление ровно один раз.
    private var previous = mapOf<String, JobStatus>()
    private var stopping = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannels()
        enterForeground()
        previous = queue().snapshot().associate { it.id to it.status }
        scope.launch {
            queue().jobsFlow.collect { onQueueChanged(it) }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Новое дело могло прийти ровно в тот момент, когда служба уже собралась
        // выключиться. Тогда возвращаем её в работу, иначе прогресс пропадёт.
        if (stopping) {
            stopping = false
            enterForeground()
            onQueueChanged(queue().snapshot())
        }
        return START_REDELIVER_INTENT
    }

    private fun queue(): JobQueue = (application as SignumApp).engine.jobs

    // Система даёт всего несколько секунд на первое уведомление, иначе убьёт службу,
    // поэтому показываем его сразу, ещё до первого события очереди.
    private fun enterForeground() {
        val snapshot = queue().snapshot()
        val first = snapshot.firstOrNull { it.status == JobStatus.running }
            ?: snapshot.firstOrNull { it.status == JobStatus.queued }
        ServiceCompat.startForeground(
            this,
            PROGRESS_ID,
            progressNotification(first, 0),
            foregroundType(),
        )
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private fun onQueueChanged(list: List<InstallJob>) {
        if (stopping) return
        list.forEach { job ->
            val was = previous[job.id]
            // was == null — дело появилось и закончилось между двумя обновлениями списка
            // (очередь отдаёт только последнее состояние), про него тоже надо сказать.
            val wasActive = was == null || was == JobStatus.queued || was == JobStatus.running
            if (!wasActive) return@forEach
            // Про остановленное пользователем не пишем: он сам нажал кнопку, он и так знает.
            when (job.status) {
                JobStatus.done -> notifyFinished(job, true)
                JobStatus.failed -> notifyFinished(job, false)
                else -> Unit
            }
        }
        previous = list.associate { it.id to it.status }

        val running = list.firstOrNull { it.status == JobStatus.running }
        val waiting = list.count { it.status == JobStatus.queued }
        val current = running ?: list.firstOrNull { it.status == JobStatus.queued }
        if (current == null) {
            stopping = true
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
            stopSelf()
            return
        }
        val rest = if (running != null) waiting else waiting - 1
        notifications().notify(PROGRESS_ID, progressNotification(current, rest))
    }

    private fun progressNotification(job: InstallJob?, rest: Int): Notification {
        val builder = NotificationCompat.Builder(this, CHANNEL_PROGRESS)
            .setSmallIcon(R.drawable.ic_stat_queue)
            .setContentIntent(openQueueIntent())
            .setOngoing(true)
            .setSilent(true)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
        if (job == null) {
            return builder
                .setContentTitle("Signum работает")
                .setContentText("Заканчиваем дела")
                .setProgress(0, 0, true)
                .build()
        }
        val percent = job.percent.toInt().coerceIn(0, 100)
        val waiting = job.status == JobStatus.queued
        builder.setContentTitle(headline(job))
            .setContentText(if (waiting) job.message else "$percent% · ${job.message}")
            .setProgress(100, percent, waiting)
        if (rest > 0) builder.setSubText(waitingLabel(rest))
        return builder.build()
    }

    private fun notifyFinished(job: InstallJob, success: Boolean) {
        val export = job.kind == JobKind.export
        val title = job.title.trim()
        val text = when {
            success && export -> "$title: файл сохранён"
            success -> "$title: установлено на iPhone"
            export -> "$title: загрузка файла не удалась"
            else -> "$title: установка на iPhone не удалась"
        }
        val detail = if (success) null else job.error?.takeIf { it.isNotBlank() }
        val note = NotificationCompat.Builder(this, CHANNEL_DONE)
            .setSmallIcon(R.drawable.ic_stat_queue)
            .setContentIntent(openQueueIntent())
            .setAutoCancel(true)
            .setContentTitle(if (success) "Готово" else "Не получилось")
            .setContentText(text)
            .apply {
                if (detail != null) {
                    setStyle(NotificationCompat.BigTextStyle().bigText(text + "\n" + detail))
                }
            }
            .build()
        // Номер уведомления считаем из идентификатора дела: повторное сообщение про то же
        // самое заменит старое, а не насыплет дубликатов.
        notifications().notify(DONE_ID_BASE + (job.id.hashCode() and 0xFFFF), note)
    }

    private fun headline(job: InstallJob): String = when (job.kind) {
        JobKind.export -> "Скачиваем " + job.title
        JobKind.sign -> "Готовим " + job.title
        JobKind.install -> "Ставим " + job.title + " на iPhone"
    }

    private fun waitingLabel(rest: Int): String = when {
        rest == 1 -> "Ещё одно приложение ждёт"
        rest in 2..4 -> "Ещё $rest приложения ждут"
        else -> "Ещё $rest приложений ждут"
    }

    private fun openQueueIntent(): PendingIntent {
        val intent = Intent(this, MainActivity::class.java)
            .setAction(Intent.ACTION_MAIN)
            .addCategory(Intent.CATEGORY_LAUNCHER)
            .putExtra(EXTRA_OPEN_QUEUE, true)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        return PendingIntent.getActivity(
            this,
            0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    private fun notifications(): NotificationManager =
        getSystemService(NotificationManager::class.java)

    private fun createChannels() {
        val manager = notifications()
        // Тихий канал: прогресс обновляется десятки раз, звенеть на каждый процент нельзя.
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_PROGRESS,
                "Идёт установка",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "Показывает, чем Signum занят прямо сейчас"
                setShowBadge(false)
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_DONE,
                "Установка завершена",
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply {
                description = "Сообщает, что приложение поставилось или что не получилось"
            },
        )
    }

    // С Android 14 тип службы обязателен. Качаем файл (обмен данными) и ставим его
    // на подключённый по кабелю iPhone (работа с подключённым устройством).
    private fun foregroundType(): Int =
        if (Build.VERSION.SDK_INT >= 29) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC or
                ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE
        } else {
            0
        }

    companion object {
        const val EXTRA_OPEN_QUEUE = "pw.appbank.signum.OPEN_QUEUE"
        private const val CHANNEL_PROGRESS = "signum_progress"
        private const val CHANNEL_DONE = "signum_done"
        private const val PROGRESS_ID = 1001
        private const val DONE_ID_BASE = 2000

        // Пользователь нажал на уведомление: экран увидит это, откроет очередь
        // и сам вернёт флаг обратно.
        val openQueueRequested = MutableStateFlow(false)

        fun start(context: Context) {
            runCatching {
                ContextCompat.startForegroundService(
                    context,
                    Intent(context, QueueService::class.java),
                )
            }
        }
    }
}
