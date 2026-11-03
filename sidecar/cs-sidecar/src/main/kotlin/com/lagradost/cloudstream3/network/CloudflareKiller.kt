package com.lagradost.cloudstream3.network

import okhttp3.Interceptor
import okhttp3.Response

/**
 * Stand-in for CloudStream's `CloudflareKiller`, which solves Cloudflare challenges in a WebView.
 * The sidecar has no WebView, so requests pass through unchanged: sites that sit behind a
 * Cloudflare challenge will answer 403/503 and the plugin reports that as its own failure.
 * Plugins still link and load because the class has the same shape as the real one.
 */
class CloudflareKiller : Interceptor {
    /** Cookies by host; always empty because no challenge is ever solved here. */
    val savedCookies: MutableMap<String, Map<String, String>> = mutableMapOf()

    override fun intercept(chain: Interceptor.Chain): Response {
        val response = chain.proceed(chain.request())
        if (response.code in CHALLENGE_CODES && response.header("Server")?.startsWith("cloudflare") == true) {
            sidecar.SidecarLog.warn("Cloudflare challenge on ${chain.request().url.host}: cannot be solved without a browser")
        }
        return response
    }

    companion object {
        private val CHALLENGE_CODES = setOf(403, 503)

        /** Splits a `Cookie` header value into a name-to-value map. */
        fun parseCookieMap(cookie: String): Map<String, String> =
            cookie.split(";").filter { it.contains("=") }.associate {
                val (name, value) = it.trim().split("=", limit = 2)
                name to value
            }
    }
}
