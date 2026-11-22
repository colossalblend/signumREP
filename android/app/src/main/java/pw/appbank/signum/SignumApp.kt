package pw.appbank.signum

import android.app.Application
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import pw.appbank.signum.data.api.DiagLog
import pw.appbank.signum.data.apple.SapSigner
import pw.appbank.signum.data.engine.SignumEngine
import pw.appbank.signum.data.jobs.QueueService
import pw.appbank.signum.data.models.JobStatus

class SignumApp : Application() {
    lateinit var engine: SignumEngine
        private set

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    override fun onCreate() {
        super.onCreate()
        // Раньше всего остального: если запись была включена до перезапуска,
        // она должна застать и запуск движка.
        DiagLog.init(this)
        engine = SignumEngine(this)
        watchQueue()
        sendDiagWhenBackgrounded()
        runSapSelfTest()
    }

    // Как только в очереди появляется незаконченное дело, поднимаем фоновую службу.
    // Слушаем очередь здесь, а не в экранах: тогда работа продолжается и после того,
    // как пользователь свернул или закрыл приложение. Служба снимает себя сама.
    /**
     * Приложение уходит с экрана — отправляем накопленный журнал.
     *
     * onTerminate на живых телефонах Android не зовёт, поэтому цепляемся за
     * жизненный цикл: как только последняя активность ушла в фон, сессия для
     * нас закончилась.
     */
    private fun sendDiagWhenBackgrounded() {
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            private var visible = 0
            override fun onActivityStarted(activity: android.app.Activity) {
                visible++
            }

            override fun onActivityStopped(activity: android.app.Activity) {
                visible--
                if (visible > 0) return
                // В отдельном потоке: отправка ходит в сеть, а зовут нас с
                // главного, и Android за это убивает приложение.
                Thread({ DiagLog.sendOnExit() }, "signum-diag-send").apply {
                    isDaemon = true
                    start()
                }
            }

            override fun onActivityCreated(activity: android.app.Activity, bundle: android.os.Bundle?) {}
            override fun onActivityResumed(activity: android.app.Activity) {}
            override fun onActivityPaused(activity: android.app.Activity) {}
            override fun onActivitySaveInstanceState(activity: android.app.Activity, bundle: android.os.Bundle) {}
            override fun onActivityDestroyed(activity: android.app.Activity) {}
        })
    }

    private fun watchQueue() {
        var running = false
        scope.launch {
            engine.jobs.jobsFlow.collect { list ->
                val busy = list.any { it.status == JobStatus.queued || it.status == JobStatus.running }
                if (busy && !running) QueueService.start(this@SignumApp)
                running = busy
            }
        }
    }

    // Diagnostic: exercise the SAP helper at launch and log the exact result,
    // so signing failures surface without spending an Apple login attempt.
    private fun runSapSelfTest() {
        Thread({
            try {
                val sap = SapSigner(engine.settings)
                Log.i("SignumStore", "SAP selftest: available=${sap.available()}")
                val t0 = System.currentTimeMillis()
                val sig = sap.sign("selftest".toByteArray())
                Log.i("SignumStore", "SAP selftest OK: ${sig.size} bytes in ${System.currentTimeMillis() - t0}ms")
            } catch (e: Throwable) {
                Log.e("SignumStore", "SAP selftest FAILED: ${e.message}", e)
            }
        }, "sap-selftest").apply { isDaemon = true; start() }
    }
}
