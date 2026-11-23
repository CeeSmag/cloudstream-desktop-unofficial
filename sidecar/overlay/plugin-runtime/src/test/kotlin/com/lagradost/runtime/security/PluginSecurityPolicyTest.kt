package com.lagradost.runtime.security

import org.junit.jupiter.api.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** Contract tests for [PluginSecurityPolicy] allowlists, trust overrides, and ASM owner rules. */
class PluginSecurityPolicyTest {

    @Test
    fun `safe java lang set includes expected pure types`() {
        assertTrue("java.lang.String" in PluginSecurityPolicy.SAFE_JAVA_LANG_CLASSES)
        assertTrue("java.lang.Character\$UnicodeScript" in PluginSecurityPolicy.SAFE_JAVA_LANG_CLASSES)
        assertTrue("java.lang.InterruptedException" in PluginSecurityPolicy.SAFE_JAVA_LANG_CLASSES)
        assertFalse("java.lang.ProcessBuilder" in PluginSecurityPolicy.SAFE_JAVA_LANG_CLASSES)
    }

    @Test
    fun `trusted plugins unlock Thread and unknown libs but still obey explicit deny and blocked host prefixes`() {
        assertFalse(PluginSecurityPolicy.isClassAllowed("java.lang.Thread", isTrusted = false))
        assertFalse(PluginSecurityPolicy.isClassAllowed("java.lang.ThreadGroup", isTrusted = false))
        assertTrue(PluginSecurityPolicy.isClassAllowed("java.lang.Thread", isTrusted = true))
        assertTrue(PluginSecurityPolicy.isClassAllowed("java.lang.ThreadGroup", isTrusted = true))
        assertTrue(PluginSecurityPolicy.isClassAllowed("com.example.CustomLib", isTrusted = true))
        assertFalse(PluginSecurityPolicy.isClassAllowed("com.example.CustomLib", isTrusted = false))
        assertFalse(PluginSecurityPolicy.isClassAllowed("java.lang.ProcessBuilder", isTrusted = true))
        assertFalse(PluginSecurityPolicy.isClassAllowed("com.lagradost.common.HostInternal", isTrusted = true))
        assertFalse(PluginSecurityPolicy.isClassAllowed("sun.misc.Unsafe", isTrusted = true))
    }

    @Test
    fun `raw socket permission and runtime stubs follow policy`() {
        assertFalse(PluginSecurityPolicy.isClassAllowed("java.net.Socket", hasSocketPermission = false))
        assertTrue(PluginSecurityPolicy.isClassAllowed("java.net.Socket", hasSocketPermission = true))
        assertFalse(PluginSecurityPolicy.isClassAllowed("javax.net.ssl.SSLSocket", hasSocketPermission = false))
        assertTrue(PluginSecurityPolicy.isClassAllowed("javax.net.ssl.SSLSocket", hasSocketPermission = true))
        assertTrue(PluginSecurityPolicy.isClassAllowed("com.lagradost.runtime.loader.stubs.URLStub"))
        assertFalse(PluginSecurityPolicy.isClassAllowed("com.lagradost.runtime.loader.SafePluginClassLoader"))
    }

    @Test
    fun `standard and ecosystem branches allow safe packages and reject dangerous ones`() {
        val allowed = listOf(
            "java.lang.invoke.LambdaMetafactory",
            "java.lang.reflect.Method",
            "java.lang.annotation.Retention",
            "java.lang.ref.WeakReference",
            "java.lang.Character\$UnicodeScript",
            "java.util.ArrayList",
            "java.io.ByteArrayInputStream",
            "java.nio.charset.StandardCharsets",
            "java.nio.ByteBuffer",
            "javax.net.ssl.SSLContext",
            "okhttp3.OkHttpClient",
            "java.net.URI",
        )
        for (name in allowed) {
            assertTrue(PluginSecurityPolicy.isClassAllowed(name), "Expected $name allowed")
        }
        val denied = listOf(
            "java.lang.invoke.MethodHandles\$Lookup",
            "java.lang.reflect.Proxy",
            "java.lang.ProcessHandle",
            "java.util.ServiceLoader",
            "java.util.jar.JarFile",
            "java.io.FileDescriptor",
            "java.nio.channels.SocketChannel",
            "java.sql.DriverManager",
            "org.apache.commons.io.FileUtils",
        )
        for (name in denied) {
            assertFalse(PluginSecurityPolicy.isClassAllowed(name), "Expected $name denied")
        }
    }

    @Test
    fun `isAsmOwnerAllowed and isSystemOrHostPackage enforce host boundaries`() {
        assertTrue(PluginSecurityPolicy.isAsmOwnerAllowed("java/lang/Thread"))
        assertTrue(PluginSecurityPolicy.isAsmOwnerAllowed("java/net/Socket"))
        assertTrue(PluginSecurityPolicy.isAsmOwnerAllowed("[Ljava/lang/String;"))
        assertFalse(PluginSecurityPolicy.isAsmOwnerAllowed("java/lang/ProcessBuilder"))
        assertFalse(PluginSecurityPolicy.isAsmOwnerAllowed("java/net/NetworkInterface"))

        assertTrue(PluginSecurityPolicy.isSystemOrHostPackage("java.lang.String"))
        assertTrue(PluginSecurityPolicy.isSystemOrHostPackage("javax.net.ssl.SSLContext"))
        assertTrue(PluginSecurityPolicy.isSystemOrHostPackage("com.lagradost.runtime.loader.ApiResolver"))
        assertFalse(PluginSecurityPolicy.isSystemOrHostPackage("com.example.provider.MyProvider"))
    }
}
