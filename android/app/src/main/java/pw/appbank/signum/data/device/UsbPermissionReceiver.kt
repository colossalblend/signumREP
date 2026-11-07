package pw.appbank.signum.data.device

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbManager
import android.os.Build
import pw.appbank.signum.SignumApp

class UsbPermissionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return
        if (action != USB_PERMISSION &&
            action != UsbManager.ACTION_USB_DEVICE_ATTACHED &&
            action != UsbManager.ACTION_USB_DEVICE_DETACHED
        ) {
            return
        }
        val device = deviceFrom(intent)
        val granted = when (action) {
            USB_PERMISSION -> intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)
            else -> device != null && AppleUsb.hasPermission(context, device)
        }
        android.util.Log.i(
            "SignumUsb",
            "event=$action granted=$granted name=${device?.productName} id=${device?.deviceId}",
        )
        (context.applicationContext as? SignumApp)?.engine?.devices?.onUsbIntent(action, device, granted)
    }

    companion object {
        fun deviceFrom(intent: Intent): UsbDevice? {
            return if (Build.VERSION.SDK_INT >= 33) {
                intent.getParcelableExtra(UsbManager.EXTRA_DEVICE, UsbDevice::class.java)
            } else {
                @Suppress("DEPRECATION")
                intent.getParcelableExtra(UsbManager.EXTRA_DEVICE)
            }
        }
    }
}
