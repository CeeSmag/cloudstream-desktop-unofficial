package com.lagradost.runtime.loader.stubs

import okhttp3.Headers
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.FileNotFoundException
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.Principal
import java.security.cert.Certificate
import java.util.Collections
import java.util.LinkedHashMap
import java.util.concurrent.TimeUnit
import javax.net.ssl.HostnameVerifier
import javax.net.ssl.HttpsURLConnection
import javax.net.ssl.SSLPeerUnverifiedException
import javax.net.ssl.SSLSocketFactory

/**
 * Shared OkHttp-backed execution core for [SafeHttpURLConnection] and [SafeHttpsURLConnection].
 *
 * All network I/O flows through the provided SSRF-guarded [OkHttpClient] (with `GuardedResolver`
 * DNS and per-hop `SafeInterceptor`), eliminating raw-JDK DNS-rebinding TOCTOU and redirect SSRF.
 */
internal class SafeHttpConnectionCore(
    private val url: URL,
    private val baseClient: OkHttpClient,
) {
    private val lock = Any()
    private val requestHeaders = LinkedHashMap<String, MutableList<String>>()
    private var requestBodyStream: BoundedBodyOutputStream? = null
    private var response: Response? = null
    private var responseBodyBytes: ByteArray? = null
    private var statusLine: String? = null
    private var headerEntries: List<Pair<String?, String>> = emptyList()
    private var headerMap: Map<String?, List<String>> = Collections.emptyMap()
    private var executed = false
    private var disconnected = false

    fun setRequestProperty(key: String?, value: String?) {
        if (key == null) throw NullPointerException("key is null")
        synchronized(lock) {
            checkNotConnected()
            if (value == null) {
                requestHeaders.remove(key)
            } else {
                requestHeaders[key] = mutableListOf(value)
            }
        }
    }

    fun addRequestProperty(key: String?, value: String?) {
        if (key == null) throw NullPointerException("key is null")
        synchronized(lock) {
            checkNotConnected()
            if (value != null) {
                requestHeaders.computeIfAbsent(key) { mutableListOf() }.add(value)
            }
        }
    }

    fun getRequestProperty(key: String?): String? = synchronized(lock) {
        if (key == null) return null
        requestHeaders[key]?.lastOrNull()
    }

    fun getRequestProperties(): Map<String, List<String>> = synchronized(lock) {
        val copy = LinkedHashMap<String, List<String>>()
        for ((k, v) in requestHeaders) {
            copy[k] = Collections.unmodifiableList(v.toList())
        }
        Collections.unmodifiableMap(copy)
    }

    fun getOutputStream(doOutput: Boolean): OutputStream = synchronized(lock) {
        if (!doOutput) {
            throw java.net.ProtocolException("cannot write to a URLConnection if doOutput=false")
        }
        checkNotConnected()
        requestBodyStream ?: BoundedBodyOutputStream(MAX_REQUEST_BODY_BYTES).also {
            requestBodyStream = it
        }
    }

    fun execute(
        method: String,
        connectTimeoutMs: Int,
        readTimeoutMs: Int,
        followRedirects: Boolean,
    ): Response {
        synchronized(lock) {
            response?.let { return it }
            if (disconnected) throw IOException("Connection already disconnected")
            executed = true
        }
        val request = buildRequest(method)
        val client = baseClient.newBuilder()
            .connectTimeout(timeoutOrDefault(connectTimeoutMs), TimeUnit.MILLISECONDS)
            .readTimeout(timeoutOrDefault(readTimeoutMs), TimeUnit.MILLISECONDS)
            .followRedirects(followRedirects)
            .followSslRedirects(followRedirects)
            .build()
        val executedResponse = client.newCall(request).execute()
        val bodyBytes = executedResponse.body.use { it.bytes() }
        synchronized(lock) {
            if (disconnected) {
                executedResponse.close()
                throw IOException("Connection disconnected during request")
            }
            response = executedResponse
            responseBodyBytes = bodyBytes
            indexHeaders(executedResponse)
            return executedResponse
        }
    }

    fun inputStream(
        method: String,
        connectTimeoutMs: Int,
        readTimeoutMs: Int,
        followRedirects: Boolean,
    ): InputStream {
        val resp = execute(method, connectTimeoutMs, readTimeoutMs, followRedirects)
        if (resp.code >= 400) {
            if (resp.code == HttpURLConnection.HTTP_NOT_FOUND || resp.code == HttpURLConnection.HTTP_GONE) {
                throw FileNotFoundException(url.toString())
            }
            throw IOException("Server returned HTTP response code: ${resp.code} for URL: $url")
        }
        val bytes = synchronized(lock) { responseBodyBytes ?: ByteArray(0) }
        return ByteArrayInputStream(bytes)
    }

    fun errorStream(): InputStream? = synchronized(lock) {
        val resp = response ?: return null
        if (resp.code < 400) return null
        val bytes = responseBodyBytes ?: return null
        ByteArrayInputStream(bytes)
    }

    fun headerField(name: String?): String? = synchronized(lock) {
        if (name == null) return statusLine
        response?.headers?.values(name)?.lastOrNull()
    }

    fun headerField(index: Int): String? = synchronized(lock) {
        headerEntries.getOrNull(index)?.second
    }

    fun headerFieldKey(index: Int): String? = synchronized(lock) {
        headerEntries.getOrNull(index)?.first
    }

    fun headerFields(): Map<String?, List<String>> = synchronized(lock) { headerMap }

    fun currentResponse(): Response? = synchronized(lock) { response }

    fun disconnect() {
        val toClose: Response?
        synchronized(lock) {
            disconnected = true
            toClose = response
            response = null
            responseBodyBytes = null
        }
        try {
            toClose?.close()
        } catch (_: Throwable) {}
    }

    private fun checkNotConnected() {
        if (executed) {
            throw IllegalStateException("Already connected")
        }
    }

    private fun buildRequest(method: String): Request {
        val builder = Request.Builder().url(url)
        val headersCopy: Map<String, List<String>>
        val bodyBytes: ByteArray?
        synchronized(lock) {
            headersCopy = requestHeaders.mapValues { it.value.toList() }
            bodyBytes = requestBodyStream?.toByteArray()
        }
        var contentType: String? = null
        for ((k, values) in headersCopy) {
            for (v in values) {
                builder.addHeader(k, v)
                if (k.equals("Content-Type", ignoreCase = true)) {
                    contentType = v
                }
            }
        }
        val effectiveMethod = if (bodyBytes != null && method == "GET") "POST" else method
        val body = when {
            bodyBytes != null -> bodyBytes.toRequestBody(contentType?.toMediaTypeOrNull())
            requiresBody(effectiveMethod) -> ByteArray(0).toRequestBody(contentType?.toMediaTypeOrNull())
            else -> null
        }
        return builder.method(effectiveMethod, body).build()
    }

    private fun indexHeaders(resp: Response) {
        val proto = resp.protocol.toString().uppercase()
        val line = "$proto ${resp.code} ${resp.message}".trim()
        statusLine = line
        val entries = ArrayList<Pair<String?, String>>(resp.headers.size + 1)
        val map = LinkedHashMap<String?, MutableList<String>>()
        entries.add(null to line)
        map.computeIfAbsent(null) { mutableListOf() }.add(line)
        val headers: Headers = resp.headers
        for (i in 0 until headers.size) {
            val k = headers.name(i)
            val v = headers.value(i)
            entries.add(k to v)
            map.computeIfAbsent(k) { mutableListOf() }.add(v)
        }
        headerEntries = entries
        headerMap = Collections.unmodifiableMap(map.mapValues { Collections.unmodifiableList(it.value) })
    }

    private class BoundedBodyOutputStream(private val maxBytes: Int) : ByteArrayOutputStream() {
        override fun write(b: Int) {
            if (size() + 1 > maxBytes) throw IOException("Request body exceeds $maxBytes byte limit")
            super.write(b)
        }

        override fun write(b: ByteArray, off: Int, len: Int) {
            if (len < 0 || size().toLong() + len.toLong() > maxBytes.toLong()) {
                throw IOException("Request body exceeds $maxBytes byte limit")
            }
            super.write(b, off, len)
        }
    }

    companion object {
        private const val DEFAULT_TIMEOUT_MS = 30_000L
        private const val MAX_REQUEST_BODY_BYTES = 16 * 1024 * 1024

        private fun timeoutOrDefault(ms: Int): Long = if (ms > 0) ms.toLong() else DEFAULT_TIMEOUT_MS

        private fun requiresBody(method: String): Boolean =
            method == "POST" || method == "PUT" || method == "PATCH"
    }
}

/** SSRF-guarded [HttpURLConnection] backed by [OkHttpClient] with [GuardedResolver]. */
internal class SafeHttpURLConnection(
    url: URL,
    client: OkHttpClient,
) : HttpURLConnection(url) {

    private val core = SafeHttpConnectionCore(url, client)

    override fun connect() {
        val resp = core.execute(method, connectTimeout, readTimeout, instanceFollowRedirects)
        responseCode = resp.code
        responseMessage = resp.message
        connected = true
    }

    override fun disconnect() {
        connected = false
        core.disconnect()
    }

    override fun usingProxy(): Boolean = false

    override fun getInputStream(): InputStream =
        core.inputStream(method, connectTimeout, readTimeout, instanceFollowRedirects).also {
            core.currentResponse()?.let { resp ->
                responseCode = resp.code
                responseMessage = resp.message
                connected = true
            }
        }

    override fun getErrorStream(): InputStream? = core.errorStream()

    override fun getOutputStream(): OutputStream = core.getOutputStream(doOutput)

    override fun getResponseCode(): Int {
        if (responseCode == -1) connect()
        return responseCode
    }

    override fun getResponseMessage(): String? {
        if (responseCode == -1) connect()
        return responseMessage
    }

    override fun setRequestProperty(key: String?, value: String?) = core.setRequestProperty(key, value)
    override fun addRequestProperty(key: String?, value: String?) = core.addRequestProperty(key, value)
    override fun getRequestProperty(key: String?): String? = core.getRequestProperty(key)
    override fun getRequestProperties(): Map<String, List<String>> = core.getRequestProperties()

    override fun getHeaderField(name: String?): String? {
        if (core.currentResponse() == null) tryConnectQuietly()
        return core.headerField(name)
    }

    override fun getHeaderField(n: Int): String? {
        if (core.currentResponse() == null) tryConnectQuietly()
        return core.headerField(n)
    }

    override fun getHeaderFieldKey(n: Int): String? {
        if (core.currentResponse() == null) tryConnectQuietly()
        return core.headerFieldKey(n)
    }

    override fun getHeaderFields(): Map<String?, List<String>> {
        if (core.currentResponse() == null) tryConnectQuietly()
        return core.headerFields()
    }

    private fun tryConnectQuietly() {
        try {
            connect()
        } catch (_: IOException) {}
    }
}

/** SSRF-guarded [HttpsURLConnection] backed by [OkHttpClient]; refuses custom SSL/hostname bypasses. */
internal class SafeHttpsURLConnection(
    url: URL,
    client: OkHttpClient,
) : HttpsURLConnection(url) {

    private val core = SafeHttpConnectionCore(url, client)

    override fun connect() {
        val resp = core.execute(method, connectTimeout, readTimeout, instanceFollowRedirects)
        responseCode = resp.code
        responseMessage = resp.message
        connected = true
    }

    override fun disconnect() {
        connected = false
        core.disconnect()
    }

    override fun usingProxy(): Boolean = false

    override fun getInputStream(): InputStream =
        core.inputStream(method, connectTimeout, readTimeout, instanceFollowRedirects).also {
            core.currentResponse()?.let { resp ->
                responseCode = resp.code
                responseMessage = resp.message
                connected = true
            }
        }

    override fun getErrorStream(): InputStream? = core.errorStream()

    override fun getOutputStream(): OutputStream = core.getOutputStream(doOutput)

    override fun getResponseCode(): Int {
        if (responseCode == -1) connect()
        return responseCode
    }

    override fun getResponseMessage(): String? {
        if (responseCode == -1) connect()
        return responseMessage
    }

    override fun setRequestProperty(key: String?, value: String?) = core.setRequestProperty(key, value)
    override fun addRequestProperty(key: String?, value: String?) = core.addRequestProperty(key, value)
    override fun getRequestProperty(key: String?): String? = core.getRequestProperty(key)
    override fun getRequestProperties(): Map<String, List<String>> = core.getRequestProperties()

    override fun getHeaderField(name: String?): String? {
        if (core.currentResponse() == null) tryConnectQuietly()
        return core.headerField(name)
    }

    override fun getHeaderField(n: Int): String? {
        if (core.currentResponse() == null) tryConnectQuietly()
        return core.headerField(n)
    }

    override fun getHeaderFieldKey(n: Int): String? {
        if (core.currentResponse() == null) tryConnectQuietly()
        return core.headerFieldKey(n)
    }

    override fun getHeaderFields(): Map<String?, List<String>> {
        if (core.currentResponse() == null) tryConnectQuietly()
        return core.headerFields()
    }

    override fun getCipherSuite(): String =
        core.currentResponse()?.handshake?.cipherSuite?.javaName
            ?: throw IllegalStateException("Connection not yet established")

    override fun getLocalCertificates(): Array<Certificate>? {
        val certs = core.currentResponse()?.handshake?.localCertificates ?: return null
        return if (certs.isEmpty()) null else certs.toTypedArray()
    }

    @Throws(SSLPeerUnverifiedException::class)
    override fun getServerCertificates(): Array<Certificate> {
        val certs = core.currentResponse()?.handshake?.peerCertificates
        if (certs.isNullOrEmpty()) throw SSLPeerUnverifiedException("No peer certificates")
        return certs.toTypedArray()
    }

    @Throws(SSLPeerUnverifiedException::class)
    override fun getPeerPrincipal(): Principal =
        core.currentResponse()?.handshake?.peerPrincipal
            ?: throw SSLPeerUnverifiedException("No peer principal")

    override fun getLocalPrincipal(): Principal? =
        core.currentResponse()?.handshake?.localPrincipal

    override fun setHostnameVerifier(v: HostnameVerifier?) {
        throw SecurityException("Plugin Security: Custom HostnameVerifier is blocked!")
    }

    override fun setSSLSocketFactory(sf: SSLSocketFactory?) {
        throw SecurityException("Plugin Security: Custom SSLSocketFactory is blocked!")
    }

    private fun tryConnectQuietly() {
        try {
            connect()
        } catch (_: IOException) {}
    }
}
