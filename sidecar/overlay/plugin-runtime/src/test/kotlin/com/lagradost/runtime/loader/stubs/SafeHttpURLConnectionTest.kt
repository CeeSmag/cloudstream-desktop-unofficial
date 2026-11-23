package com.lagradost.runtime.loader.stubs

import com.lagradost.runtime.loader.PluginBytecodeTransformer
import com.lagradost.runtime.security.PluginSecurityVerifier
import com.lagradost.runtime.testsupport.SyntheticClass
import com.lagradost.runtime.testsupport.SyntheticJar
import com.sun.net.httpserver.HttpServer
import okhttp3.Dns
import okhttp3.OkHttpClient
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.objectweb.asm.Opcodes
import java.io.File
import java.io.FileNotFoundException
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Proxy
import java.net.URL
import java.util.concurrent.Callable
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import javax.net.ssl.HostnameVerifier
import javax.net.ssl.HttpsURLConnection
import javax.net.ssl.SSLSocketFactory
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * Contract, SSRF, bytecode-verification, and concurrency tests for [SafeHttpURLConnection],
 * [SafeHttpsURLConnection], and [URLStub.openConnection].
 */
class SafeHttpURLConnectionTest {

    @TempDir
    lateinit var tempDir: File

    @Test
    fun `URLStub openConnection enforces http-https schemes SSRF checks and rejects custom proxies`() {
        val blocked = listOf(
            "file:///etc/passwd",
            "ftp://example.com/file.bin",
            "http://127.0.0.1:8080/admin",
            "http://localhost/secret",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/router",
            "http://192.168.1.1/",
            "http://[::1]:8080/",
        )
        for (raw in blocked) {
            assertFailsWith<SecurityException>("Expected $raw blocked") {
                URLStub.openConnection(URL(raw))
            }
        }
        val customProxy = Proxy(Proxy.Type.HTTP, InetSocketAddress.createUnresolved("127.0.0.1", 8080))
        assertFailsWith<SecurityException> {
            URLStub.openConnection(URL("https://1.1.1.1/"), customProxy)
        }
        val http = URLStub.openConnection(URL("http://1.1.1.1/"))
        assertIs<SafeHttpURLConnection>(http)
        val https = URLStub.openConnection(URL("https://1.1.1.1/"), Proxy.NO_PROXY)
        assertIs<SafeHttpsURLConnection>(https)
        assertFailsWith<SecurityException> {
            https.setHostnameVerifier(HostnameVerifier { _, _ -> true })
        }
        assertFailsWith<SecurityException> {
            https.setSSLSocketFactory(SSLSocketFactory.getDefault() as SSLSocketFactory)
        }
        // ReflectionStub must not allow bypassing URLStub or GuardedMemberPolicy via reflection.
        assertTrue(!ReflectionStub.isReflectionAllowed(java.net.URLClassLoader::class.java))
        assertTrue(!ReflectionStub.isReflectionAllowed(javax.net.ssl.SSLSocket::class.java))
        assertTrue(!ReflectionStub.isReflectionAllowed(URL::class.java, "openConnection"))
        assertTrue(!ReflectionStub.isReflectionAllowed(URL::class.java, "openStream"))
        assertTrue(!ReflectionStub.isReflectionAllowed(HttpsURLConnection::class.java, "setDefaultSSLSocketFactory"))
    }

    @Test
    fun `SafeHttpURLConnection performs GET POST headers and 404 errorStream against HTTP server`() {
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/ok") { ex ->
            val agent = ex.requestHeaders.getFirst("X-Custom") ?: ""
            val payload = "hello:$agent".toByteArray()
            ex.responseHeaders.add("X-Reply", "yes")
            ex.sendResponseHeaders(200, payload.size.toLong())
            ex.responseBody.use { it.write(payload) }
        }
        server.createContext("/echo") { ex ->
            val body = ex.requestBody.readBytes()
            ex.sendResponseHeaders(201, body.size.toLong())
            ex.responseBody.use { it.write(body) }
        }
        server.createContext("/missing") { ex ->
            val err = "nope".toByteArray()
            ex.sendResponseHeaders(404, err.size.toLong())
            ex.responseBody.use { it.write(err) }
        }
        server.start()
        try {
            val port = server.address.port
            val client = OkHttpClient.Builder().build()

            val get = SafeHttpURLConnection(URL("http://127.0.0.1:$port/ok"), client)
            get.connectTimeout = 5000
            get.readTimeout = 5000
            get.setRequestProperty("X-Custom", "stream")
            assertEquals(200, get.responseCode)
            assertEquals("yes", get.getHeaderField("X-Reply"))
            assertEquals("hello:stream", get.inputStream.bufferedReader().readText())
            get.disconnect()

            val post = SafeHttpURLConnection(URL("http://127.0.0.1:$port/echo"), client)
            post.requestMethod = "POST"
            post.doOutput = true
            post.setRequestProperty("Content-Type", "text/plain")
            post.outputStream.use { it.write("payload".toByteArray()) }
            assertEquals(201, post.responseCode)
            assertEquals("payload", post.inputStream.bufferedReader().readText())
            post.disconnect()

            val missing = SafeHttpURLConnection(URL("http://127.0.0.1:$port/missing"), client)
            assertFailsWith<FileNotFoundException> { missing.inputStream }
            assertEquals(404, missing.responseCode)
            assertEquals("nope", assertNotNull(missing.errorStream).bufferedReader().readText())
            missing.disconnect()
        } finally {
            server.stop(0)
        }
    }

    @Test
    fun `redirect to loopback is blocked by SafeInterceptor on the OkHttp client`() {
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/hop") { ex ->
            ex.responseHeaders.add("Location", "http://127.0.0.1:${server.address.port}/secret")
            ex.sendResponseHeaders(302, -1)
            ex.close()
        }
        server.createContext("/secret") { ex ->
            val data = "TOP_SECRET".toByteArray()
            ex.sendResponseHeaders(200, data.size.toLong())
            ex.responseBody.use { it.write(data) }
        }
        server.start()
        try {
            val port = server.address.port
            // First host is allowed via test DNS to reach our local redirector, while SafeInterceptor
            // (installed by URLStub.buildSafeOkHttpClient) still inspects every hop including redirects.
            val fakePublicClient = URLStub.buildSafeOkHttpClient(
                OkHttpClient.Builder(),
            ).newBuilder().dns(object : Dns {
                override fun lookup(hostname: String): List<InetAddress> =
                    if (hostname == "public.example.test") {
                        listOf(InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)))
                    } else {
                        Dns.SYSTEM.lookup(hostname)
                    }
            }).build()
            val conn = SafeHttpURLConnection(URL("http://public.example.test:$port/hop"), fakePublicClient)
            assertFailsWith<SecurityException>("Redirect to 127.0.0.1 must be blocked") {
                conn.inputStream
            }
        } finally {
            server.stop(0)
        }
    }

    @Test
    fun `transformed URL openConnection and safe HttpURLConnection calls pass verifier while TLS bypasses stay blocked`() {
        val safeJar = SyntheticJar.write(
            tempDir,
            "safe-http",
            SyntheticClass("p/Artwork").method("fetch", "(Ljava/lang/String;)Ljava/io/InputStream;") {
                visitTypeInsn(Opcodes.NEW, "java/net/URL")
                visitInsn(Opcodes.DUP)
                visitVarInsn(Opcodes.ALOAD, 1)
                visitMethodInsn(Opcodes.INVOKESPECIAL, "java/net/URL", "<init>", "(Ljava/lang/String;)V", false)
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/URL", "openConnection", "()Ljava/net/URLConnection;", false)
                visitTypeInsn(Opcodes.CHECKCAST, "java/net/HttpURLConnection")
                visitInsn(Opcodes.DUP)
                visitIntInsn(Opcodes.SIPUSH, 5000)
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/HttpURLConnection", "setConnectTimeout", "(I)V", false)
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/HttpURLConnection", "getInputStream", "()Ljava/io/InputStream;", false)
                visitInsn(Opcodes.ARETURN)
            },
        )
        PluginBytecodeTransformer.transform(safeJar)
        PluginSecurityVerifier.verifyJar(safeJar, "safe-http", isTrusted = false)

        val unsafeJar = SyntheticJar.write(
            tempDir,
            "unsafe-https",
            SyntheticClass("p/Bypass").method("run", "(Ljavax/net/ssl/HttpsURLConnection;)V") {
                visitVarInsn(Opcodes.ALOAD, 1)
                visitInsn(Opcodes.ACONST_NULL)
                visitMethodInsn(
                    Opcodes.INVOKEVIRTUAL,
                    "javax/net/ssl/HttpsURLConnection",
                    "setHostnameVerifier",
                    "(Ljavax/net/ssl/HostnameVerifier;)V",
                    false,
                )
                visitInsn(Opcodes.ICONST_0)
                visitMethodInsn(Opcodes.INVOKESTATIC, "java/net/HttpURLConnection", "setFollowRedirects", "(Z)V", false)
                visitInsn(Opcodes.RETURN)
            },
        )
        PluginBytecodeTransformer.transform(unsafeJar)
        val err = assertFailsWith<SecurityException> {
            PluginSecurityVerifier.verifyJar(unsafeJar, "unsafe-https", isTrusted = false)
        }
        assertTrue("setHostnameVerifier" in err.message.orEmpty(), err.message)
        assertTrue("setFollowRedirects" in err.message.orEmpty(), err.message)
    }

    @Test
    fun `concurrent SafeHttpURLConnection requests and disconnects remain race-free`() {
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/ping") { ex ->
            val body = "pong".toByteArray()
            ex.sendResponseHeaders(200, body.size.toLong())
            ex.responseBody.use { it.write(body) }
        }
        server.start()
        val pool = Executors.newFixedThreadPool(8)
        try {
            val url = URL("http://127.0.0.1:${server.address.port}/ping")
            val client = OkHttpClient.Builder().build()
            val start = CountDownLatch(1)
            val tasks = (0 until 8).map {
                pool.submit(
                    Callable {
                        start.await(5, TimeUnit.SECONDS)
                        for (i in 0 until 25) {
                            val conn = SafeHttpURLConnection(url, client)
                            conn.setRequestProperty("X-Iter", i.toString())
                            assertEquals(200, conn.responseCode)
                            assertEquals("pong", conn.inputStream.bufferedReader().readText())
                            conn.disconnect()
                            conn.disconnect()
                        }
                        true
                    },
                )
            }
            start.countDown()
            for (t in tasks) {
                assertTrue(t.get(20, TimeUnit.SECONDS))
            }
        } finally {
            pool.shutdownNow()
            server.stop(0)
        }
    }
}
