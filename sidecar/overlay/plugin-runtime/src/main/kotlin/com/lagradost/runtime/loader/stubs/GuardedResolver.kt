package com.lagradost.runtime.loader.stubs

import okhttp3.Dns
import java.net.InetAddress
import java.net.UnknownHostException

/**
 * The one SSRF-guarded way plugin code turns a host name into addresses.
 *
 * Refuses local names (`localhost`, `*.local`, …) before any lookup, then refuses the answer when
 * any address is private or local ([URLStub.isPrivateOrLocalAddress]). Used by the OkHttp `Dns`
 * of every plugin client ([URLStub.buildSafeOkHttpClient]) and by [InetAddressStub].
 *
 * Holds only the injected [dns] (immutable), so one instance is safe to share between threads.
 */
class GuardedResolver(private val dns: Dns) {

    @Throws(UnknownHostException::class)
    fun resolve(hostname: String): List<InetAddress> {
        URLStub.validateHostName(hostname)
        val resolved = dns.lookup(hostname)
        if (resolved.isEmpty()) {
            throw UnknownHostException("No address for '$hostname'")
        }
        for (addr in resolved) {
            if (URLStub.isPrivateOrLocalAddress(addr)) {
                throw SecurityException(
                    "Plugin Security: Private/local network address '${addr.hostAddress}' for '$hostname' is blocked!",
                )
            }
        }
        return resolved
    }

    companion object {
        /** The system resolver behind the guard; what plugins get in production. */
        @JvmField
        val SYSTEM: GuardedResolver = GuardedResolver(Dns.SYSTEM)
    }
}
