package com.lagradost.runtime.loader.stubs

import okhttp3.Dns
import org.junit.jupiter.api.Test
import java.net.InetAddress
import java.net.UnknownHostException
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

/** A resolver double that answers from a table and counts every lookup. */
private class FakeDns(private val answers: Map<String, List<String>>) : Dns {
    val lookups = mutableListOf<String>()

    override fun lookup(hostname: String): List<InetAddress> {
        lookups += hostname
        val ips = answers[hostname] ?: throw UnknownHostException(hostname)
        return ips.map { InetAddress.getByAddress(hostname, InetAddress.getByName(it).address) }
    }
}

class GuardedResolverTest {

    private val dns = FakeDns(
        mapOf(
            "public.example" to listOf("93.184.216.34"),
            "internal.example" to listOf("10.0.0.5"),
            "metadata.example" to listOf("169.254.169.254"),
            "mixed.example" to listOf("93.184.216.34", "192.168.1.1"),
            "ula.example" to listOf("fd00::1"),
            "empty.example" to emptyList(),
        ),
    )
    private val resolver = GuardedResolver(dns)

    @Test
    fun `public answers are returned`() {
        val result = resolver.resolve("public.example")
        assertEquals(listOf("93.184.216.34"), result.map { it.hostAddress })
    }

    @Test
    fun `private, metadata, mixed and ULA answers are refused`() {
        for (host in listOf("internal.example", "metadata.example", "mixed.example", "ula.example")) {
            assertFailsWith<SecurityException>(host) { resolver.resolve(host) }
        }
    }

    @Test
    fun `local names are refused before any lookup`() {
        for (host in listOf("localhost", "LOCALHOST", "printer.local", "app.localhost", "[localhost]", " ")) {
            assertFailsWith<SecurityException>(host) { resolver.resolve(host) }
        }
        assertEquals(emptyList(), dns.lookups)
    }

    @Test
    fun `unknown and empty answers are UnknownHostException`() {
        assertFailsWith<UnknownHostException> { resolver.resolve("nowhere.example") }
        assertFailsWith<UnknownHostException> { resolver.resolve("empty.example") }
    }
}

class InetAddressStubTest {

    private val dns = FakeDns(
        mapOf(
            "public.example" to listOf("93.184.216.34", "2606:2800:220:1::1"),
            "127.1" to listOf("127.0.0.1"),
            "lan.example" to listOf("192.168.0.10"),
        ),
    )
    private val resolution = InetAddressStub.Resolution(GuardedResolver(dns))

    @Test
    fun `literals become addresses without any lookup, private ones included`() {
        for (literal in listOf("10.0.0.0", "127.0.0.1", "fc00::", "::1", "[::1]", "93.184.216.34")) {
            val expected = InetAddress.getByAddress(IpLiteral.parse(literal)).address
            assertContentEquals(expected, resolution.getByName(literal).address, literal)
        }
        assertContentEquals(byteArrayOf(10, 0, 0, 0), resolution.getByName("10.0.0.0").address)
        assertEquals(1, resolution.getAllByName("10.0.0.1").size)
        assertEquals(emptyList(), dns.lookups, "a literal must never reach the resolver")
    }

    @Test
    fun `no host means loopback, as in the JDK`() {
        assertTrue(resolution.getByName(null).isLoopbackAddress)
        assertTrue(resolution.getByName("").isLoopbackAddress)
        assertTrue(resolution.getAllByName(null).single().isLoopbackAddress)
        assertEquals(emptyList(), dns.lookups)
    }

    @Test
    fun `host names go through the guarded resolver`() {
        assertEquals("93.184.216.34", resolution.getByName("public.example").hostAddress)
        assertEquals(2, resolution.getAllByName("public.example").size)
        assertFailsWith<SecurityException> { resolution.getByName("lan.example") }
        assertFailsWith<SecurityException> { resolution.getByName("localhost") }
        assertFailsWith<UnknownHostException> { resolution.getByName("nowhere.example") }
    }

    @Test
    fun `lenient numeric forms are looked up, and a loopback answer is refused`() {
        assertFailsWith<SecurityException> { resolution.getByName("127.1") }
        assertEquals(listOf("127.1"), dns.lookups)
    }

    @Test
    fun `local host is loopback and host names never need a reverse lookup`() {
        assertTrue(InetAddressStub.getLocalHost().isLoopbackAddress)
        val unnamed = InetAddress.getByAddress(byteArrayOf(10, 1, 2, 3))
        assertEquals("10.1.2.3", InetAddressStub.getHostName(unnamed))
        assertEquals("10.1.2.3", InetAddressStub.getCanonicalHostName(unnamed))
        val named = InetAddress.getByAddress("cdn.example", byteArrayOf(93.toByte(), 184.toByte(), 216.toByte(), 34))
        assertEquals("cdn.example", InetAddressStub.getHostName(named))
        assertEquals("cdn.example", InetAddressStub.getCanonicalHostName(named))
        val v6 = InetAddress.getByAddress(IpLiteral.parse("fc00::1"))
        assertEquals(v6.hostAddress, InetAddressStub.getHostName(v6))
    }

    @Test
    fun `reachability probes are refused`() {
        val addr = InetAddress.getLoopbackAddress()
        assertFailsWith<SecurityException> { InetAddressStub.isReachable(addr, 100) }
        assertFailsWith<SecurityException> { InetAddressStub.isReachable(addr, null, 1, 100) }
    }

    @Test
    fun `production stub parses literals without the system resolver`() {
        assertContentEquals(byteArrayOf(10, 0, 0, 0), InetAddressStub.getByName("10.0.0.0").address)
        assertEquals(1, InetAddressStub.getAllByName("::1").size)
    }
}
