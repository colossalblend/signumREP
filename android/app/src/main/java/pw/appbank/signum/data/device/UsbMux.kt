package pw.appbank.signum.data.device

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.TimeoutException
import java.util.concurrent.atomic.AtomicInteger

   
                                                             
                                                                            
   
class UsbMux(private val usb: ClaimedAppleUsb) {
    private val streams = java.util.concurrent.ConcurrentHashMap<Int, MuxStream>()
    private val io = Any()
    private val pktbuf = ByteArrayOutputStream()

    @Volatile
    private var version = 0
    private var txSeq = 0
    private var rxSeq = 0

    fun handshake(timeoutMs: Int = 8_000) {
        version = 0
        txSeq = 0
        rxSeq = 0
        pktbuf.reset()
        val vh = ByteBuffer.allocate(12).order(ByteOrder.BIG_ENDIAN)
        vh.putInt(2)
        vh.putInt(0)
        vh.putInt(0)
        sendMux(PROTO_VERSION, vh.array())
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            val frame = recvFrame((deadline - System.currentTimeMillis()).toInt().coerceAtLeast(1)) ?: continue
            if (frame.proto != PROTO_VERSION || frame.payload.size < 8) continue
            val major = ByteBuffer.wrap(frame.payload).order(ByteOrder.BIG_ENDIAN).int
            version = major
            android.util.Log.i("SignumMux", "handshake mux-v$major")
            if (version >= 2) {
                txSeq = 0
                rxSeq = 0xFFFF
                sendMux(PROTO_SETUP, byteArrayOf(0x07))
            }
            return
        }
        error("iPhone не ответил на USB (handshake). Разблокируйте телефон и повторите «Доверять».")
    }

    fun connect(dport: Int, timeoutMs: Int = 8_000): MuxStream {
        var local = nextSport.getAndIncrement()
        if (local > 0xF000) {
            nextSport.set(0x8000)
            local = nextSport.getAndIncrement()
        }
        val stream = MuxStream(this, local, dport)
        streams[local] = stream
        stream.syn(timeoutMs)
        return stream
    }

    internal fun drop(sport: Int) {
        streams.remove(sport)
    }

    internal fun sendTcp(sport: Int, dport: Int, seq: Int, ack: Int, flags: Int, payload: ByteArray = ByteArray(0)) {
        val tcp = ByteBuffer.allocate(20 + payload.size).order(ByteOrder.BIG_ENDIAN)
        tcp.putShort(sport.toShort())
        tcp.putShort(dport.toShort())
        tcp.putInt(seq)
        tcp.putInt(ack)
        tcp.putShort(((5 shl 12) or (flags and 0x1FF)).toShort())
        tcp.putShort((TX_WIN shr 8).toShort())
        tcp.putShort(0)
        tcp.putShort(0)
        if (payload.isNotEmpty()) tcp.put(payload)
        sendMux(PROTO_TCP, tcp.array())
    }

    internal fun pump(timeoutMs: Int): Boolean {
        val frame = recvFrame(timeoutMs) ?: return false
        if (frame.proto != PROTO_TCP || frame.payload.size < 20) return true
        val tcp = ByteBuffer.wrap(frame.payload).order(ByteOrder.BIG_ENDIAN)
        val sport = tcp.short.toInt() and 0xFFFF
        val dport = tcp.short.toInt() and 0xFFFF
        val seqn = tcp.int
        val ack = tcp.int
        val offFlags = tcp.short.toInt() and 0xFFFF
        val flags = offFlags and 0x1FF
        val dataOff = (offFlags ushr 12) * 4
        tcp.position(20 + (dataOff - 20).coerceAtLeast(0))
        val payload = ByteArray(tcp.remaining())
        tcp.get(payload)
        val pkt = MuxPacket(sport, dport, seqn, ack, flags, payload)
        val dest = streams[pkt.dport] ?: if (flags and TH_RST != 0) streams[pkt.sport] else null
        if (dest == null) {
            android.util.Log.w(
                "SignumMux",
                "tcp in no dest sport=$sport dport=$dport flags=0x${flags.toString(16)} len=${payload.size}",
            )
        } else {
            dest.offer(pkt)
        }
        return true
    }

    private fun sendMux(proto: Int, payload: ByteArray) {
        synchronized(io) {
            val hdrSize = if (version < 2) 8 else 16
            val total = hdrSize + payload.size
            val buf = ByteBuffer.allocate(total).order(ByteOrder.BIG_ENDIAN)
            buf.putInt(proto)
            buf.putInt(total)
            if (version >= 2) {
                buf.putInt(MAGIC)
                buf.putShort(txSeq.toShort())
                buf.putShort(rxSeq.toShort())
                txSeq = (txSeq + 1) and 0xFFFF
            }
            buf.put(payload)
            val packet = buf.array()
            val sent = usb.connection.bulkTransfer(usb.bulkOut, packet, packet.size, 4_000)
            if (sent < 0) error("USB: не удалось отправить пакет")
            if (packet.isNotEmpty() && packet.size % 512 == 0) {
                usb.connection.bulkTransfer(usb.bulkOut, ByteArray(0), 0, 200)
            }
        }
    }

    private fun recvFrame(timeoutMs: Int): MuxFrame? {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (true) {
            val parsed = tryParse()
            if (parsed != null) return parsed
            val left = (deadline - System.currentTimeMillis()).toInt()
            if (left <= 0) return null
            val chunk = ByteArray(16_384)
            val n = synchronized(io) {
                usb.connection.bulkTransfer(usb.bulkIn, chunk, chunk.size, left)
            }
            if (n <= 0) continue
            pktbuf.write(chunk, 0, n)
        }
    }

    private fun tryParse(): MuxFrame? {
        val data = pktbuf.toByteArray()
        if (data.size < 8) return null
        val hdr = ByteBuffer.wrap(data).order(ByteOrder.BIG_ENDIAN)
        val proto = hdr.int
        val length = hdr.int
        if (length < 8 || length > 1_000_000) {
            android.util.Log.w("SignumMux", "bad frame proto=$proto len=$length have=${data.size}")
            pktbuf.reset()
            return null
        }
        if (data.size < length) return null
        val hdrSize = if (version < 2) 8 else 16
        if (version >= 2 && data.size >= 16) {
            hdr.int
            hdr.short
            rxSeq = hdr.short.toInt() and 0xFFFF
        }
        if (length < hdrSize) {
            pktbuf.reset()
            return null
        }
        val payload = data.copyOfRange(hdrSize, length)
        val rest = if (data.size > length) data.copyOfRange(length, data.size) else ByteArray(0)
        pktbuf.reset()
        if (rest.isNotEmpty()) pktbuf.write(rest)
        return MuxFrame(proto, payload)
    }

    private data class MuxFrame(val proto: Int, val payload: ByteArray)

    class MuxPacket(
        val sport: Int,
        val dport: Int,
        val seq: Int,
        val ack: Int,
        val flags: Int,
        val payload: ByteArray,
    )

    class MuxStream(
        private val mux: UsbMux,
        private val sport: Int,
        private val dport: Int,
    ) {
        private var sndSeq = 0
        private var rcvAck = 0
        private var peerAck = 0
        private val inbox = ArrayDeque<Byte>()
        private val pending = ArrayDeque<MuxPacket>()

        fun offer(pkt: MuxPacket) {
            synchronized(this) { pending.addLast(pkt) }
        }

        fun syn(timeoutMs: Int) {
            mux.sendTcp(sport, dport, sndSeq, 0, TH_SYN)
            val deadline = System.currentTimeMillis() + timeoutMs
            while (System.currentTimeMillis() < deadline) {
                val pkt = nextPacket((deadline - System.currentTimeMillis()).toInt().coerceAtLeast(1)) ?: continue
                if (pkt.flags and TH_RST != 0) {
                    mux.drop(sport)
                    throw UsbResetException(rstReason(pkt))
                }
                if (pkt.flags and TH_SYN != 0 && pkt.flags and TH_ACK != 0) {
                    rcvAck = pkt.seq + 1
                    sndSeq += 1
                    peerAck = pkt.ack
                    mux.sendTcp(sport, dport, sndSeq, rcvAck, TH_ACK)
                    android.util.Log.i("SignumMux", "connected sport=$sport dport=$dport")
                    return
                }
            }
            throw TimeoutException("iPhone не ответил на USB. Разблокируйте телефон и нажмите «Доверять».")
        }

        fun write(data: ByteArray) {
            var offset = 0
            while (offset < data.size) {
                applyIncoming()
                val end = minOf(offset + MAX_PAYLOAD, data.size)
                val chunk = data.copyOfRange(offset, end)
                mux.sendTcp(sport, dport, sndSeq, rcvAck, TH_ACK, chunk)
                sndSeq += chunk.size
                offset = end
            }
        }

        fun read(max: Int, timeoutMs: Int): ByteArray {
            drain(80)
            if (inbox.isEmpty()) drain(timeoutMs)
            if (inbox.isEmpty()) return ByteArray(0)
            val n = minOf(max, inbox.size)
            val out = ByteArray(n)
            repeat(n) { out[it] = inbox.removeFirst() }
            return out
        }

        fun readExact(count: Int, timeoutMs: Int): ByteArray {
            val deadline = System.currentTimeMillis() + timeoutMs
            val out = ArrayList<Byte>(count)
            while (out.size < count) {
                val left = (deadline - System.currentTimeMillis()).toInt()
                if (left <= 0) error("USB: таймаут чтения")
                val chunk = read(count - out.size, left)
                if (chunk.isEmpty()) continue
                chunk.forEach { out.add(it) }
            }
            return out.toByteArray()
        }

        fun close() {
            mux.drop(sport)
        }

        private fun handleIncoming(pkt: MuxPacket) {
            if (pkt.flags and TH_RST != 0) {
                mux.drop(sport)
                throw UsbResetException(rstReason(pkt))
            }
            peerAck = pkt.ack
            if (pkt.payload.isNotEmpty()) {
                pkt.payload.forEach { inbox.addLast(it) }
                rcvAck = pkt.seq + pkt.payload.size
                mux.sendTcp(sport, dport, sndSeq, rcvAck, TH_ACK)
            }
        }

        private fun applyIncoming() {
            while (true) {
                val pkt = synchronized(this) {
                    if (pending.isNotEmpty()) pending.removeFirst() else null
                } ?: break
                handleIncoming(pkt)
            }
        }

        private fun collectIncoming(timeoutMs: Int) {
            applyIncoming()
            val deadline = System.currentTimeMillis() + timeoutMs
            while (System.currentTimeMillis() < deadline) {
                val wait = (deadline - System.currentTimeMillis()).toInt().coerceAtLeast(1)
                val pkt = nextPacket(wait) ?: break
                handleIncoming(pkt)
                applyIncoming()
                if (sndSeq - peerAck <= SEND_WINDOW / 2) break
            }
        }

        private fun drain(timeoutMs: Int) {
            val deadline = System.currentTimeMillis() + timeoutMs
            do {
                val wait = (deadline - System.currentTimeMillis()).toInt().coerceAtLeast(1)
                val pkt = nextPacket(wait) ?: break
                handleIncoming(pkt)
            } while (System.currentTimeMillis() < deadline && inbox.isEmpty())
        }

        private fun nextPacket(timeoutMs: Int): MuxPacket? {
            synchronized(this) {
                if (pending.isNotEmpty()) return pending.removeFirst()
            }
            val deadline = System.currentTimeMillis() + timeoutMs
            while (System.currentTimeMillis() < deadline) {
                mux.pump((deadline - System.currentTimeMillis()).toInt().coerceAtLeast(1))
                synchronized(this) {
                    if (pending.isNotEmpty()) return pending.removeFirst()
                }
            }
            return null
        }
    }

    companion object {
        const val TH_FIN = 0x01
        const val TH_SYN = 0x02
        const val TH_RST = 0x04
        const val TH_PSH = 0x08
        const val TH_ACK = 0x10
        const val LOCKDOWN_PORT = 62078
        private val nextSport = AtomicInteger(0x8000)
        private const val PROTO_VERSION = 0
        private const val PROTO_SETUP = 2
        private const val PROTO_TCP = 6
        private const val MAGIC = 0xfeedface.toInt()
        private const val TX_WIN = 131072
        private const val MAX_PAYLOAD_SAVED = 16_000
        private const val MAX_PAYLOAD = MAX_PAYLOAD_SAVED
        private const val SEND_WINDOW = 65_536

        private fun rstReason(pkt: MuxPacket): String {
            val text = runCatching { pkt.payload.toString(Charsets.UTF_8).trim() }.getOrDefault("")
            android.util.Log.w("SignumMux", "RST sport=${pkt.sport} dport=${pkt.dport} reason=$text")
            return text.take(120)
        }
    }
}
