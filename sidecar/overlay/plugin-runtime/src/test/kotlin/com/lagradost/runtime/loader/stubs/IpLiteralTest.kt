package com.lagradost.runtime.loader.stubs

import org.junit.jupiter.api.Test
import java.net.InetAddress
import kotlin.test.assertContentEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull

class IpLiteralTest {

    /** Valid literals; the JDK parses each without a lookup, so it is the oracle for the bytes. */
    private val valid = listOf(
        "0.0.0.0", "255.255.255.255", "10.0.0.0", "127.0.0.1", "192.168.1.20", "100.64.0.0",
        "::", "::1", "fc00::", "fe80::abcd", "FE80::ABCD", "2001:db8::1", "64:ff9b:1::",
        "1:2:3:4:5:6:7:8", "1:2:3:4:5:6:7::", "::2:3:4:5:6:7:8", "1::8",
        "::ffff:1.2.3.4", "64:ff9b::1.2.3.4", "1:2:3:4:5:6:1.2.3.4", "::1.2.3.4",
        "[::1]", "[2001:db8::1]",
    )

    /** Host names, lenient numeric forms the JDK or a resolver would accept, and malformed text. */
    private val notLiterals = listOf(
        "", "[]", "example.com", "localhost", "1.2.3.4.example.com",
        // Lenient IPv4 forms: never parsed here, so they go to the guarded resolver.
        "127.1", "0x7f.0.0.1", "017.0.0.1", "0177.0.0.1", "2130706433", "1.2.3.", ".1.2.3", "1..2.3",
        "256.0.0.1", "1.2.3.4.5", "1.2.3.-4", "+1.2.3.4", " 1.2.3.4", "1.2.3.4 ", "١.٢.٣.٤",
        // Malformed IPv6.
        ":::", "1:::2", "1::2::3", "12345::", ":1::2", "1::2:", "1:2:3:4:5:6:7:8:9", "1:2:3:4:5:6:7:8::",
        "::1:2:3:4:5:6:7:8", "1:2:3:4:5:6:7", "g::1", "::1.2.3", "1.2.3.4::", "::1.2.3.4:5", "1.2.3.4::1",
        "::ffff:01.2.3.4", "fe80::1%eth0", "fe80::1%1", "[::1", "::1]", "[[::1]]",
        // Brackets are for IPv6 only.
        "[10.0.0.1]",
    )

    @Test
    fun `valid literals parse to the bytes the JDK gives`() {
        for (text in valid) {
            val parsed = assertNotNull(IpLiteral.parse(text), "should parse: $text")
            val expected = InetAddress.getByName(text).address
            assertContentEquals(expected, InetAddress.getByAddress(parsed).address, text)
        }
    }

    @Test
    fun `anything that is not a strict literal is rejected`() {
        for (text in notLiterals) {
            assertNull(IpLiteral.parse(text), "should not be a literal: '$text'")
        }
    }

    @Test
    fun `ipv4 literal is four bytes and ipv6 sixteen`() {
        assertContentEquals(byteArrayOf(10, 0, 0, 1), IpLiteral.parse("10.0.0.1"))
        val v6 = assertNotNull(IpLiteral.parse("fc00::1"))
        assertContentEquals(byteArrayOf(0xfc.toByte(), 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1), v6)
    }
}
