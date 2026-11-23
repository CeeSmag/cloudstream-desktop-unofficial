package com.lagradost.runtime.testsupport

import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassWriter
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import org.objectweb.asm.tree.ClassNode
import org.objectweb.asm.tree.MethodInsnNode
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import java.util.zip.ZipOutputStream

/**
 * Builds plugin-like classes with ASM and packs them into JARs, so sandbox tests do not need
 * real `.cs3` fixtures. Every method is `public static`; frames and maxs are computed.
 */
class SyntheticClass(val internalName: String) {
    private val writer = ClassWriter(ClassWriter.COMPUTE_FRAMES or ClassWriter.COMPUTE_MAXS)

    init {
        writer.visit(Opcodes.V11, Opcodes.ACC_PUBLIC, internalName, null, "java/lang/Object", null)
    }

    fun field(name: String, descriptor: String, access: Int = Opcodes.ACC_PRIVATE): SyntheticClass {
        writer.visitField(access, name, descriptor, null, null).visitEnd()
        return this
    }

    fun method(
        name: String,
        descriptor: String,
        access: Int = Opcodes.ACC_PUBLIC or Opcodes.ACC_STATIC,
        body: MethodVisitor.() -> Unit,
    ): SyntheticClass {
        val mv = writer.visitMethod(access, name, descriptor, null, null)
        mv.visitCode()
        mv.body()
        mv.visitMaxs(0, 0)
        mv.visitEnd()
        return this
    }

    fun bytes(): ByteArray {
        writer.visitEnd()
        return writer.toByteArray()
    }
}

object SyntheticJar {
    fun write(dir: File, name: String, vararg classes: SyntheticClass): File {
        val jar = File(dir, "$name.jar")
        ZipOutputStream(jar.outputStream()).use { zip ->
            for (cls in classes) {
                zip.putNextEntry(ZipEntry("${cls.internalName}.class"))
                zip.write(cls.bytes())
                zip.closeEntry()
            }
        }
        return jar
    }

    /** Every method call in [jar] as `owner.name desc`, for asserting on rewrites. */
    fun calls(jar: File): List<String> = ZipFile(jar).use { zip ->
        zip.entries().asSequence().filter { it.name.endsWith(".class") }.flatMap { entry ->
            val node = ClassNode()
            zip.getInputStream(entry).use { ClassReader(it).accept(node, 0) }
            node.methods.flatMap { m ->
                m.instructions.filterIsInstance<MethodInsnNode>().map { "${it.owner}.${it.name} ${it.desc}" }
            }
        }.toList()
    }
}
