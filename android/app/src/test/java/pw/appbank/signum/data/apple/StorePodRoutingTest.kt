package pw.appbank.signum.data.apple

import org.junit.Assert.*
import org.junit.Test
import java.io.IOException
import java.net.SocketTimeoutException
import java.net.UnknownHostException

class StorePodRoutingTest {
    @Test
    fun stalePodFallsBackAndFollowsCompleteHandshakeUrl() {
        val urls = mutableListOf<String>()
        val pods = mutableListOf<String>()
        val start = "https://p99-buy.itunes.apple.com/purchases?page=2&range=2025-all"
        val final = "https://p44-buy.itunes.apple.com/purchases?page=2&range=2025-all&mzcprh=1"
        val result = StorePodRouting.execute(start, send = { url ->
            urls += url
            when (urls.size) {
                1 -> throw UnknownHostException("stale pod")
                2 -> StorePodRouting.Reply(302, "", final, "44")
                else -> StorePodRouting.Reply(200, "ok")
            }
        }, onPod = { pods += it })
        assertEquals("ok", result)
        assertEquals(listOf(start, "https://buy.itunes.apple.com/purchases?page=2&range=2025-all", final), urls)
        assertEquals("44", pods.last())
    }

    @Test
    fun sameHostRedirectKeepsHandshakeAndUsesNewCookie() {
        var cookie = ""
        var calls = 0
        val result = StorePodRouting.execute("https://p25-buy.itunes.apple.com/purchases?page=2", send = { url ->
            if (++calls == 1) {
                cookie = "itspod=25"
                StorePodRouting.Reply(302, "", "?page=2&mzcprh=1")
            } else {
                assertEquals("https://p25-buy.itunes.apple.com/purchases?page=2&mzcprh=1", url)
                assertEquals("itspod=25", cookie)
                StorePodRouting.Reply(200, "ok")
            }
        }, onPod = {})
        assertEquals("ok", result)
    }

    @Test
    fun genericDnsFailureAndTimeoutDoNotTriggerPodFallback() {
        var calls = 0
        assertThrows(UnknownHostException::class.java) {
            StorePodRouting.execute<String>("https://buy.itunes.apple.com/purchases", send = { _: String ->
                calls++
                throw UnknownHostException("offline")
            }, onPod = {})
        }
        assertEquals(1, calls)
        calls = 0
        assertThrows(SocketTimeoutException::class.java) {
            StorePodRouting.execute<String>("https://p25-buy.itunes.apple.com/purchases", send = { _: String ->
                calls++
                throw SocketTimeoutException("slow")
            }, onPod = {})
        }
        assertEquals(1, calls)
    }

    @Test
    fun repeatedRedirectsAreBoundedAndMissingLocationIsAnError() {
        var calls = 0
        assertThrows(IOException::class.java) {
            StorePodRouting.execute<String>("https://p25-buy.itunes.apple.com/purchases", send = { url ->
                calls++
                StorePodRouting.Reply(302, "", url)
            }, onPod = {})
        }
        assertEquals(6, calls)
        assertThrows(IOException::class.java) {
            StorePodRouting.execute<String>("https://buy.itunes.apple.com/purchases", send = {
                StorePodRouting.Reply(302, "")
            }, onPod = {})
        }
    }

    @Test
    fun headerPodWinsAndInvalidPodIsIgnored() {
        var pod = ""
        StorePodRouting.execute("https://p25-buy.itunes.apple.com/purchases", send = {
            StorePodRouting.Reply(200, "ok", pod = "44")
        }, onPod = { pod = it })
        assertEquals("44", pod)
        assertNull(StorePodRouting.validPod("../44"))
        assertNull(StorePodRouting.validPod("44.invalid"))
        assertNull(StorePodRouting.withoutPod("https://auth.itunes.apple.com/auth"))
    }

    @Test
    fun relativeRedirectPreservesEncodingAndRejectsNonAppleHosts() {
        assertEquals("https://buy.itunes.apple.com/a/next?page=2&x=%2F", StorePodRouting.redirectUrl(
            "https://buy.itunes.apple.com/a/purchases", "next?page=2&x=%2F"))
        for (location in listOf("https://apple.com.invalid/auth", "http://buy.itunes.apple.com/auth", "https://user@buy.itunes.apple.com/auth")) {
            assertThrows(IllegalArgumentException::class.java) {
                StorePodRouting.redirectUrl("https://buy.itunes.apple.com/auth", location)
            }
        }
    }
}
