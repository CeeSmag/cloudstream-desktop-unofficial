package com.lagradost.runtime.loader

import com.lagradost.runtime.security.PluginSecurityVerifier
import com.lagradost.runtime.testsupport.SyntheticClass
import com.lagradost.runtime.testsupport.SyntheticJar
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.objectweb.asm.Opcodes
import java.io.File
import java.lang.reflect.InvocationTargetException
import java.net.InetAddress
import java.net.Proxy
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

/**
 * End to end through the real pipeline: [PluginBytecodeTransformer] -> [PluginSecurityVerifier]
 * -> [CompatPluginClassLoader] over [SafePluginClassLoader] (untrusted), then the plugin code runs.
 */
class GuardedCallRewriteTest {

    @TempDir
    lateinit var tempDir: File

    private val inet = "java/net/InetAddress"
    private val stub = "com/lagradost/runtime/loader/stubs/InetAddressStub"

    /**
     * Shaped like Eclipsia's `AddressPolicy$Cidr` and `NetPolicy`, the plugin that motivated the
     * member policy: CIDR bases from literal `getByName` + `getAddress`, and `Proxy.NO_PROXY`.
     */
    private fun eclipsiaShaped(): SyntheticClass = SyntheticClass("com/example/net/AddressProbe")
        .method("cidrBase", "(Ljava/lang/String;)[B") {
            visitVarInsn(Opcodes.ALOAD, 0)
            visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getByName", "(Ljava/lang/String;)L$inet;", false)
            visitMethodInsn(Opcodes.INVOKEVIRTUAL, inet, "getAddress", "()[B", false)
            visitInsn(Opcodes.ARETURN)
        }
        .method("isLoopback", "(Ljava/lang/String;)Z") {
            visitVarInsn(Opcodes.ALOAD, 0)
            visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getByName", "(Ljava/lang/String;)L$inet;", false)
            visitMethodInsn(Opcodes.INVOKEVIRTUAL, inet, "isLoopbackAddress", "()Z", false)
            visitInsn(Opcodes.IRETURN)
        }
        .method("hostName", "(Ljava/lang/String;)Ljava/lang/String;") {
            // Receiver typed as the subclass: the owner javac emits is Inet4Address.
            visitVarInsn(Opcodes.ALOAD, 0)
            visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getByName", "(Ljava/lang/String;)L$inet;", false)
            visitTypeInsn(Opcodes.CHECKCAST, "java/net/Inet4Address")
            visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/Inet4Address", "getHostName", "()Ljava/lang/String;", false)
            visitInsn(Opcodes.ARETURN)
        }
        .method("probe", "(Ljava/lang/String;)Z") {
            visitVarInsn(Opcodes.ALOAD, 0)
            visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getByName", "(Ljava/lang/String;)L$inet;", false)
            visitIntInsn(Opcodes.BIPUSH, 100)
            visitMethodInsn(Opcodes.INVOKEVIRTUAL, inet, "isReachable", "(I)Z", false)
            visitInsn(Opcodes.IRETURN)
        }
        .method("localHost", "()L$inet;") {
            visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getLocalHost", "()L$inet;", false)
            visitInsn(Opcodes.ARETURN)
        }
        .method("noProxy", "()Ljava/net/Proxy;") {
            visitFieldInsn(Opcodes.GETSTATIC, "java/net/Proxy", "NO_PROXY", "Ljava/net/Proxy;")
            visitInsn(Opcodes.ARETURN)
        }

    /** Loaders hold their JAR open; Windows cannot delete [tempDir] until they are closed. */
    private val loaders = mutableListOf<CompatPluginClassLoader>()

    @AfterEach
    fun closeLoaders() {
        loaders.forEach { it.close() }
    }

    private fun load(jar: File): Class<*> {
        val parent = SafePluginClassLoader(this::class.java.classLoader, isTrusted = false)
        val loader = CompatPluginClassLoader(arrayOf(jar.toURI().toURL()), parent)
        loaders += loader
        return loader.loadClass("com.example.net.AddressProbe")
    }

    private fun Class<*>.call(name: String, vararg args: Any?): Any? =
        methods.single { it.name == name }.invoke(null, *args)

    @Test
    fun `transform rewrites redirect members and keeps allowed ones`() {
        val jar = SyntheticJar.write(tempDir, "probe", eclipsiaShaped())
        PluginBytecodeTransformer.transform(jar)
        val calls = SyntheticJar.calls(jar)

        assertTrue("$stub.getByName (Ljava/lang/String;)L$inet;" in calls, calls.toString())
        assertTrue("$stub.getHostName (L$inet;)Ljava/lang/String;" in calls, calls.toString())
        assertTrue("$stub.isReachable (L$inet;I)Z" in calls, calls.toString())
        assertTrue("$stub.getLocalHost ()L$inet;" in calls, calls.toString())
        assertTrue("$inet.getAddress ()[B" in calls, "allowed members stay direct")
        for (direct in listOf("getByName", "getHostName", "isReachable", "getLocalHost")) {
            assertFalse(calls.any { it.startsWith("java/net/Inet") && it.contains(".$direct ") }, "direct $direct left")
        }
    }

    @Test
    fun `an eclipsia-shaped plugin passes verification untrusted and runs guarded`() {
        val jar = SyntheticJar.write(tempDir, "probe", eclipsiaShaped())
        PluginBytecodeTransformer.transform(jar)
        PluginSecurityVerifier.verifyJar(jar, "probe-plugin", isTrusted = false)

        val probe = load(jar)
        assertContentEquals(byteArrayOf(10, 0, 0, 0), probe.call("cidrBase", "10.0.0.0") as ByteArray)
        assertEquals(16, (probe.call("cidrBase", "fc00::") as ByteArray).size)
        assertEquals(true, probe.call("isLoopback", "127.0.0.1"))
        assertEquals("10.1.2.3", probe.call("hostName", "10.1.2.3"))
        assertTrue((probe.call("localHost") as InetAddress).isLoopbackAddress)
        assertEquals(Proxy.NO_PROXY, probe.call("noProxy"))

        val refused = assertFailsWith<InvocationTargetException> { probe.call("probe", "93.184.216.34") }
        assertIs<SecurityException>(refused.cause)
    }

    @Test
    fun `the untransformed jar fails verification, naming every unguarded member`() {
        val jar = SyntheticJar.write(tempDir, "raw", eclipsiaShaped())
        val error = assertFailsWith<SecurityException> {
            PluginSecurityVerifier.verifyJar(jar, "raw-plugin", isTrusted = false)
        }
        val message = error.message.orEmpty()
        assertTrue("java.net.InetAddress.getByName" in message, message)
        assertTrue("java.net.InetAddress.isReachable(I)Z" in message, message)
        assertTrue("java.net.InetAddress.getLocalHost" in message, message)
        assertTrue("java.net.Inet4Address.getHostName" in message, message)
        assertFalse("getAddress" in message, "allowed member reported: $message")
        assertFalse("NO_PROXY" in message, "allowed field reported: $message")
    }

    @Test
    fun `class loading guards a jar that skipped the transformer`() {
        // Defense in depth: an old cached jar still gets the stub at load time.
        val jar = SyntheticJar.write(tempDir, "cached", eclipsiaShaped())
        val probe = load(jar)
        val refused = assertFailsWith<InvocationTargetException> { probe.call("probe", "127.0.0.1") }
        assertIs<SecurityException>(refused.cause)
        assertTrue((probe.call("localHost") as InetAddress).isLoopbackAddress)
    }

    @Test
    fun `rewriting twice changes nothing`() {
        val jar = SyntheticJar.write(tempDir, "twice", eclipsiaShaped())
        PluginBytecodeTransformer.transform(jar)
        val once = SyntheticJar.calls(jar)
        PluginBytecodeTransformer.transform(jar)
        assertEquals(once, SyntheticJar.calls(jar))
    }
}
