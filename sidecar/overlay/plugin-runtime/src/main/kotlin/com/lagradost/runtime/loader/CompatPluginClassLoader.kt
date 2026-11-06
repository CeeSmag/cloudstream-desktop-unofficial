package com.lagradost.runtime.loader

import com.lagradost.runtime.loader.stubs.ExecutorsStub
import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.ClassWriter
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import java.io.File
import java.net.URL
import java.net.URLClassLoader
import java.util.concurrent.ConcurrentHashMap
import java.util.zip.ZipFile

/**
 * A URLClassLoader that applies backwards-compatibility and sandbox security patches to plugin
 * bytecode at class-load time, so it works regardless of whether the jar is cached or freshly converted.
 */
class CompatPluginClassLoader(urls: Array<URL>, parent: ClassLoader) : URLClassLoader(urls, parent) {

    private var jarZip: ZipFile? = try {
        urls.firstOrNull()?.let { u ->
            if (u.protocol.equals("file", ignoreCase = true)) {
                ZipFile(File(u.toURI()))
            } else {
                null
            }
        }
    } catch (_: Throwable) {
        null
    }

    override fun close() {
        try {
            jarZip?.close()
        } catch (_: Throwable) {}
        jarZip = null
        super.close()
    }

    companion object {
        init {
            registerAsParallelCapable()
        }
    }

    override fun findClass(name: String): Class<*> {
        synchronized(getClassLoadingLock(name)) {
            val loaded = findLoadedClass(name)
            if (loaded != null) return loaded

            return try {
                val bytes = readClassBytes(name)
                val patched = applyCompatPatches(bytes)
                defineClass(name, patched, 0, patched.size)
            } catch (e: LinkageError) {
                findLoadedClass(name) ?: try {
                    Class.forName(name, false, this)
                } catch (_: Throwable) {
                    throw e
                }
            }
        }
    }

    private fun readClassBytes(name: String): ByteArray {
        val path = name.replace('.', '/') + ".class"
        val zip = jarZip
        val entry = zip?.getEntry(path)
        return if (zip != null && entry != null) {
            zip.getInputStream(entry).use { it.readBytes() }
        } else {
            val url = findResource(path) ?: throw ClassNotFoundException(name)
            val conn = url.openConnection()
            conn.useCaches = false
            conn.getInputStream().use { it.readBytes() }
        }
    }

    private fun applyCompatPatches(bytes: ByteArray): ByteArray {
        return try {
            val reader = ClassReader(bytes)
            val writer = SafeComputeClassWriter(
                reader = null,
                flags = ClassWriter.COMPUTE_FRAMES or ClassWriter.COMPUTE_MAXS,
                classLoader = parent,
            )
            reader.accept(CompatPatchVisitor(writer, parent), ClassReader.SKIP_FRAMES)
            writer.toByteArray()
        } catch (_: Exception) {
            bytes
        }
    }

    private class CompatPatchVisitor(cv: ClassVisitor, private val runtimeLoader: ClassLoader) :
        ClassVisitor(Opcodes.ASM9, cv) {
        override fun visitMethod(
            access: Int,
            name: String,
            descriptor: String?,
            signature: String?,
            exceptions: Array<out String>?,
        ): MethodVisitor {
            val mv = super.visitMethod(access, name, descriptor, signature, exceptions)
            return CompatMethodVisitor(mv, runtimeLoader)
        }
    }

    private class CompatMethodVisitor(mv: MethodVisitor, private val runtimeLoader: ClassLoader) :
        MethodVisitor(Opcodes.ASM9, mv) {

        override fun visitMethodInsn(
            opcode: Int,
            owner: String,
            name: String,
            descriptor: String,
            isInterface: Boolean,
        ) {
            if (tryRewriteSandboxCall(opcode, owner, name, descriptor)) {
                return
            }
            if (owner.startsWith("kotlinx/coroutines/") && looksMangled(name)) {
                val resolvedName = CoroutinesMethodResolver.resolve(owner, name, runtimeLoader)
                if (resolvedName != name) {
                    super.visitMethodInsn(opcode, owner, resolvedName, descriptor, isInterface)
                    return
                }
            }
            super.visitMethodInsn(opcode, owner, name, descriptor, isInterface)
        }

        private fun tryRewriteSandboxCall(
            opcode: Int,
            owner: String,
            name: String,
            descriptor: String,
        ): Boolean {
            if (opcode == Opcodes.INVOKESTATIC) {
                if (owner == "java/util/concurrent/Executors" && ExecutorsStub.hasMethod(name, descriptor)) {
                    super.visitMethodInsn(
                        Opcodes.INVOKESTATIC,
                        "com/lagradost/runtime/loader/stubs/ExecutorsStub",
                        name,
                        descriptor,
                        false,
                    )
                    return true
                }
                if (owner == "java/util/Locale" && name == "getDefault" && descriptor == "()Ljava/util/Locale;") {
                    super.visitMethodInsn(
                        Opcodes.INVOKESTATIC,
                        "com/lagradost/cloudstream3/PrivacySpoofer",
                        "getSpoofedLocale",
                        "()Ljava/util/Locale;",
                        false,
                    )
                    return true
                }
                if (owner == "java/util/TimeZone" && name == "getDefault" && descriptor == "()Ljava/util/TimeZone;") {
                    super.visitMethodInsn(
                        Opcodes.INVOKESTATIC,
                        "com/lagradost/cloudstream3/PrivacySpoofer",
                        "getSpoofedTimeZone",
                        "()Ljava/util/TimeZone;",
                        false,
                    )
                    return true
                }
                if (owner == "java/time/ZoneId" && name == "systemDefault" && descriptor == "()Ljava/time/ZoneId;") {
                    super.visitMethodInsn(
                        Opcodes.INVOKESTATIC,
                        "com/lagradost/cloudstream3/PrivacySpoofer",
                        "getSpoofedZoneId",
                        "()Ljava/time/ZoneId;",
                        false,
                    )
                    return true
                }
            }
            if (opcode == Opcodes.INVOKEVIRTUAL &&
                owner == "okhttp3/OkHttpClient\$Builder" &&
                name == "build" &&
                descriptor == "()Lokhttp3/OkHttpClient;"
            ) {
                super.visitMethodInsn(
                    Opcodes.INVOKESTATIC,
                    "com/lagradost/runtime/loader/stubs/URLStub",
                    "buildSafeOkHttpClient",
                    "(Lokhttp3/OkHttpClient\$Builder;)Lokhttp3/OkHttpClient;",
                    false,
                )
                return true
            }
            return false
        }

        private fun looksMangled(name: String): Boolean {
            val idx = name.lastIndexOf('_')
            if (idx < 1) return false
            val suffix = name.substring(idx + 1)
            return suffix.length >= 6 && suffix.all { it.isLetterOrDigit() }
        }
    }

    private object CoroutinesMethodResolver {
        private val cache = ConcurrentHashMap<String, String>()

        fun resolve(owner: String, mangledName: String, loader: ClassLoader): String {
            val key = "$owner.$mangledName"
            return cache.getOrPut(key) { findCurrentName(owner, mangledName, loader) }
        }

        private fun findCurrentName(owner: String, mangledName: String, loader: ClassLoader): String {
            return try {
                val className = owner.replace('/', '.')
                val clazz = Class.forName(className, false, loader)
                val methods = clazz.declaredMethods
                if (methods.any { it.name == mangledName }) return mangledName
                val baseName = mangledName.substringBeforeLast('_')
                val candidate = methods.firstOrNull { m ->
                    m.name == baseName || m.name.startsWith("${baseName}_")
                }
                candidate?.name ?: mangledName
            } catch (_: Exception) {
                mangledName
            }
        }
    }
}
