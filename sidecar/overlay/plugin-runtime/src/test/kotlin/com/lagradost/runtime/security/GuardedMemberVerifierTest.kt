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

    @Test
    fun `exact max reported violations has no overflow suffix and cache skips rescan`() {
        val cls = SyntheticClass("test/Exact")
        for (i in 0 until PluginSecurityVerifier.MAX_REPORTED_VIOLATIONS) {
            cls.method("m$i", "()V") {
                visitMethodInsn(Opcodes.INVOKESTATIC, inet, "getLocalHost", "()L$inet;", false)
                visitInsn(Opcodes.POP)
                visitInsn(Opcodes.RETURN)
            }
        }
        val msg = refusal(jarOf(cls))
        assertFalse("(and " in msg, msg)
        assertEquals(PluginSecurityVerifier.MAX_REPORTED_VIOLATIONS, msg.split("; ").size)

        val clean = single {
            visitLdcInsn("benign-constant")
            visitInsn(Opcodes.POP)
        }
        verify(clean)
        // Second call with identical mtime exercises the verifiedJarSignatures cache hit.
        verify(clean)
    }

    @Test
    fun `non-class zip entry cannot spoof pluginJarClasses to bypass foreign owner checks`() {
        val jar = File(tempDir, "spoofed-entry.jar")
        val cls = SyntheticClass("test/Spoof").method("run", "()V") {
            visitTypeInsn(Opcodes.NEW, "java/lang/ProcessBuilder")
            visitInsn(Opcodes.DUP)
            visitInsn(Opcodes.ICONST_0)
            visitTypeInsn(Opcodes.ANEWARRAY, "java/lang/String")
            visitMethodInsn(Opcodes.INVOKESPECIAL, "java/lang/ProcessBuilder", "<init>", "([Ljava/lang/String;)V", false)
            visitInsn(Opcodes.POP)
            visitInsn(Opcodes.RETURN)
        }
        java.util.zip.ZipOutputStream(jar.outputStream()).use { zip ->
            zip.putNextEntry(java.util.zip.ZipEntry("java/lang/ProcessBuilder"))
            zip.write(ByteArray(0))
            zip.closeEntry()
            zip.putNextEntry(java.util.zip.ZipEntry("test/Spoof.class"))
            zip.write(cls.bytes())
            zip.closeEntry()
        }
        val msg = refusal(jar)
        assertTrue("Disallowed class 'java.lang.ProcessBuilder'" in msg, msg)
    }

    @Test
    fun `disallowed field accesses and targeted calls are refused`() {
        val fieldMsg = refusal(
            single {
                visitFieldInsn(Opcodes.GETSTATIC, "java/nio/file/StandardOpenOption", "READ", "Ljava/nio/file/StandardOpenOption;")
                visitInsn(Opcodes.POP)
            },
        )
        assertTrue("java.nio.file.StandardOpenOption" in fieldMsg, fieldMsg)

        val targetedMsg = refusal(
            single {
                visitInsn(Opcodes.ACONST_NULL)
                visitLdcInsn("id")
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/Runtime", "exec", "(Ljava/lang/String;)Ljava/lang/Process;", false)
                visitInsn(Opcodes.POP)
                visitInsn(Opcodes.ACONST_NULL)
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/URL", "openStream", "()Ljava/io/InputStream;", false)
                visitInsn(Opcodes.POP)
                visitInsn(Opcodes.ACONST_NULL)
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/net/URL", "getContent", "()Ljava/lang/Object;", false)
                visitInsn(Opcodes.POP)
                visitLdcInsn("java.lang.String")
                visitInsn(Opcodes.ICONST_1)
                visitInsn(Opcodes.ACONST_NULL)
                visitMethodInsn(
                    Opcodes.INVOKESTATIC,
                    "java/lang/Class",
                    "forName",
                    "(Ljava/lang/String;ZLjava/lang/ClassLoader;)Ljava/lang/Class;",
                    false,
                )
                visitInsn(Opcodes.POP)
            },
        )
        assertTrue("Blocked Runtime.exec()" in targetedMsg, targetedMsg)
        assertTrue("Illegal URL.openStream()" in targetedMsg, targetedMsg)
        assertTrue("Illegal URL.getContent()" in targetedMsg, targetedMsg)
        assertTrue("Illegal ClassLoader injection via Class.forName" in targetedMsg, targetedMsg)
    }

    @Test
    fun `reflection by name and disallowed string or type LDC constants are refused`() {
        val reflMsg = refusal(
            single {
                visitLdcInsn("java.lang.Runtime")
                visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/Class", "forName", "(Ljava/lang/String;)Ljava/lang/Class;", false)
                visitInsn(Opcodes.POP)
                visitInsn(Opcodes.ACONST_NULL)
                visitLdcInsn("SUN.MISC.UNSAFE")
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/ClassLoader", "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", false)
                visitInsn(Opcodes.POP)
                visitLdcInsn("java.lang.ProcessBuilder\$Redirect")
                visitInsn(Opcodes.POP)
                visitLdcInsn("java.lang.Runtime.Sub")
                visitInsn(Opcodes.POP)
                visitLdcInsn(Type.getObjectType("java/lang/ProcessBuilder"))
                visitInsn(Opcodes.POP)
            },
        )
        assertTrue("Disallowed reflective invocation java.lang.Class.forName(\"java.lang.Runtime\")" in reflMsg, reflMsg)
        assertTrue("Disallowed reflective invocation java.lang.ClassLoader.loadClass(\"SUN.MISC.UNSAFE\")" in reflMsg, reflMsg)
        assertTrue("java.lang.ProcessBuilder\$Redirect" in reflMsg, reflMsg)
        assertTrue("java.lang.Runtime.Sub" in reflMsg, reflMsg)
        assertTrue("Disallowed Type 'java.lang.ProcessBuilder' loaded via LDC" in reflMsg, reflMsg)
    }

    @Test
    fun `invokedynamic refuses disallowed bootstrap owners types and non-guarded handles`() {
        val badBsm = Handle(Opcodes.H_INVOKESTATIC, "java/lang/ProcessBuilder", "bsm", "()Ljava/lang/invoke/CallSite;", false)
        val okBsm = Handle(
            Opcodes.H_INVOKESTATIC,
            "java/lang/invoke/LambdaMetafactory",
            "metafactory",
            "(Ljava/lang/invoke/MethodHandles\$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;" +
                "Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodType;)" +
                "Ljava/lang/invoke/CallSite;",
            false,
        )
        val badHandle = Handle(Opcodes.H_NEWINVOKESPECIAL, "java/lang/ProcessBuilder", "<init>", "([Ljava/lang/String;)V", false)
        val msg = refusal(
            single {
                visitInvokeDynamicInsn("bad1", "()V", badBsm)
                visitInvokeDynamicInsn("bad2", "()V", okBsm, Type.getObjectType("java/lang/ProcessBuilder"), badHandle)
            },
        )
        assertTrue("Disallowed invokedynamic bootstrap method owner 'java.lang.ProcessBuilder'" in msg, msg)
        assertTrue("Disallowed Type 'java.lang.ProcessBuilder' in invokedynamic arguments" in msg, msg)
        assertTrue("Disallowed MethodHandle 'java.lang.ProcessBuilder'" in msg, msg)
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
