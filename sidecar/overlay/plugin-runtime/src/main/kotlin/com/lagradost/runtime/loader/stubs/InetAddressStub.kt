package com.lagradost.runtime.loader.stubs

import java.net.InetAddress
import java.net.NetworkInterface
import java.net.UnknownHostException

/**
 * Guarded replacements for the members of [InetAddress] that touch the network or the host.
 * Plugin call sites are rewritten to these by
 * [com.lagradost.runtime.loader.GuardedCallRewriter], following
 * [com.lagradost.runtime.security.GuardedMemberPolicy].
 *
 * - IP literals become addresses with no lookup at all ([IpLiteral], then `getByAddress`).
 *   Literal private addresses are allowed as *values*: they carry no capability, and every
 *   consumer that could act on them (OkHttp `Dns`/interceptor, socket stubs) is guarded itself.
 * - Host names go through [GuardedResolver], which refuses local names and private answers.
 * - No reverse DNS, no machine name, no reachability probes.
 *
 * Stateless; [Resolution] holds only an immutable resolver.
 */
object InetAddressStub {

    private val production = Resolution(GuardedResolver.SYSTEM)

    @JvmStatic
    @Throws(UnknownHostException::class)
    fun getByName(host: String?): InetAddress = production.getByName(host)

    @JvmStatic
    @Throws(UnknownHostException::class)
    fun getAllByName(host: String?): Array<InetAddress> = production.getAllByName(host)

    /** The loopback address: the real local host name and LAN address stay private. */
    @JvmStatic
    fun getLocalHost(): InetAddress = InetAddress.getLoopbackAddress()

    /** The name the address was made with, else its literal text. Never a reverse lookup. */
    @JvmStatic
    fun getHostName(address: InetAddress): String {
        // InetAddress.toString() is "<name or empty>/<literal>" and never resolves.
        val name = address.toString().substringBefore('/')
        return name.ifEmpty { address.hostAddress }
    }

    /** Same as [getHostName]: the canonical name would need a reverse lookup. */
    @JvmStatic
    fun getCanonicalHostName(address: InetAddress): String = getHostName(address)

    @JvmStatic
    fun isReachable(address: InetAddress, timeout: Int): Boolean = refuseProbe(address)

    @JvmStatic
    fun isReachable(address: InetAddress, netif: NetworkInterface?, ttl: Int, timeout: Int): Boolean =
        refuseProbe(address)

    private fun refuseProbe(address: InetAddress): Boolean {
        throw SecurityException(
            "Plugin Security: InetAddress.isReachable('${address.hostAddress}') is blocked " +
                "(plugins may not probe hosts; use the app's HTTP client).",
        )
    }

    /** Name-to-address rules with an injectable resolver (tests use a fake one). */
    class Resolution(private val resolver: GuardedResolver) {

        @Throws(UnknownHostException::class)
        fun getByName(host: String?): InetAddress = getAllByName(host).first()

        @Throws(UnknownHostException::class)
        fun getAllByName(host: String?): Array<InetAddress> {
            // JDK semantics: no host means the loopback address.
            if (host.isNullOrEmpty()) return arrayOf(InetAddress.getLoopbackAddress())
            IpLiteral.parse(host)?.let { return arrayOf(InetAddress.getByAddress(it)) }
            return resolver.resolve(host).toTypedArray()
        }
    }
}
