package pw.appbank.signum.data.device

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.hardware.usb.UsbConstants
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbDeviceConnection
import android.hardware.usb.UsbEndpoint
import android.hardware.usb.UsbInterface
import android.hardware.usb.UsbManager
import android.os.Build

const val APPLE_VID = 0x05AC
const val USB_PERMISSION = "com.appbank.signum.USB_PERMISSION"

data class ClaimedAppleUsb(
    val device: UsbDevice,
    val connection: UsbDeviceConnection,
    val intf: UsbInterface,
    val bulkIn: UsbEndpoint,
    val bulkOut: UsbEndpoint,
    val held: List<UsbInterface> = emptyList(),
)

class UsbResetException(reason: String) : Exception(
    if (reason.isBlank()) "USB: соединение сброшено" else "USB: соединение сброшено ($reason)",
)

object AppleUsb {
    fun manager(context: Context): UsbManager =
        context.getSystemService(Context.USB_SERVICE) as UsbManager

    fun allDevices(context: Context): List<UsbDevice> =
        manager(context).deviceList.values.toList()

    fun listAppleDevices(context: Context): List<UsbDevice> =
        allDevices(context).filter { looksLikeIphone(it) }

    fun looksLikeIphone(device: UsbDevice): Boolean {
        if (device.vendorId == APPLE_VID) return true
        val name = ((device.productName ?: "") + " " + (device.manufacturerName ?: "")).lowercase()
        return name.contains("iphone") || name.contains("ipad") || name.contains("ipod") || name.contains("apple")
    }

    fun describe(device: UsbDevice): String {
        val name = device.productName ?: device.deviceName
        val maker = device.manufacturerName ?: ""
        return buildString {
            append(name)
            if (maker.isNotBlank()) append(" ($maker)")
            append(String.format(" vid=%04x pid=%04x", device.vendorId, device.productId))
        }
    }

    fun permissionIntent(context: Context, device: UsbDevice): PendingIntent {
        val intent = Intent(context, UsbPermissionReceiver::class.java).apply {
            action = USB_PERMISSION
            setPackage(context.packageName)
        }
        val flags = if (Build.VERSION.SDK_INT >= 31) {
            PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        } else {
            PendingIntent.FLAG_UPDATE_CURRENT
        }
        return PendingIntent.getBroadcast(context, device.deviceId, intent, flags)
    }

    fun hasPermission(context: Context, device: UsbDevice): Boolean =
        runCatching { manager(context).hasPermission(device) }.getOrDefault(false)

    fun requestPermission(context: Context, device: UsbDevice) {
        manager(context).requestPermission(device, permissionIntent(context, device))
    }

    fun resolve(context: Context, hint: UsbDevice): UsbDevice {
        val list = listAppleDevices(context)
        return list.find { it.deviceId == hint.deviceId }
            ?: list.find { it.vendorId == hint.vendorId && it.productId == hint.productId }
            ?: list.firstOrNull()
            ?: hint
    }

    fun claim(context: Context, device: UsbDevice): ClaimedAppleUsb {
        val usb = manager(context)
        if (!hasPermission(context, device)) {
            error("Нет разрешения USB. Подтвердите доступ к iPhone.")
        }
        dump(device)
        var last: String? = null
        repeat(5) { attempt ->
            val live = resolve(context, device)
            if (!hasPermission(context, live)) {
                error("Нет разрешения USB. Подтвердите доступ к iPhone.")
            }
            val connection = usb.openDevice(live)
            if (connection == null) {
                last = "openDevice вернул null"
                android.util.Log.w("SignumUsb", "try=${attempt + 1} $last")
                Thread.sleep(180L * (attempt + 1))
                return@repeat
            }
            val cfg = pickConfiguration(live)
            if (cfg != null) {
                val ok = connection.setConfiguration(cfg)
                android.util.Log.i(
                    "SignumUsb",
                    "setConfiguration id=${cfg.id} ok=$ok ifaces=${cfg.interfaceCount}",
                )
                Thread.sleep(250)
            }
            val mux = muxFromConfig(cfg) ?: muxInterfaces(live).firstOrNull {
                it.interfaceClass == 255 && it.interfaceSubclass == 254
            }
            if (mux == null) {
                connection.close()
                last = "нет usbmux 255/254/2 в выбранном конфиге"
                Thread.sleep(220L * (attempt + 1))
                return@repeat
            }
            val claimed = connection.claimInterface(mux, true)
            android.util.Log.i(
                "SignumUsb",
                "claim mux iface=${mux.id} class=${mux.interfaceClass}/${mux.interfaceSubclass}/${mux.interfaceProtocol} ok=$claimed",
            )
            if (!claimed) {
                connection.close()
                last = "claimInterface usbmux отказал"
                Thread.sleep(220L * (attempt + 1))
                return@repeat
            }
            runCatching { connection.setInterface(mux) }
            val eps = bulkEndpoints(mux)
            if (eps == null) {
                connection.close()
                last = "usbmux без bulk IN/OUT"
                return@repeat
            }
            val held = holdSiblings(connection, live, mux)
            return ClaimedAppleUsb(live, connection, mux, eps.first, eps.second, held)
        }
        error(
            "Не удалось захватить USB (${last ?: "неизвестно"}). " +
                "Закройте «Файлы» / уведомление iPhone и Nearby, разблокируйте телефон и повторите.",
        )
    }

    fun release(claimed: ClaimedAppleUsb) {
        claimed.held.forEach { runCatching { claimed.connection.releaseInterface(it) } }
        runCatching { claimed.connection.releaseInterface(claimed.intf) }
        runCatching { claimed.connection.close() }
    }

    private fun holdSiblings(
        connection: UsbDeviceConnection,
        device: UsbDevice,
        mux: UsbInterface,
    ): List<UsbInterface> {
        val held = mutableListOf<UsbInterface>()
        val source = ArrayList<UsbInterface>()
        for (c in 0 until device.configurationCount) {
            val cfg = device.getConfiguration(c)
            for (i in 0 until cfg.interfaceCount) source += cfg.getInterface(i)
        }
        if (source.isEmpty()) {
            for (i in 0 until device.interfaceCount) source += device.getInterface(i)
        }
        for (intf in source) {
            if (intf.id == mux.id && intf.interfaceClass == mux.interfaceClass) continue
            val steal = intf.interfaceClass == 6 ||
                (intf.interfaceClass == 255 && intf.interfaceSubclass == 253)
            if (!steal) continue
            val ok = runCatching { connection.claimInterface(intf, true) }.getOrDefault(false)
            android.util.Log.i(
                "SignumUsb",
                "hold sibling iface=${intf.id} ${intf.interfaceClass}/${intf.interfaceSubclass} ok=$ok",
            )
            if (ok) held += intf
        }
        return held
    }

    private fun muxFromConfig(cfg: android.hardware.usb.UsbConfiguration?): UsbInterface? {
        if (cfg == null) return null
        for (i in 0 until cfg.interfaceCount) {
            val intf = cfg.getInterface(i)
            if (intf.interfaceClass == 255 && intf.interfaceSubclass == 254) return intf
        }
        return null
    }

    private fun pickConfiguration(device: UsbDevice): android.hardware.usb.UsbConfiguration? {
        if (device.configurationCount <= 0) return null
        var muxWithNcm: android.hardware.usb.UsbConfiguration? = null
        var fallback = device.getConfiguration(0)
        for (c in 0 until device.configurationCount) {
            val cfg = device.getConfiguration(c)
            var mux = false
            var ncm = false
            for (i in 0 until cfg.interfaceCount) {
                val intf = cfg.getInterface(i)
                if (intf.interfaceClass == 255 && intf.interfaceSubclass == 254) mux = true
                if (intf.interfaceClass == 255 && intf.interfaceSubclass == 253) ncm = true
            }
            if (mux && !ncm) return cfg
            if (mux) muxWithNcm = cfg
            if (mux || cfg.interfaceCount > 0) fallback = cfg
        }
        return muxWithNcm ?: fallback
    }

    private fun muxInterfaces(device: UsbDevice): List<UsbInterface> {
        val mux = mutableListOf<UsbInterface>()
        for (c in 0 until device.configurationCount) {
            val cfg = device.getConfiguration(c)
            for (i in 0 until cfg.interfaceCount) {
                val intf = cfg.getInterface(i)
                if (intf.interfaceClass == 255 && intf.interfaceSubclass == 254) mux += intf
            }
        }
        return mux
    }

    private fun bulkEndpoints(intf: UsbInterface): Pair<UsbEndpoint, UsbEndpoint>? {
        var inn: UsbEndpoint? = null
        var out: UsbEndpoint? = null
        for (e in 0 until intf.endpointCount) {
            val ep = intf.getEndpoint(e)
            if (ep.type != UsbConstants.USB_ENDPOINT_XFER_BULK) continue
            if (ep.direction == UsbConstants.USB_DIR_IN) inn = ep
            if (ep.direction == UsbConstants.USB_DIR_OUT) out = ep
        }
        val a = inn ?: return null
        val b = out ?: return null
        return a to b
    }

    private fun dump(device: UsbDevice) {
        val ifaces = (0 until device.interfaceCount).joinToString { i ->
            val intf = device.getInterface(i)
            "i${intf.id}:${intf.interfaceClass}/${intf.interfaceSubclass}/${intf.interfaceProtocol} ep=${intf.endpointCount}"
        }
        android.util.Log.i(
            "SignumUsb",
            "device=${describe(device)} cfgs=${device.configurationCount} ifaces=[$ifaces]",
        )
    }
}
