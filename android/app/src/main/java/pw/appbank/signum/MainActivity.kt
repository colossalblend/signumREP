package pw.appbank.signum

import android.Manifest
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.hardware.usb.UsbManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import pw.appbank.signum.data.device.USB_PERMISSION
import pw.appbank.signum.data.device.UsbPermissionReceiver
import pw.appbank.signum.data.jobs.QueueService
import pw.appbank.signum.ui.navigation.SignumRoot

class MainActivity : ComponentActivity() {
    private val usbReceiver = UsbPermissionReceiver()
    private val askNotifications =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars = false
        val filter = IntentFilter().apply {
            addAction(USB_PERMISSION)
            addAction(UsbManager.ACTION_USB_DEVICE_ATTACHED)
            addAction(UsbManager.ACTION_USB_DEVICE_DETACHED)
        }
        if (Build.VERSION.SDK_INT >= 33) {
            registerReceiver(usbReceiver, filter, RECEIVER_EXPORTED)
        } else {
            registerReceiver(usbReceiver, filter)
        }
        handleUsbIntent(intent)
        val engine = (application as SignumApp).engine
        handleQueueIntent(intent)
        askNotificationsWhenNeeded()
        setContent { SignumRoot(engine) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleUsbIntent(intent)
        handleQueueIntent(intent)
    }

    // Разрешение спрашиваем не на первом запуске, а когда появилось первое дело:
    // в этот момент понятно, зачем оно нужно, и отказ выглядит осознанным.
    private fun askNotificationsWhenNeeded() {
        if (Build.VERSION.SDK_INT < 33) return
        val engine = (application as SignumApp).engine
        lifecycleScope.launch {
            engine.jobs.jobsFlow.first { it.isNotEmpty() }
            val granted = ContextCompat.checkSelfPermission(
                this@MainActivity,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED
            if (!granted) askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    // Пришли по нажатию на уведомление — просим экран открыть очередь.
    private fun handleQueueIntent(intent: Intent?) {
        if (intent == null || !intent.getBooleanExtra(QueueService.EXTRA_OPEN_QUEUE, false)) return
        // Флаг снимаем сразу, иначе поворот экрана снова уведёт на очередь.
        intent.removeExtra(QueueService.EXTRA_OPEN_QUEUE)
        QueueService.openQueueRequested.value = true
    }

    override fun onResume() {
        super.onResume()
        (application as? SignumApp)?.engine?.devices?.hostActivity = this
    }

    override fun onPause() {
        val devices = (application as? SignumApp)?.engine?.devices
        if (devices?.hostActivity === this) devices.hostActivity = null
        super.onPause()
    }

    override fun onDestroy() {
        super.onDestroy()
        runCatching { unregisterReceiver(usbReceiver) }
    }

    private fun handleUsbIntent(intent: Intent?) {
        val action = intent?.action ?: return
        if (action != UsbManager.ACTION_USB_DEVICE_ATTACHED && action != USB_PERMISSION) return
        val device = UsbPermissionReceiver.deviceFrom(intent) ?: return
        val granted = if (action == USB_PERMISSION) {
            intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)
        } else {
            false
        }
        (application as? SignumApp)?.engine?.devices?.onUsbIntent(action, device, granted)
    }
}
