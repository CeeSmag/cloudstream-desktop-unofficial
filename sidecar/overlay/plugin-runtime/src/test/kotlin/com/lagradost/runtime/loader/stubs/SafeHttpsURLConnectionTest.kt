package com.lagradost.runtime.loader.stubs

import okhttp3.CipherSuite
import okhttp3.Handshake
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okhttp3.TlsVersion
import okio.Buffer
import org.junit.jupiter.api.Test
import java.io.FileNotFoundException
import java.io.IOException
import java.math.BigInteger
import java.net.ProtocolException
import java.net.URI
import java.net.URL
import java.security.Principal
import java.security.PublicKey
import java.security.cert.X509Certificate
import java.util.Date
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import javax.net.ssl.HostnameVerifier
import javax.net.ssl.SSLPeerUnverifiedException
import javax.net.ssl.SSLSocketFactory
import javax.security.auth.x500.X500Principal
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Contract and boundary tests for [SafeHttpsURLConnection], [SafeHttpURLConnection],
 * and [SafeHttpConnectionCore] using deterministic in-process OkHttp transport interceptors.
 */
class SafeHttpsURLConnectionTest {

    private fun url(raw: String): URL = URI(raw).toURL()

    private fun stubClient(handler: (Interceptor.Chain) -> Response): OkHttpClient =
        OkHttpClient.Builder().addInterceptor(Interceptor { handler(it) }).build()

    private fun okResponse(
        request: Request,
        code: Int = 200,
        message: String = "OK",
        body: String = "ok",
        headers: List<Pair<String, String>> = emptyList(),
        handshake: Handshake? = null,
    ): Response {
        val builder = Response.Builder()
            .request(request)
            .protocol(Protocol.HTTP_1_1)
            .code(code)
            .message(message)
            .body(body.toResponseBody("text/plain".toMediaType()))
        if (handshake != null) builder.handshake(handshake)
        for ((k, v) in headers) builder.addHeader(k, v)
        return builder.build()
    }

    @Test
    fun `SafeHttpsURLConnection exposes TLS handshake before and after connect`() {
        val peerCert = FakeX509Certificate("CN=peer.example")
        val localCert = FakeX509Certificate("CN=client.example")
        val handshake = Handshake.get(
            TlsVersion.TLS_1_3,
            CipherSuite.TLS_AES_128_GCM_SHA256,
            listOf(peerCert),
            listOf(localCert),
        )
        val client = stubClient { okResponse(it.request(), body = "secure-body", handshake = handshake) }
        val conn = SafeHttpsURLConnection(url("https://example.test/secure"), client)
        assertFalse(conn.usingProxy())
        assertFailsWith<IllegalStateException> { conn.cipherSuite }
        assertNull(conn.localCertificates)
        assertFailsWith<SSLPeerUnverifiedException> { conn.serverCertificates }
        assertFailsWith<SSLPeerUnverifiedException> { conn.peerPrincipal }
        assertNull(conn.localPrincipal)

        assertEquals("secure-body", conn.inputStream.bufferedReader().readText())
        assertEquals(CipherSuite.TLS_AES_128_GCM_SHA256.javaName, conn.cipherSuite)
        assertEquals(1, conn.serverCertificates.size)
        assertEquals("CN=peer.example", conn.peerPrincipal.name)
        assertEquals(1, assertNotNull(conn.localCertificates).size)
        assertEquals("CN=client.example", assertNotNull(conn.localPrincipal).name)
        // responseCode populated during getInputStream remains readable after disconnect.
        conn.disconnect()
        assertEquals(200, conn.responseCode)
        assertEquals("OK", conn.responseMessage)
    }

    @Test
    fun `SafeHttpsURLConnection indexes response headers and request properties`() {
        val client = stubClient {
            okResponse(it.request(), headers = listOf("X-A" to "one", "X-A" to "two", "X-B" to "three"))
        }
        val conn = SafeHttpsURLConnection(url("https://example.test/hdrs"), client)
        conn.setRequestProperty("X-Req", "first")
        conn.addRequestProperty("X-Req", "second")
        conn.addRequestProperty("X-Req", null)
        assertEquals("second", conn.getRequestProperty("X-Req"))
        assertNull(conn.getRequestProperty(null))
        assertEquals(listOf("first", "second"), conn.requestProperties["X-Req"])

        assertEquals("OK", conn.responseMessage)
        assertNull(conn.errorStream)
        assertEquals("HTTP/1.1 200 OK", conn.getHeaderField(null))
        assertEquals("two", conn.getHeaderField("X-A"))
        assertEquals("HTTP/1.1 200 OK", conn.getHeaderField(0))
        assertNull(conn.getHeaderFieldKey(0))
        assertEquals("X-A", conn.getHeaderFieldKey(1))
        assertEquals("one", conn.getHeaderField(1))
        assertEquals("three", conn.getHeaderField(3))
        assertNull(conn.getHeaderField(99))
        assertNull(conn.getHeaderFieldKey(99))
        assertEquals(listOf("one", "two"), conn.headerFields["X-A"])
        assertEquals(listOf("HTTP/1.1 200 OK"), conn.headerFields[null])
        conn.disconnect()
    }

    @Test
    fun `SafeHttpsURLConnection handles empty handshake certs and blocks TLS bypass setters`() {
        val emptyHandshake = Handshake.get(
            TlsVersion.TLS_1_3,
            CipherSuite.TLS_AES_128_GCM_SHA256,
            emptyList(),
            emptyList(),
        )
        val client = stubClient { okResponse(it.request(), handshake = emptyHandshake) }
        val conn = SafeHttpsURLConnection(url("https://example.test/empty-certs"), client)
        assertEquals(200, conn.responseCode)
        assertNull(conn.localCertificates)
        assertNull(conn.localPrincipal)
        assertFailsWith<SSLPeerUnverifiedException> { conn.serverCertificates }
        assertFailsWith<SSLPeerUnverifiedException> { conn.peerPrincipal }
        assertFailsWith<SecurityException> { conn.hostnameVerifier = HostnameVerifier { _, _ -> true } }
        assertFailsWith<SecurityException> { conn.sslSocketFactory = SSLSocketFactory.getDefault() as SSLSocketFactory }
    }

    @Test
    fun `header getters and responseMessage lazily connect on both HTTP and HTTPS`() {
        val okClient = stubClient { okResponse(it.request(), message = "Created", headers = listOf("X-Lazy" to "ready")) }
        assertEquals("Created", SafeHttpURLConnection(url("http://example.test/m"), okClient).responseMessage)
        assertEquals("ready", SafeHttpURLConnection(url("http://example.test/l"), okClient).getHeaderField("X-Lazy"))
        assertEquals("ready", SafeHttpURLConnection(url("http://example.test/l"), okClient).getHeaderField(1))
        assertEquals("X-Lazy", SafeHttpURLConnection(url("http://example.test/l"), okClient).getHeaderFieldKey(1))
        assertEquals(listOf("ready"), SafeHttpURLConnection(url("http://example.test/l"), okClient).headerFields["X-Lazy"])

        assertEquals("Created", SafeHttpsURLConnection(url("https://example.test/m"), okClient).responseMessage)
        assertEquals("ready", SafeHttpsURLConnection(url("https://example.test/l"), okClient).getHeaderField("X-Lazy"))
        assertEquals("ready", SafeHttpsURLConnection(url("https://example.test/l"), okClient).getHeaderField(1))
        assertEquals("X-Lazy", SafeHttpsURLConnection(url("https://example.test/l"), okClient).getHeaderFieldKey(1))
        assertEquals(listOf("ready"), SafeHttpsURLConnection(url("https://example.test/l"), okClient).headerFields["X-Lazy"])

        val failClient = stubClient { throw IOException("network down") }
        assertNull(SafeHttpURLConnection(url("http://example.test/down"), failClient).getHeaderField("X-Lazy"))
        assertNull(SafeHttpsURLConnection(url("https://example.test/down"), failClient).getHeaderField("X-Lazy"))
    }

    @Test
    fun `status code boundaries 399 400 410 and 500 behave according to HttpURLConnection contract`() {
        val c399 = SafeHttpURLConnection(url("http://example.test/399"), stubClient { okResponse(it.request(), 399, "Custom", "ok399") })
        assertEquals("ok399", c399.inputStream.bufferedReader().readText())
        assertNull(c399.errorStream)
        assertFalse(c399.usingProxy())
        c399.disconnect()
        assertEquals(399, c399.responseCode)
        assertEquals("Custom", c399.responseMessage)

        val c400 = SafeHttpsURLConnection(url("https://example.test/400"), stubClient { okResponse(it.request(), 400, "Bad", "bad-req") })
        assertFailsWith<IOException> { c400.inputStream }
        assertEquals("bad-req", assertNotNull(c400.errorStream).bufferedReader().readText())

        val c410 = SafeHttpsURLConnection(url("https://example.test/410"), stubClient { okResponse(it.request(), 410, "Gone", "gone") })
        assertFailsWith<FileNotFoundException> { c410.inputStream }
        assertEquals("gone", assertNotNull(c410.errorStream).bufferedReader().readText())

        val c500 = SafeHttpURLConnection(url("http://example.test/500"), stubClient { okResponse(it.request(), 500, "Err", "boom") })
        val err = assertFailsWith<IOException> { c500.inputStream }
        assertFalse(err is FileNotFoundException)
        assertEquals("boom", assertNotNull(c500.errorStream).bufferedReader().readText())
    }

    @Test
    fun `request properties GET-to-POST body upgrade timeouts and post-connect guards hold`() {
        var seenMethod = ""
        var seenContentType: String? = null
        var seenBody = ""
        var seenConnectMs = -1
        var seenReadMs = -1
        val client = stubClient { chain ->
            val req = chain.request()
            seenMethod = req.method
            seenContentType = req.body?.contentType()?.toString()
            seenBody = req.body?.let { b -> Buffer().also { b.writeTo(it) }.readUtf8() }.orEmpty()
            seenConnectMs = chain.connectTimeoutMillis()
            seenReadMs = chain.readTimeoutMillis()
            okResponse(req)
        }
        val conn = SafeHttpURLConnection(url("http://example.test/up"), client)
        assertFailsWith<NullPointerException> { conn.setRequestProperty(null, "v") }
        assertFailsWith<NullPointerException> { conn.addRequestProperty(null, "v") }
        assertFailsWith<ProtocolException> { conn.outputStream }
        conn.setRequestProperty("X-Drop", "val")
        conn.setRequestProperty("X-Drop", null)
        assertNull(conn.getRequestProperty("X-Drop"))
        conn.addRequestProperty("X-Multi", "a")
        conn.addRequestProperty("X-Multi", "b")
        assertEquals("b", conn.getRequestProperty("X-Multi"))
        assertEquals(listOf("a", "b"), conn.requestProperties["X-Multi"])
        conn.setRequestProperty("content-type", "application/json")
        conn.doOutput = true
        conn.connectTimeout = 0
        conn.readTimeout = 1234
        conn.outputStream.write("{\"ok\":true}".toByteArray())
        assertEquals(200, conn.responseCode)
        assertEquals("POST", seenMethod)
        assertEquals("application/json", seenContentType)
        assertEquals("{\"ok\":true}", seenBody)
        assertEquals(30_000, seenConnectMs)
        assertEquals(1234, seenReadMs)
        assertFailsWith<IllegalStateException> { conn.setRequestProperty("X-Late", "1") }
        assertFailsWith<IllegalStateException> { conn.addRequestProperty("X-Late", "1") }
        assertFailsWith<IllegalStateException> { conn.outputStream }
    }

    @Test
    fun `explicit PUT attaches empty body and honors 1ms timeout`() {
        var seenMethod = ""
        var seenBody = "unset"
        var seenConnectMs = -1
        val client = stubClient { chain ->
            val req = chain.request()
            seenMethod = req.method
            seenBody = req.body?.let { b -> Buffer().also { b.writeTo(it) }.readUtf8() }.orEmpty()
            seenConnectMs = chain.connectTimeoutMillis()
            okResponse(req)
        }
        val put = SafeHttpsURLConnection(url("https://example.test/put"), client)
        put.requestMethod = "PUT"
        put.connectTimeout = 1
        assertEquals(200, put.responseCode)
        assertEquals("PUT", seenMethod)
        assertEquals("", seenBody)
        assertEquals(1, seenConnectMs)
    }

    @Test
    fun `BoundedBodyOutputStream enforces exact 16 MiB cap on single-byte and array writes`() {
        val maxBytes = 16 * 1024 * 1024
        val conn = SafeHttpURLConnection(url("http://example.test/big"), stubClient { okResponse(it.request()) })
        conn.doOutput = true
        val out = conn.outputStream
        assertFailsWith<IOException> { out.write(ByteArray(4), 0, -1) }
        out.write(ByteArray(0), 0, 0)
        val chunk = ByteArray(1024 * 1024)
        repeat(15) { out.write(chunk) }
        // Fill to maxBytes - 1, then land on exact maxBytes via single-byte write(Int).
        out.write(ByteArray(maxBytes - 15 * chunk.size - 1))
        out.write('Z'.code)
        // One more byte via either overload must fail.
        assertFailsWith<IOException> { out.write('B'.code) }
        assertFailsWith<IOException> { out.write(byteArrayOf(1), 0, 1) }
    }

    @Test
    fun `disconnect before and during in-flight request fails cleanly`() {
        val pre = SafeHttpURLConnection(url("http://example.test/pre"), stubClient { okResponse(it.request()) })
        pre.disconnect()
        assertFailsWith<IOException> { pre.connect() }

        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        val client = stubClient { chain ->
            entered.countDown()
            release.await(5, TimeUnit.SECONDS)
            okResponse(chain.request())
        }
        val conn = SafeHttpsURLConnection(url("https://example.test/inflight"), client)
        val pool = Executors.newSingleThreadExecutor()
        try {
            val future = pool.submit<Throwable?> {
                try {
                    conn.connect()
                    null
                } catch (t: Throwable) {
                    t
                }
            }
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            conn.disconnect()
            release.countDown()
            val thrown = future.get(5, TimeUnit.SECONDS)
            assertTrue(thrown is IOException, "Expected IOException, got $thrown")
        } finally {
            pool.shutdownNow()
        }
    }

    /** Minimal X509Certificate test double providing only the X500Principal read by OkHttp Handshake. */
    private class FakeX509Certificate(dn: String) : X509Certificate() {
        private val principal = X500Principal(dn)
        override fun getSubjectX500Principal(): X500Principal = principal
        override fun getEncoded(): ByteArray = ByteArray(0)
        override fun verify(key: PublicKey?) {}
        override fun verify(key: PublicKey?, sigProvider: String?) {}
        override fun toString(): String = principal.name
        override fun getPublicKey(): PublicKey = throw UnsupportedOperationException()
        override fun hasUnsupportedCriticalExtension(): Boolean = false
        override fun getCriticalExtensionOIDs(): MutableSet<String> = mutableSetOf()
        override fun getNonCriticalExtensionOIDs(): MutableSet<String> = mutableSetOf()
        override fun getExtensionValue(oid: String?): ByteArray? = null
        override fun checkValidity() {}
        override fun checkValidity(date: Date?) {}
        override fun getVersion(): Int = 3
        override fun getSerialNumber(): BigInteger = BigInteger.ONE
        override fun getIssuerDN(): Principal = principal
        override fun getSubjectDN(): Principal = principal
        override fun getNotBefore(): Date = Date(0)
        override fun getNotAfter(): Date = Date(Long.MAX_VALUE)
        override fun getTBSCertificate(): ByteArray = ByteArray(0)
        override fun getSignature(): ByteArray = ByteArray(0)
        override fun getSigAlgName(): String = "SHA256withRSA"
        override fun getSigAlgOID(): String = "1.2.840.113549.1.1.11"
        override fun getSigAlgParams(): ByteArray? = null
        override fun getIssuerUniqueID(): BooleanArray? = null
        override fun getSubjectUniqueID(): BooleanArray? = null
        override fun getKeyUsage(): BooleanArray? = null
        override fun getBasicConstraints(): Int = -1
    }
}
