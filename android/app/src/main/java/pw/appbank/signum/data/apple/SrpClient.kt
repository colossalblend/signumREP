package pw.appbank.signum.data.apple

import org.bouncycastle.crypto.digests.SHA256Digest
import java.math.BigInteger
import java.security.SecureRandom

   
                                                                          
                                                             
   
class SrpClient(
    private val username: ByteArray,
) {
    private val aSecret: ByteArray
    private val a: BigInteger
                                                          
    val aPub: ByteArray

    init {
        val buf = ByteArray(32)
        val rnd = SecureRandom()
        var priv: BigInteger
        do {
            rnd.nextBytes(buf)
            priv = BigInteger(1, buf)
        } while (priv <= BigInteger.ONE || priv >= N)
        aSecret = buf.copyOf()
        a = priv
        aPub = trim(g.modPow(a, N).toUnsigned())
    }

    fun processReply(password: ByteArray, salt: ByteArray, bPubBytes: ByteArray): Verifier {
        val bPub = BigInteger(1, bPubBytes)
        if (bPub.mod(N) == BigInteger.ZERO) error("Apple не подтвердил вход. Проверьте пароль и повторите.")

                                                         
        val identity = sha256(byteArrayOf(0x3A) + password)
        val x = BigInteger(1, sha256(salt + identity))

        val aPad = pad(aPub)
        val bPad = pad(bPubBytes)
        val u = BigInteger(1, sha256(aPad + bPad))
        val k = BigInteger(1, sha256(pad(N) + pad(g.toUnsigned())))

        val gx = g.modPow(x, N)
        val base = bPub.subtract(k.multiply(gx)).mod(N)
        val exp = a.add(u.multiply(x))
        val s = base.modPow(exp, N)
        val premaster = trim(s.toUnsigned())
        val sessionKey = sha256(premaster)

                                                                              
        val nHash = sha256(pad(N))
        val gHash = sha256(pad(g.toUnsigned()))
        val xorNg = ByteArray(nHash.size) { i -> (nHash[i].toInt() xor gHash[i].toInt()).toByte() }
        val m1 = sha256(xorNg + sha256(username) + salt + aPub + bPubBytes + sessionKey)
        val m2 = sha256(aPub + m1 + sessionKey)
        return Verifier(premaster, sessionKey, m1, m2)
    }

    class Verifier(
                                                                                    
        val key: ByteArray,
        val sessionKey: ByteArray,
        val proof: ByteArray,
        val m2: ByteArray,
    ) {
        private val expectedM2: ByteArray get() = m2
        fun verifyServer(m2: ByteArray) {
            if (!expectedM2.contentEquals(m2)) {
                error("Apple не подтвердил вход. Проверьте пароль и повторите.")
            }
        }
    }

    companion object {
        private val N = BigInteger(
            "AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050" +
                "A37329CBB4A099ED8193E0757767A13DD52312AB4B03310DCD7F48A9DA04FD50" +
                "E8083969EDB767B0CF6095179A163AB3661A05FBD5FAAAE82918A9962F0B93B8" +
                "55F97993EC975EEAA80D740ADBF4FF747359D041D5C33EA71D281E446B14773B" +
                "CA97B43A23FB801676BD207A436C6481F1D2B9078717461A5B9D32E688F87748" +
                "544523B524B0D57D5EA77A2775D2ECFA032CFBDBF52FB3786160279004E57AE6" +
                "AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DBFBB6" +
                "94B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73",
            16,
        )
        private val g = BigInteger.valueOf(2)
        private val nLen = (N.bitLength() + 7) / 8

        fun sha256(data: ByteArray): ByteArray {
            val d = SHA256Digest()
            d.update(data, 0, data.size)
            val out = ByteArray(32)
            d.doFinal(out, 0)
            return out
        }

        fun pad(value: BigInteger): ByteArray = pad(value.toUnsigned())

        fun pad(raw: ByteArray): ByteArray {
            val unsigned = trim(raw)
            if (unsigned.size >= nLen) return unsigned
            return ByteArray(nLen - unsigned.size) + unsigned
        }

        fun trim(raw: ByteArray): ByteArray {
            var i = 0
            while (i < raw.size - 1 && raw[i] == 0.toByte()) i++
            return raw.copyOfRange(i, raw.size)
        }

        private fun BigInteger.toUnsigned(): ByteArray {
            val raw = toByteArray()
            return if (raw.isNotEmpty() && raw[0] == 0.toByte()) raw.copyOfRange(1, raw.size) else raw
        }
    }
}
