package pw.appbank.signum.data.device

import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException

/**
 * Транспорт под usbmux-протокол. USB-вариант гоняет кадры через bulk-endpoints,
 * Wi-Fi-вариант — через TCP-сокет на порт 27015 (там iPhone принимает тот же
 * бинарный протокол, когда включена «синхронизация по Wi-Fi» в iTunes/Finder).
 */
interface MuxLink : AutoCloseable {
    /** Отправить кадр целиком (транспорт сам решает про ZLP и т.п.). */
    fun write(packet: ByteArray)

    /**
     * Блокирующее чтение в буфер. Возвращает число прочитанных байт или -1,
     * если за таймаут не пришло ничего. Обрыв соединения бросает IOException.
     */
    fun read(into: ByteArray, timeoutMs: Int): Int
}

/** Классический транспорт: USB bulk endpoints заявленного iPhone. */
class UsbLink(private val usb: ClaimedAppleUsb) : MuxLink {
    override fun write(packet: ByteArray) {
        val sent = usb.connection.bulkTransfer(usb.bulkOut, packet, packet.size, 4_000)
        if (sent < 0) error("USB: не удалось отправить пакет")
        // Хост-контроллер требует zero-length packet после кадра, кратного 512.
        if (packet.isNotEmpty() && packet.size % 512 == 0) {
            usb.connection.bulkTransfer(usb.bulkOut, ByteArray(0), 0, 200)
        }
    }

    override fun read(into: ByteArray, timeoutMs: Int): Int =
        usb.connection.bulkTransfer(usb.bulkIn, into, into.size, timeoutMs)

    override fun close() {}
}

/** Wi-Fi транспорт: iPhone слушает usbmux-протокол на TCP-порту 27015. */
class TcpLink(
    private val host: String,
    private val port: Int,
    connectTimeoutMs: Int = 4_000,
) : MuxLink {
    private val socket = Socket()

    init {
        socket.tcpNoDelay = true
        socket.connect(InetSocketAddress(host, port), connectTimeoutMs)
    }

    override fun write(packet: ByteArray) {
        val out = socket.getOutputStream()
        out.write(packet)
        out.flush()
    }

    override fun read(into: ByteArray, timeoutMs: Int): Int {
        socket.soTimeout = timeoutMs.coerceAtLeast(1)
        return try {
            socket.getInputStream().read(into)
        } catch (e: SocketTimeoutException) {
            -1
        }
        // Обрыв (EOF/reset) вылетит IOException наружу — так вызывающий цикл
        // не крутится впустую до конца дедлайна.
    }

    override fun close() {
        runCatching { socket.close() }
    }
}
