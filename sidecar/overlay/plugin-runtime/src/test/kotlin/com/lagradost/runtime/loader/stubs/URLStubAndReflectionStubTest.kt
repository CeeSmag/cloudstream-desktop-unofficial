package com.lagradost.runtime.loader.stubs

import com.lagradost.runtime.loader.CompatPluginClassLoader
import com.lagradost.runtime.loader.SafePluginClassLoader
import com.lagradost.runtime.testsupport.SyntheticClass
import com.lagradost.runtime.testsupport.SyntheticJar
import okhttp3.OkHttpClient
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.objectweb.asm.Opcodes
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.net.InetAddress
import java.net.Proxy
import java.net.ProxySelector
import java.net.SocketAddress
import java.net.URI
import java.net.URL
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** Focused security boundary tests for [URLStub] and [ReflectionStub]. */
class URLStubAndReflectionStubTest {

    @TempDir
    lateinit var tempDir: File

    private fun url(raw: String): URL = URI(raw).toURL()

    @Test
    fun `URLStub validates URLs URIs schemes and hostnames directly`() {
        URLStub.validateUrl(url("https://1.1.1.1/ok"))
        URLStub.validateUri(URI("https://1.1.1.1/ok"))
        URLStub.validateUri(URI("/relative/path"))
        assertEquals("example.com", URLStub.validateHostName("  [EXAMPLE.COM]  "))

        for (bad in listOf(url("file:///etc/passwd"), url("ftp://1.1.1.1/a"), url("http:///nohost"))) {
            assertFailsWith<SecurityException>("Expected $bad blocked") { URLStub.validateUrl(bad) }
        }
        for (bad in listOf(URI("file:///etc/passwd"), URI("ftp://1.1.1.1/a"), URI("http://127.0.0.1/x"), URI("//10.0.0.1:8080/x"))) {
            assertFailsWith<SecurityException>("Expected $bad blocked") { URLStub.validateUri(bad) }
        }
        for (bad in listOf("", "localhost", "api.localhost", "printer.local")) {
            assertFailsWith<SecurityException>("Expected $bad blocked") { URLStub.validateHostName(bad) }
        }
        assertFailsWith<SecurityException> { URLStub.openStream(url("http://127.0.0.1/a")) }
        assertFailsWith<SecurityException> { URLStub.getContent(url("http://127.0.0.1/a")) }
        assertFailsWith<SecurityException> { URLStub.getContent(url("http://127.0.0.1/a"), arrayOf(InputStream::class.java)) }
    }

    @Test
    fun `URLStub isPrivateOrLocalAddress enforces RFC1918 CGNAT zero-net link-local and IPv6 ULA boundaries`() {
        val blocked = listOf(
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "100.127.255.254",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.1",
            "::1",
            "fc00::1",
            "fd12:3456::1",
        )
        for (ip in blocked) {
            assertTrue(URLStub.isPrivateOrLocalAddress(InetAddress.getByName(ip)), "Expected $ip blocked")
        }
        val publicIps = listOf(
            "1.1.1.1",
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.1",
            "140.82.121.4",
            "203.0.113.1",
            "2606:4700:4700::1111",
            "fbff::1",
            "fe00::1",
        )
        for (ip in publicIps) {
            assertFalse(URLStub.isPrivateOrLocalAddress(InetAddress.getByName(ip)), "Expected $ip public")
        }
    }

    @Test
    fun `buildSafeOkHttpClient is idempotent and installs SafeDns`() {
        val builder = OkHttpClient.Builder()
        val first = URLStub.buildSafeOkHttpClient(builder)
        val second = URLStub.buildSafeOkHttpClient(builder)
        assertEquals(2, first.interceptors.size)
        assertEquals(2, first.networkInterceptors.size)
        assertEquals(2, second.interceptors.size)
        assertEquals(2, second.networkInterceptors.size)
        assertEquals(listOf(InetAddress.getByName("1.1.1.1")), second.dns.lookup("1.1.1.1"))
        assertFailsWith<SecurityException> { second.dns.lookup("127.0.0.1") }
    }

    @Test
    fun `installNetworkGuard validates ProxySelector URIs delegates connectFailed and restores cleanly`() {
        val original = ProxySelector.getDefault()
        var failedCount = 0
        val delegate = object : ProxySelector() {
            override fun select(uri: URI?): List<Proxy> = listOf(Proxy.NO_PROXY)
            override fun connectFailed(uri: URI?, sa: SocketAddress?, ioe: IOException?) {
                failedCount++
            }
        }
        try {
            ProxySelector.setDefault(delegate)
            URLStub.installNetworkGuard()
            val guard = ProxySelector.getDefault()
            assertEquals(listOf(Proxy.NO_PROXY), guard.select(URI("https://1.1.1.1/ok")))
            assertEquals(listOf(Proxy.NO_PROXY), guard.select(null))
            assertFailsWith<SecurityException> { guard.select(URI("http://127.0.0.1/secret")) }
            guard.connectFailed(URI("https://1.1.1.1/ok"), null, IOException("boom"))
            assertEquals(1, failedCount)
            // Calling installNetworkGuard a second time is a no-op.
            URLStub.installNetworkGuard()
            assertEquals(guard, ProxySelector.getDefault())
        } finally {
            ProxySelector.setDefault(original)
        }
    }

    @Test
    fun `ReflectionStub enforces allowlist across primitives arrays plugin classes and default deny`() {
        assertTrue(ReflectionStub.isReflectionAllowed(Int::class.javaPrimitiveType!!))
        assertTrue(ReflectionStub.isReflectionAllowed(Array<String>::class.java))
        assertFalse(ReflectionStub.isReflectionAllowed(Array<ProcessBuilder>::class.java))
        assertTrue(ReflectionStub.isReflectionAllowed(String::class.java, "length"))
        assertTrue(ReflectionStub.isReflectionAllowed(URL::class.java, "getHost"))
        assertFalse(ReflectionStub.isReflectionAllowed(URL::class.java, "getContent"))
        assertTrue(ReflectionStub.isReflectionAllowed(ArrayList::class.java, "size"))
        // Default deny for non-whitelisted JDK classes outside EXPLICIT_DENY.
        assertFalse(ReflectionStub.isReflectionAllowed(java.awt.Point::class.java))
        assertFalse(ReflectionStub.isReflectionAllowed(javax.xml.XMLConstants::class.java))

        // Classes loaded by CompatPluginClassLoader are allowed.
        val jar = SyntheticJar.write(
            tempDir,
            "plugin-refl",
            SyntheticClass("p/PluginModel").method("<init>", "()V", access = Opcodes.ACC_PUBLIC) {
                visitVarInsn(Opcodes.ALOAD, 0)
                visitMethodInsn(Opcodes.INVOKESPECIAL, "java/lang/Object", "<init>", "()V", false)
                visitInsn(Opcodes.RETURN)
            },
        )
        val safeParent = SafePluginClassLoader(this::class.java.classLoader)
        CompatPluginClassLoader(arrayOf(jar.toURI().toURL()), safeParent).use { loader ->
            val pluginCls = loader.loadClass("p.PluginModel")
            assertTrue(ReflectionStub.isReflectionAllowed(pluginCls))
        }
    }

    @Test
    fun `ReflectionStub invoke get set newInstance and setAccessible guard both allowed and blocked targets`() {
        val lengthMethod = String::class.java.getMethod("length")
        assertEquals(5, ReflectionStub.invoke(lengthMethod, "hello", null))
        ReflectionStub.setAccessible(lengthMethod, true)

        val openConn = URL::class.java.getMethod("openConnection")
        assertFailsWith<SecurityException> { ReflectionStub.invoke(openConn, url("https://1.1.1.1/"), null) }
        assertFailsWith<SecurityException> { ReflectionStub.setAccessible(openConn, true) }

        val jar = SyntheticJar.write(
            tempDir,
            "plugin-field",
            SyntheticClass("p/Box")
                .field("secret", "Ljava/lang/String;")
                .method("<init>", "()V", access = Opcodes.ACC_PUBLIC) {
                    visitVarInsn(Opcodes.ALOAD, 0)
                    visitMethodInsn(Opcodes.INVOKESPECIAL, "java/lang/Object", "<init>", "()V", false)
                    visitInsn(Opcodes.RETURN)
                },
        )
        val safeParent = SafePluginClassLoader(this::class.java.classLoader)
        CompatPluginClassLoader(arrayOf(jar.toURI().toURL()), safeParent).use { loader ->
            val boxCls = loader.loadClass("p.Box")
            val ctor = boxCls.getDeclaredConstructor()
            ReflectionStub.setAccessible(ctor, true)
            val box = ReflectionStub.newInstance(ctor, null)
            val secretField = boxCls.getDeclaredField("secret")
            ReflectionStub.setAccessible(secretField, true)
            ReflectionStub.set(secretField, box, "after")
            assertEquals("after", ReflectionStub.get(secretField, box))
        }

        val blockedField = System::class.java.getDeclaredField("out")
        assertFailsWith<SecurityException> { ReflectionStub.get(blockedField, null) }
        assertFailsWith<SecurityException> { ReflectionStub.set(blockedField, null, null) }
        assertFailsWith<SecurityException> { ReflectionStub.setAccessible(blockedField, true) }

        val sbCtor = StringBuilder::class.java.getConstructor(String::class.java)
        assertEquals("hi", ReflectionStub.newInstance(sbCtor, arrayOf("hi")).toString())
        val pbCtor = ProcessBuilder::class.java.getConstructor(Array<String>::class.java)
        assertFailsWith<SecurityException> { ReflectionStub.newInstance(pbCtor, arrayOf(arrayOf("id"))) }
        assertFailsWith<SecurityException> { ReflectionStub.setAccessible(pbCtor, true) }
    }
}
