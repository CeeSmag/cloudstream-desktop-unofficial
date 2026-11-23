package apiscanner

import org.objectweb.asm.ClassReader
import org.objectweb.asm.Handle
import org.objectweb.asm.Type
import org.objectweb.asm.tree.ClassNode
import org.objectweb.asm.tree.FieldInsnNode
import org.objectweb.asm.tree.InvokeDynamicInsnNode
import org.objectweb.asm.tree.LdcInsnNode
import org.objectweb.asm.tree.MethodInsnNode
import org.objectweb.asm.tree.TypeInsnNode
import java.io.File
import java.util.zip.ZipFile

/** What a reference points at. */
enum class RefKind { METHOD, FIELD, CLASS }

/**
 * One member or class a plugin uses, by JVM internal names (`android/content/res/Resources`,
 * `getAssets`, `()Landroid/content/res/AssetManager;`). [name] and [descriptor] are empty for [RefKind.CLASS].
 */
data class Reference(val kind: RefKind, val owner: String, val name: String, val descriptor: String) {
    /** Readable form: `android.content.res.Resources.getAssets()Landroid/content/res/AssetManager;`. */
    fun display(): String {
        val dotted = owner.replace('/', '.')
        return when (kind) {
            RefKind.CLASS -> dotted
            RefKind.FIELD -> "$dotted.$name : $descriptor"
            RefKind.METHOD -> "$dotted.$name$descriptor"
        }
    }
}

/** Owners the runtime provides in place of Android and CloudStream: what the scan is about. */
val SCANNED_PREFIXES = listOf("android/", "androidx/", "com/google/android/", "dalvik/", "com/lagradost/")

fun isScannedOwner(owner: String): Boolean = SCANNED_PREFIXES.any { owner.startsWith(it) }

/**
 * Every reference of a converted plugin JAR to a [SCANNED_PREFIXES] class, with how many times it
 * occurs. A member named on one of the plugin's own classes counts when the plugin does not declare
 * it: the JVM looks it up in the superclasses, so it is reported on the first one outside the plugin
 * (a plugin activity calling `getString` uses `android.content.Context.getString`).
 */
object ReferenceCollector {

    fun collect(jar: File): Map<Reference, Int> {
        val classes = readClasses(jar)
        val counts = HashMap<Reference, Int>()
        val owners = OwnerLookup(classes)
        for (node in classes.values) {
            for (method in node.methods) {
                for (insn in method.instructions) {
                    referencesOf(insn).forEach { ref ->
                        owners.resolve(ref)?.let { counts.merge(it, 1, Int::plus) }
                    }
                }
            }
        }
        return counts
    }

    private fun readClasses(jar: File): Map<String, ClassNode> = ZipFile(jar).use { zip ->
        zip.entries().asSequence()
            .filter { it.name.endsWith(".class") }
            .map { entry ->
                val node = ClassNode()
                zip.getInputStream(entry).use { ClassReader(it).accept(node, ClassReader.SKIP_FRAMES) }
                node
            }
            .associateBy { it.name }
    }

    private fun referencesOf(insn: Any): List<Reference> = when (insn) {
        is MethodInsnNode -> listOf(Reference(RefKind.METHOD, insn.owner, insn.name, insn.desc))
        is FieldInsnNode -> listOf(Reference(RefKind.FIELD, insn.owner, insn.name, insn.desc))
        is TypeInsnNode -> classRef(Type.getObjectType(insn.desc))
        is LdcInsnNode -> constantRefs(insn.cst)
        is InvokeDynamicInsnNode -> insn.bsmArgs.flatMap(::constantRefs) + handleRef(insn.bsm)
        else -> emptyList()
    }

    private fun constantRefs(constant: Any?): List<Reference> = when (constant) {
        is Handle -> handleRef(constant)
        is Type -> if (constant.sort == Type.OBJECT || constant.sort == Type.ARRAY) classRef(constant) else emptyList()
        else -> emptyList()
    }

    private fun handleRef(handle: Handle): List<Reference> {
        val isField = handle.tag <= org.objectweb.asm.Opcodes.H_PUTSTATIC
        val kind = if (isField) RefKind.FIELD else RefKind.METHOD
        return listOf(Reference(kind, handle.owner, handle.name, handle.desc))
    }

    private fun classRef(type: Type): List<Reference> {
        val element = if (type.sort == Type.ARRAY) type.elementType else type
        return if (element.sort == Type.OBJECT) listOf(Reference(RefKind.CLASS, element.internalName, "", "")) else emptyList()
    }
}

/** Maps a reference on a plugin class to the scanned class it really lands on, if any. */
private class OwnerLookup(private val classes: Map<String, ClassNode>) {

    fun resolve(ref: Reference): Reference? {
        val plugin = classes[ref.owner] ?: return ref.takeIf { isScannedOwner(it.owner) }
        if (ref.kind == RefKind.CLASS) return null
        var current: ClassNode? = plugin
        while (current != null) {
            if (declares(current, ref)) return null
            val superName = current.superName ?: return null
            current = classes[superName] ?: return ref.copy(owner = superName).takeIf { isScannedOwner(superName) }
        }
        return null
    }

    private fun declares(node: ClassNode, ref: Reference): Boolean = when (ref.kind) {
        RefKind.METHOD -> node.methods.any { it.name == ref.name && it.desc == ref.descriptor }
        RefKind.FIELD -> node.fields.any { it.name == ref.name && it.desc == ref.descriptor }
        RefKind.CLASS -> true
    }
}
