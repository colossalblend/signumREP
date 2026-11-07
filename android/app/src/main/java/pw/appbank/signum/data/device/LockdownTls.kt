package pw.appbank.signum.data.device

import org.bouncycastle.crypto.util.PrivateKeyFactory
import org.bouncycastle.tls.AlertLevel
import org.bouncycastle.tls.Certificate
import org.bouncycastle.tls.CertificateRequest
import org.bouncycastle.tls.CipherSuite
import org.bouncycastle.tls.DefaultTlsClient
import org.bouncycastle.tls.HashAlgorithm
import org.bouncycastle.tls.ProtocolVersion
import org.bouncycastle.tls.SignatureAlgorithm
import org.bouncycastle.tls.SignatureAndHashAlgorithm
import org.bouncycastle.tls.TlsAuthentication
import org.bouncycastle.tls.TlsClientContext
import org.bouncycastle.tls.TlsClientProtocol
import org.bouncycastle.tls.TlsCredentials
import org.bouncycastle.tls.TlsExtensionsUtils
import org.bouncycastle.tls.TlsServerCertificate
import org.bouncycastle.tls.TlsUtils
import org.bouncycastle.tls.crypto.impl.bc.BcDefaultTlsCredentialedSigner
import org.bouncycastle.tls.crypto.impl.bc.BcTlsCrypto
import pw.appbank.signum.data.api.DebugDump
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.security.PrivateKey
import java.security.SecureRandom
import java.util.Hashtable
import java.util.Vector

internal fun lockdownTlsConnect(
    raw: UsbMux.MuxStream,
    hostCertDer: ByteArray,
    hostKey: PrivateKey,
    rootCertDer: ByteArray?,
): TlsClientProtocol {
    val crypto = BcTlsCrypto(SecureRandom())
    val input = MuxIn(raw)
    val output = RecordMuxOut(raw)
    val protocol = TlsClientProtocol(input, output)
    protocol.connect(LockdownTlsClient(crypto, hostCertDer, hostKey, rootCertDer))
    DebugDump.pair("lockdown TLS ready (bc-tls)")
    return protocol
}

private class LockdownTlsClient(
    private val bcCrypto: BcTlsCrypto,
    private val hostCertDer: ByteArray,
    private val hostKey: PrivateKey,
    private val rootCertDer: ByteArray?,
) : DefaultTlsClient(bcCrypto) {
    private var tlsContext: TlsClientContext? = null

    override fun init(context: TlsClientContext) {
        super.init(context)
        tlsContext = context
    }

    override fun getSupportedVersions(): Array<ProtocolVersion> =
        arrayOf(ProtocolVersion.TLSv12, ProtocolVersion.TLSv10)

    override fun getSupportedCipherSuites(): IntArray = intArrayOf(
        CipherSuite.TLS_RSA_WITH_AES_256_CBC_SHA,
        CipherSuite.TLS_RSA_WITH_AES_128_CBC_SHA,
    )

    override fun shouldUseExtendedMasterSecret(): Boolean = false

    override fun requiresCloseNotify(): Boolean = false

    override fun shouldCheckSigAlgOfPeerCerts(): Boolean = false

    override fun shouldUseGMTUnixTime(): Boolean = true

    override fun notifySecureRenegotiation(secureRenegotiation: Boolean) {
        DebugDump.pair("tls renegotiation=$secureRenegotiation")
    }

    override fun getClientExtensions(): Hashtable<Any?, Any?> {
        val ext = Hashtable<Any?, Any?>()
        val algs = Vector<SignatureAndHashAlgorithm>()
        algs.add(SignatureAndHashAlgorithm(HashAlgorithm.sha1, SignatureAlgorithm.rsa))
        algs.add(SignatureAndHashAlgorithm(HashAlgorithm.sha256, SignatureAlgorithm.rsa))
        @Suppress("UNCHECKED_CAST")
        TlsExtensionsUtils.addSignatureAlgorithmsExtension(ext as Hashtable<*, *>, algs)
        DebugDump.pair("tls clienthello sigalgs=sha1-rsa,sha256-rsa")
        return ext
    }

    override fun notifyServerVersion(serverVersion: ProtocolVersion) {
        DebugDump.pair("tls server=$serverVersion")
    }

    override fun notifySelectedCipherSuite(selectedCipherSuite: Int) {
        DebugDump.pair("tls cipher=0x${selectedCipherSuite.toString(16)}")
    }

    override fun notifyAlertRaised(
        alertLevel: Short,
        alertDescription: Short,
        message: String?,
        cause: Throwable?,
    ) {
        DebugDump.pair("tls alert raised level=$alertLevel desc=$alertDescription msg=$message")
        if (cause != null) android.util.Log.e("SignumPair", "tls alert cause", cause)
    }

    override fun notifyAlertReceived(alertLevel: Short, alertDescription: Short) {
        val kind = if (alertLevel.toInt() == AlertLevel.fatal.toInt()) "fatal" else "warn"
        DebugDump.pair("tls alert recv $kind desc=$alertDescription")
    }

    override fun getAuthentication(): TlsAuthentication = object : TlsAuthentication {
        override fun notifyServerCertificate(serverCertificate: TlsServerCertificate) {
            val n = serverCertificate.certificate?.length ?: 0
            DebugDump.pair("tls server cert ok chain=$n")
        }

        override fun getClientCredentials(certificateRequest: CertificateRequest): TlsCredentials {
            val ctx = tlsContext ?: error("TLS context")
            DebugDump.pair("tls client cert der=${hostCertDer.size}b root=${rootCertDer?.size ?: 0}b")
            val host = bcCrypto.createCertificate(hostCertDer)
            val chain = if (rootCertDer != null) {
                Certificate(arrayOf(host, bcCrypto.createCertificate(rootCertDer)))
            } else {
                Certificate(arrayOf(host))
            }
            val priv = PrivateKeyFactory.createKey(hostKey.encoded)
            val negotiated = ctx.securityParametersHandshake?.negotiatedVersion
            val alg = if (negotiated != null && TlsUtils.isTLSv12(negotiated)) {
                SignatureAndHashAlgorithm(HashAlgorithm.sha1, SignatureAlgorithm.rsa)
            } else {
                null
            }
            return BcDefaultTlsCredentialedSigner(
                org.bouncycastle.tls.crypto.TlsCryptoParameters(ctx),
                bcCrypto,
                priv,
                chain,
                alg,
            )
        }
    }
}

private class MuxIn(private val raw: UsbMux.MuxStream) : InputStream() {
    private var buf = ByteArray(0)
    private var pos = 0

    override fun read(): Int {
        val one = ByteArray(1)
        val n = read(one, 0, 1)
        return if (n <= 0) -1 else one[0].toInt() and 0xff
    }

    override fun read(b: ByteArray, off: Int, len: Int): Int {
        if (len <= 0) return 0
        if (pos >= buf.size) {
            val deadline = System.currentTimeMillis() + 20_000
            while (System.currentTimeMillis() < deadline) {
                val wait = (deadline - System.currentTimeMillis()).toInt().coerceAtLeast(1)
                buf = raw.read(8_192, wait)
                pos = 0
                if (buf.isNotEmpty()) break
            }
            if (buf.isEmpty() || pos >= buf.size) {
                throw IOException("USB: таймаут TLS (нет ответа iPhone)")
            }
        }
        val n = minOf(len, buf.size - pos)
        System.arraycopy(buf, pos, b, off, n)
        pos += n
        return n
    }
}

   
                                                                            
                                                                          
                                                                                  
   
private class RecordMuxOut(private val raw: UsbMux.MuxStream) : OutputStream() {
    private val acc = ByteArrayOutputStream()
    private var first = true

    override fun write(b: Int) = write(byteArrayOf(b.toByte()))

    override fun write(b: ByteArray, off: Int, len: Int) {
        if (len <= 0) return
        acc.write(b, off, len)
        emitCompleteRecords()
    }

    override fun flush() {
        emitCompleteRecords()
        if (acc.size() > 0) {
            DebugDump.pair("tls flush leftover=${acc.size()} (held)")
        }
    }

    private fun emitCompleteRecords() {
        val data = acc.toByteArray()
        var i = 0
        while (i + 5 <= data.size) {
            val recLen = ((data[i + 3].toInt() and 0xff) shl 8) or (data[i + 4].toInt() and 0xff)
            val total = 5 + recLen
            if (total <= 5 || i + total > data.size) break
            emit(data.copyOfRange(i, i + total))
            i += total
        }
        acc.reset()
        if (i < data.size) acc.write(data, i, data.size - i)
    }

    private fun emit(pkt: ByteArray) {
        if (pkt.isEmpty()) return
        if (first) {
            first = false
            val n = minOf(pkt.size, 12)
            val head = (0 until n).joinToString(" ") { "%02x".format(pkt[it]) }
            DebugDump.pair("tls first record ${pkt.size} bytes: $head")
        }
        raw.write(pkt)
    }
}

internal class BcTlsPipe(private val protocol: TlsClientProtocol) {
    fun write(data: ByteArray) {
        protocol.outputStream.write(data)
        protocol.outputStream.flush()
    }

    fun readExact(count: Int, timeoutMs: Int): ByteArray {
        val deadline = System.currentTimeMillis() + timeoutMs
        val out = ByteArray(count)
        var got = 0
        while (got < count) {
            if (System.currentTimeMillis() > deadline) error("USB: таймаут TLS")
            val n = try {
                protocol.inputStream.read(out, got, count - got)
            } catch (e: IOException) {
                throw e
            }
            if (n < 0) error("TLS lockdown закрыт")
            got += n
        }
        return out
    }
}
