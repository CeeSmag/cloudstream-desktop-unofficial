package com.lagradost.runtime.security

import com.lagradost.runtime.loader.PluginBytecodeTransformer
import com.lagradost.runtime.testsupport.SyntheticClass
import com.lagradost.runtime.testsupport.SyntheticJar
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.objectweb.asm.Handle
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import org.objectweb.asm.Type
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

/** [PluginSecurityVerifier] on guarded owners, and its all-violations report. */
class GuardedMemberVerifierTest {

    @TempDir
    lateinit var tempDir: File

    private val inet = "java/net/InetAddress"
    private var counter = 0

    private fun jarOf(vararg classes: SyntheticClass): File = SyntheticJar.write(tempDir, "p${counter++}", *classes)

    private fun single(body: MethodVisitor.() -> Unit): File =
        jarOf(SyntheticClass("test/Probe").method("run", "()V") { body(); visitInsn(Opcodes.RETURN) })

    private fun verify(jar: File) = PluginSecurityVerifier.verifyJar(jar, "test-plugin", isTrusted = false)

    private fun refusal(jar: File): String =
        assertFailsWith<SecurityException> { verify(jar) }.message.orEmpty()

    @Test
    fun `allowed members pass`() {
        verify(
            single {
                visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getLoopbackAddress", "()L$inet;", false)
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, inet, "getAddress", "()[B", false)
                visitInsn(Opcodes.POP)
                visitFieldInsn(Opcodes.GETSTATIC, "java/net/Proxy", "NO_PROXY", "Ljava/net/Proxy;")
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/Proxy", "type", "()Ljava/net/Proxy\$Type;", false)
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/Proxy\$Type", "ordinal", "()I", false)
                visitInsn(Opcodes.POP)
                visitLdcInsn(Type.getObjectType(inet))
                visitInsn(Opcodes.POP)
            },
        )
    }

    @Test
    fun `direct calls to redirect members are refused`() {
        val message = refusal(
            single {
                visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getLoopbackAddress", "()L$inet;", false)
                visitIntInsn(Opcodes.BIPUSH, 10)
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, inet, "isReachable", "(I)Z", false)
                visitInsn(Opcodes.POP)
            },
        )
        assertTrue("Disallowed member 'java.net.InetAddress.isReachable(I)Z'" in message, message)
    }

    @Test
    fun `unlisted members are refused`() {
        val message = refusal(
            single {
                visitInsn(Opcodes.ACONST_NULL)
                visitMethodInsn(
                    Opcodes.INVOKEVIRTUAL,
                    "java/net/Inet6Address",
                    "getScopedInterface",
                    "()Ljava/net/NetworkInterface;",
                    false,
                )
                visitInsn(Opcodes.POP)
            },
        )
        assertTrue("java.net.Inet6Address.getScopedInterface" in message, message)
    }

    @Test
    fun `proxies cannot be built and their address is out of reach`() {
        val message = refusal(
            single {
                visitTypeInsn(Opcodes.NEW, "java/net/Proxy")
                visitInsn(Opcodes.DUP)
                visitInsn(Opcodes.ACONST_NULL)
                visitInsn(Opcodes.ACONST_NULL)
                visitMethodInsn(
                    Opcodes.INVOKESPECIAL,
                    "java/net/Proxy",
                    "<init>",
                    "(Ljava/net/Proxy\$Type;Ljava/net/SocketAddress;)V",
                    false,
                )
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/Proxy", "address", "()Ljava/net/SocketAddress;", false)
                visitInsn(Opcodes.POP)
            },
        )
        assertTrue("java.net.Proxy.<init>" in message, message)
        assertTrue("java.net.Proxy.address" in message, message)
    }

    @Test
    fun `method handles to redirect members are refused, to allowed members accepted`() {
        val getByName = Handle(Opcodes.H_INVOKESTATIC, inet, "getByName", "(Ljava/lang/String;)L$inet;", false)
        val getLocalHost = Handle(Opcodes.H_INVOKESTATIC, inet, "getLocalHost", "()L$inet;", false)
        val getAddress = Handle(Opcodes.H_INVOKEVIRTUAL, inet, "getAddress", "()[B", false)
        val metafactory = Handle(
            Opcodes.H_INVOKESTATIC,
            "java/lang/invoke/LambdaMetafactory",
            "metafactory",
            "(Ljava/lang/invoke/MethodHandles\$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;" +
                "Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodType;)" +
                "Ljava/lang/invoke/CallSite;",
            false,
        )
        fun lambda(target: Handle) = single {
            visitInvokeDynamicInsn(
                "get",
                "()Ljava/util/function/Supplier;",
                metafactory,
                Type.getType("()Ljava/lang/Object;"),
                target,
                Type.getType(target.desc),
            )
            visitInsn(Opcodes.POP)
        }

        val viaLdc = refusal(single { visitLdcInsn(getByName); visitInsn(Opcodes.POP) })
        assertTrue("Disallowed MethodHandle 'java.net.InetAddress.getByName" in viaLdc, viaLdc)
        val viaLambda = refusal(lambda(getLocalHost))
        assertTrue("java.net.InetAddress.getLocalHost" in viaLambda, viaLambda)
        verify(single { visitLdcInsn(getAddress); visitInsn(Opcodes.POP) })
    }

    @Test
    fun `neighbouring network classes stay denied`() {
        val message = refusal(
            single {
                visitMethodInsn(
                    Opcodes.INVOKESTATIC,
                    "java/net/NetworkInterface",
                    "getNetworkInterfaces",
                    "()Ljava/util/Enumeration;",
                    false,
                )
                visitInsn(Opcodes.POP)
                visitInsn(Opcodes.ACONST_NULL)
                visitInsn(Opcodes.ICONST_0)
                visitMethodInsn(
                    Opcodes.INVOKESTATIC,
                    "java/net/InetSocketAddress",
                    "createUnresolved",
                    "(Ljava/lang/String;I)Ljava/net/InetSocketAddress;",
                    false,
                )
                visitInsn(Opcodes.POP)
            },
        )
        assertTrue("Disallowed class 'java.net.NetworkInterface'" in message, message)
        assertTrue("Disallowed class 'java.net.InetSocketAddress'" in message, message)
    }

    @Test
    fun `every violation of a jar is reported in one message`() {
        val first = SyntheticClass("test/A").method("run", "()V") {
            visitLdcInsn("HOME")
            visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/System", "getenv", "(Ljava/lang/String;)Ljava/lang/String;", false)
            visitInsn(Opcodes.POP)
            visitInsn(Opcodes.RETURN)
        }
        val second = SyntheticClass("test/B").method("run", "()V") {
            visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getLocalHost", "()L$inet;", false)
            visitInsn(Opcodes.POP)
            visitInsn(Opcodes.ACONST_NULL)
            visitInsn(Opcodes.ACONST_NULL)
            visitMethodInsn(Opcodes.INVOKESTATIC, "java/nio/file/Paths", "get", "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;", false)
            visitInsn(Opcodes.POP)
            visitInsn(Opcodes.RETURN)
        }
        val message = refusal(jarOf(first, second))
        assertTrue("System.getenv()" in message, message)
        assertTrue("java.net.InetAddress.getLocalHost" in message, message)
        assertTrue("java.nio.file.Paths" in message, message)
    }

    @Test
    fun `a long report is capped and repeated sites are listed once`() {
        val cls = SyntheticClass("test/Many")
        for (i in 0 until PluginSecurityVerifier.MAX_REPORTED_VIOLATIONS + 5) {
            cls.method("m$i", "()V") {
                repeat(2) { // the same site twice in one method counts once
                    visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getLocalHost", "()L$inet;", false)
                    visitInsn(Opcodes.POP)
                }
                visitInsn(Opcodes.RETURN)
            }
        }
        val message = refusal(jarOf(cls))
        assertTrue(message.endsWith("(and 5 more)"), message)
        assertEquals(PluginSecurityVerifier.MAX_REPORTED_VIOLATIONS, message.split("; ").size - 1)
    }

    @Test
    fun `a permission is asked for only when nothing else is wrong`() {
        val socketOnly = single {
            visitInsn(Opcodes.ACONST_NULL)
            visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/Socket", "close", "()V", false)
        }
        assertFailsWith<RequiresPermissionException> { verify(socketOnly) }

        val socketAndProbe = single {
            visitInsn(Opcodes.ACONST_NULL)
            visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/Socket", "close", "()V", false)
            visitInsn(Opcodes.ACONST_NULL)
            visitIntInsn(Opcodes.BIPUSH, 1)
            visitMethodInsn(Opcodes.INVOKEVIRTUAL, inet, "isReachable", "(I)Z", false)
            visitInsn(Opcodes.POP)
        }
        val error = assertFailsWith<SecurityException> { verify(socketAndProbe) }
        assertFalse(error is RequiresPermissionException, "must not ask for a permission of a plugin that cannot load")
        assertIs<SecurityException>(error)
    }

    @Test
    fun `trusted plugins skip the scan, as before`() {
        val jar = single {
            visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getLocalHost", "()L$inet;", false)
            visitInsn(Opcodes.POP)
        }
        PluginSecurityVerifier.verifyJar(jar, "trusted-plugin", isTrusted = true)
    }

    /**
     * The real plugin, when its converted JAR is at hand: `CS_TEST_ECLIPSIA_JAR=<path to Eclipsia-jvm.jar>`.
     * Works on a copy, so the app's cache is untouched.
     */
    @Test
    fun `real eclipsia jar passes after the transform`() {
        val source = System.getenv("CS_TEST_ECLIPSIA_JAR")?.let(::File)?.takeIf { it.isFile } ?: return
        val copy = File(tempDir, "Eclipsia-jvm.jar")
        source.copyTo(copy)
        PluginBytecodeTransformer.transform(copy)
        PluginSecurityVerifier.verifyJar(copy, "Eclipsia", isTrusted = false)
    }
}
