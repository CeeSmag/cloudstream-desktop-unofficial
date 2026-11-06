package com.lagradost.runtime.loader.stubs

import okhttp3.Dns
import okhttp3.Interceptor
import okhttp3.OkHttpClient
import okhttp3.Response
import java.io.IOException
import java.io.InputStream
import java.net.InetAddress
import java.net.Proxy
import java.net.ProxySelector
import java.net.SocketAddress
import java.net.URI
import java.net.URL
import java.net.URLConnection
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Network security gate for CloudStream plugins.
 *
 * Blocks `file://` and non-HTTP(S)/WS(S) schemes as well as SSRF / local-network connections
 * (loopback, localhost, RFC1918 private LAN, link-local / cloud metadata, CGNAT, and IPv6 ULA).
 */
object URLStub {
    private val ALLOWED_SCHEMES = setOf("http", "https", "ws", "wss", "socket")
    private val guardInstalled = AtomicBoolean(false)

    private object SafeDns : Dns {
        override fun lookup(hostname: String): List<InetAddress> {
            validateHost(hostname)
            val resolved = Dns.SYSTEM.lookup(hostname)
            for (addr in resolved) {
                if (isPrivateOrLocalAddress(addr)) {
                    throw SecurityException(
                        "Plugin Security: Private/local network address '${addr.hostAddress}' for '$hostname' is blocked!",
                    )
                }
            }
            return resolved
        }
    }

    private object SafeInterceptor : Interceptor {
        override fun intercept(chain: Interceptor.Chain): Response {
            val request = chain.request()
            val url = request.url
            validateScheme(url.scheme)
            validateHost(url.host)
            return chain.proceed(request)
        }
    }

    @JvmStatic
    fun installNetworkGuard() {
        if (!guardInstalled.compareAndSet(false, true)) return
        val delegate = ProxySelector.getDefault()
        ProxySelector.setDefault(object : ProxySelector() {
            override fun select(uri: URI?): List<Proxy> {
                if (uri != null) {
                    validateUri(uri)
                }
                return delegate?.select(uri) ?: listOf(Proxy.NO_PROXY)
            }

            override fun connectFailed(uri: URI?, sa: SocketAddress?, ioe: IOException?) {
                delegate?.connectFailed(uri, sa, ioe)
            }
        })
    }

    @JvmStatic
    fun buildSafeOkHttpClient(builder: OkHttpClient.Builder): OkHttpClient {
        if (!builder.interceptors().contains(SafeInterceptor)) {
            builder.addInterceptor(SafeInterceptor)
        }
        builder.dns(SafeDns)
        return builder.build()
    }

    @JvmStatic
    fun validateUrl(url: URL) {
        validateScheme(url.protocol)
        val host = url.host
        if (host.isNullOrBlank()) {
            throw SecurityException("Plugin Security: URL with missing host is blocked!")
        }
        validateHost(host)
    }

    @JvmStatic
    fun validateUri(uri: URI) {
        val scheme = uri.scheme
        if (!scheme.isNullOrBlank()) {
            validateScheme(scheme)
        }
        val host = uri.host ?: uri.authority?.substringBeforeLast(':')
        if (!host.isNullOrBlank()) {
            validateHost(host)
        }
    }

    @JvmStatic
    fun validateScheme(scheme: String?) {
        val normalized = scheme?.lowercase()?.trim() ?: ""
        if (normalized == "file") {
            throw SecurityException("Plugin Security: file:// URLs are blocked!")
        }
        if (normalized !in ALLOWED_SCHEMES) {
            throw SecurityException("Plugin Security: Disallowed URL scheme '$normalized'!")
        }
    }

    @JvmStatic
    fun validateHost(host: String) {
        val normalized = host.trim().trimStart('[').trimEnd(']').lowercase()
        if (normalized.isEmpty() ||
            normalized == "localhost" ||
            normalized.endsWith(".localhost") ||
            normalized.endsWith(".local")
        ) {
            throw SecurityException("Plugin Security: Local network access to '$host' is blocked!")
        }
        val addresses = try {
            InetAddress.getAllByName(normalized)
        } catch (_: Throwable) {
            return
        }
        for (addr in addresses) {
            if (isPrivateOrLocalAddress(addr)) {
                throw SecurityException(
                    "Plugin Security: Private/local network address '${addr.hostAddress}' for '$host' is blocked!",
                )
            }
        }
    }

    @JvmStatic
    fun isPrivateOrLocalAddress(addr: InetAddress): Boolean {
        if (addr.isLoopbackAddress ||
            addr.isAnyLocalAddress ||
            addr.isLinkLocalAddress ||
            addr.isSiteLocalAddress ||
            addr.isMulticastAddress
        ) {
            return true
        }
        val raw = addr.address
        if (raw.size == 4) {
            val b0 = raw[0].toInt() and 0xFF
            val b1 = raw[1].toInt() and 0xFF
            if (b0 == 0 || b0 == 10 || b0 == 127) return true
            if (b0 == 100 && b1 in 64..127) return true
            if (b0 == 169 && b1 == 254) return true
            if (b0 == 172 && b1 in 16..31) return true
            if (b0 == 192 && b1 == 168) return true
        } else if (raw.size == 16) {
            val b0 = raw[0].toInt() and 0xFF
            if ((b0 and 0xFE) == 0xFC) return true
        }
        return false
    }

    @JvmStatic
    fun openConnection(url: URL): URLConnection {
        validateUrl(url)
        return url.openConnection()
    }

    @JvmStatic
    fun openConnection(url: URL, proxy: Proxy): URLConnection {
        validateUrl(url)
        return url.openConnection(proxy)
    }

    @JvmStatic
    fun openStream(url: URL): InputStream {
        validateUrl(url)
        return url.openStream()
    }

    @JvmStatic
    fun getContent(url: URL): Any {
        validateUrl(url)
        return url.content
    }

    @JvmStatic
    fun getContent(url: URL, classes: Array<Class<*>>): Any? {
        validateUrl(url)
        return url.getContent(classes)
    }
}
