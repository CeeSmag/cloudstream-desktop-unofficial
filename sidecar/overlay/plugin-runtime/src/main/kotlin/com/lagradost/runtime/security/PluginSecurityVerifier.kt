package com.lagradost.runtime.security

import com.lagradost.runtime.permission.PluginPermission
import com.lagradost.runtime.permission.PluginPermissionAPI
import com.lagradost.runtime.security.GuardedMemberPolicy.MemberRef
import com.lagradost.runtime.security.GuardedMemberPolicy.MemberRule
import org.objectweb.asm.ClassReader
import org.objectweb.asm.Handle
import org.objectweb.asm.Type
import org.objectweb.asm.tree.ClassNode
import org.objectweb.asm.tree.FieldInsnNode
import org.objectweb.asm.tree.InvokeDynamicInsnNode
import org.objectweb.asm.tree.LdcInsnNode
import org.objectweb.asm.tree.MethodInsnNode
import org.objectweb.asm.tree.MethodNode
import java.io.File
import java.util.zip.ZipFile

/**
 * Static bytecode gate for untrusted plugins: a default-deny allowlist of the JDK and ecosystem
 * classes plugin code may touch ([PluginSecurityPolicy]), refined per member for partly safe JDK
 * types ([GuardedMemberPolicy]), plus targeted blocks (Runtime/System calls, raw URL connections,
 * ClassLoader injection, reflection by name on system internals).
 *
 * Overlay of the pinned reference file. Differences: member-level checks for guarded owners, and
 * every violation of a JAR is reported in one [SecurityException] instead of only the first. A
 * [RequiresPermissionException] (the caller then asks the viewer for that permission) is thrown only
 * when nothing else is wrong, so nobody is asked for a permission of a plugin that cannot load anyway.
 */
object PluginSecurityVerifier {

    /** How many violations the exception message lists; the rest are counted. */
    const val MAX_REPORTED_VIOLATIONS = 20

    private val verifiedJarSignatures = java.util.concurrent.ConcurrentHashMap<String, Long>()

    private val DISALLOWED_REFLECTION_TARGETS = setOf(
        "java.lang.Runtime",
        "java.lang.System",
        "java.lang.ProcessBuilder",
        "java.lang.Process",
        "java.lang.ClassLoader",
        "java.lang.Compiler",
        "java.lang.SecurityManager",
        "java.lang.invoke.MethodHandles",
        "java.lang.invoke.MethodHandle",
        "java.lang.invoke.VarHandle",
        "java.lang.reflect.Method",
        "java.lang.reflect.Field",
        "java.lang.reflect.Constructor",
        "sun.misc.Unsafe",
        "jdk.internal.misc.Unsafe",
    )

    @Throws(SecurityException::class)
    fun verifyJar(jarFile: File, pluginInternalName: String, isTrusted: Boolean = false) {
        if (isTrusted) {
            // User explicitly trusts this plugin/repository: bypass static verification
            return
        }

        val cacheKey = try {
            jarFile.canonicalPath
        } catch (_: Throwable) {
            jarFile.absolutePath
        }
        val lastMod = jarFile.lastModified()
        if (verifiedJarSignatures[cacheKey] == lastMod) {
            return
        }

        val findings = collectFindings(jarFile, pluginInternalName)
        if (findings.violations.isNotEmpty()) {
            throw PluginSecurityBlockedException(findings.violations, describe(findings.violations))
        }
        findings.permissionNeeded?.let { throw it }
        verifiedJarSignatures[cacheKey] = lastMod
    }

    /** Everything the scan finds in an untrusted JAR, without throwing. */
    class Findings(
        /** Distinct violations in the order found; any one keeps the plugin from loading. */
        val violations: List<String>,
        /** The first permission it needs, asked for only when [violations] is empty. */
        val permissionNeeded: RequiresPermissionException?,
    )

    /** Scans [jarFile] as [verifyJar] does (no cache, no exception) for tools that report. */
    fun collectFindings(jarFile: File, pluginInternalName: String): Findings {
        val scan = ZipFile(jarFile).use { zip -> scanJar(zip, pluginInternalName) }
        return Findings(scan.violations.distinct(), scan.permissionNeeded)
    }

    /** One line per violation, then "(and N more)" past [MAX_REPORTED_VIOLATIONS]. */
    fun describe(violations: List<String>): String {
        val shown = violations.take(MAX_REPORTED_VIOLATIONS).joinToString("; ")
        val hidden = violations.size - MAX_REPORTED_VIOLATIONS
        return if (hidden > 0) "$shown; (and $hidden more)" else shown
    }

    private fun scanJar(zip: ZipFile, pluginInternalName: String): ClassScan {
        val pluginJarClasses = zip.entries().asSequence()
            .filter { it.name.endsWith(".class") }
            .map { it.name.removeSuffix(".class") }
            .toSet()
        val scan = ClassScan(pluginJarClasses, pluginInternalName)
        for (entry in zip.entries()) {
            if (!entry.name.endsWith(".class")) continue
            zip.getInputStream(entry).use { input ->
                val classNode = ClassNode()
                ClassReader(input).accept(classNode, 0)
                scan.scanClass(classNode)
            }
        }
        return scan
    }

    /** Collects the violations of one JAR. Not shared between threads (one per [verifyJar] call). */
    private class ClassScan(private val pluginJarClasses: Set<String>, private val pluginInternalName: String) {
        /** In the order found; [verifyJar] drops repeats (a repeated call site is one problem). */
        val violations = mutableListOf<String>()

        /** The first permission the JAR needs, reported only when [violations] stays empty. */
        var permissionNeeded: RequiresPermissionException? = null
            private set
        private var where = ""

        fun scanClass(classNode: ClassNode) {
            for (method in classNode.methods) {
                where = "${dotted(classNode.name)}.${method.name}()"
                scanMethod(method)
            }
        }

        private fun scanMethod(method: MethodNode) {
            for (insn in method.instructions) {
                when (insn) {
                    is MethodInsnNode -> checkMethodInsn(insn)
                    is FieldInsnNode -> checkMember(insn.owner, insn.name, insn.desc, "class")
                    is InvokeDynamicInsnNode -> checkInvokeDynamic(insn)
                    is LdcInsnNode -> checkLdc(insn)
                }
            }
        }

        private fun isForeign(owner: String) = !pluginJarClasses.contains(owner)

        private fun ownerAllowed(owner: String) =
            !isForeign(owner) || PluginSecurityPolicy.isAsmOwnerAllowed(owner, isTrusted = false)

        /**
         * Field/method access: for guarded owners the member must be `Allow` (a `Redirect` member
         * still called directly was missed by the rewriter); otherwise the owner must be allowed.
         */
        private fun checkMember(owner: String, name: String, desc: String, label: String) {
            if (isForeign(owner) && PluginSecurityPolicy.isMemberGuarded(owner)) {
                val rule = GuardedMemberPolicy.ruleFor(MemberRef(owner, name, desc))
                if (rule != MemberRule.Allow) {
                    violations += "Disallowed member '${dotted(owner)}.$name$desc' referenced in $where"
                }
                return
            }
            if (!ownerAllowed(owner)) {
                violations += "Disallowed $label '${dotted(owner)}' referenced in $where"
            }
        }

        private fun checkMethodInsn(insn: MethodInsnNode) {
            checkMember(insn.owner, insn.name, insn.desc, "class")
            checkTargetedCalls(insn)
            checkReflectionByName(insn)
            checkPermissionGatedCalls(insn)
        }

        private fun checkTargetedCalls(insn: MethodInsnNode) {
            val owner = insn.owner
            val name = insn.name
            // Fallback block for dangerous Runtime calls (in case the bytecode transformer missed them)
            if (owner == "java/lang/Runtime" && name in setOf("exec", "loadLibrary", "load", "exit", "halt")) {
                violations += "Blocked Runtime.$name() invocation in $where"
            }
            if (owner == "java/net/URL" && (name == "openStream" || name == "openConnection" || name == "getContent")) {
                violations += "Illegal URL.$name() call in $where. Use the NiceHttp 'app' object for network requests."
            }
            if (owner == "java/lang/Class" && name == "forName" && insn.desc.contains("ClassLoader")) {
                violations += "Illegal ClassLoader injection via Class.forName(...) in $where"
            }
            if (owner == "java/lang/System" &&
                name in setOf("exit", "loadLibrary", "load", "setSecurityManager", "getProperty", "getProperties", "getenv")
            ) {
                violations += "Disallowed System.$name() call in $where"
            }
        }

        /** Reflection by string targeting dangerous system internals. */
        private fun checkReflectionByName(insn: MethodInsnNode) {
            val reflective = (
                insn.owner == "java/lang/Class" &&
                    insn.name in setOf("forName", "getMethod", "getDeclaredMethod", "getField", "getDeclaredField")
                ) || (insn.owner == "java/lang/ClassLoader" && insn.name == "loadClass")
            if (!reflective) return
            val prev = insn.previous
            if (prev is LdcInsnNode && prev.cst is String) {
                val targetName = (prev.cst as String).trim().replace('/', '.')
                if (isDisallowedReflectionTarget(targetName)) {
                    violations += "Disallowed reflective invocation ${dotted(insn.owner)}.${insn.name}(\"$targetName\") in $where"
                }
            }
        }

        /** Raw sockets need the NETWORK_SOCKETS permission (HTTP via URLConnection is member-guarded & SSRF-safe via URLStub). */
        private fun checkPermissionGatedCalls(insn: MethodInsnNode) {
            val owner = insn.owner
            val rawSocket = owner == "java/net/Socket" || owner == "java/net/ServerSocket" || owner == "java/net/DatagramSocket"
            if (!rawSocket) return
            if (PluginPermissionAPI.hasPermission(pluginInternalName, PluginPermission.NETWORK_SOCKETS)) return
            val permission = PluginPermission.NETWORK_SOCKETS.displayName
            if (permissionNeeded != null) return
            permissionNeeded = RequiresPermissionException(
                permission,
                "Raw socket access in ${where.substringBeforeLast('.')}. Requires '$permission' permission.",
            )
        }

        private fun checkInvokeDynamic(insn: InvokeDynamicInsnNode) {
            if (!ownerAllowed(insn.bsm.owner)) {
                violations += "Disallowed invokedynamic bootstrap method owner '${dotted(insn.bsm.owner)}' in $where"
            }
            for (arg in insn.bsmArgs) {
                when {
                    arg is Handle -> checkHandle(arg, "in invokedynamic arguments")
                    arg is Type && arg.sort == Type.OBJECT && !ownerAllowed(arg.internalName) ->
                        violations += "Disallowed Type '${dotted(arg.internalName)}' in invokedynamic arguments in $where"
                }
            }
        }

        /**
         * A method handle to a guarded member is refused unless the member is `Allow`: handles
         * (`InetAddress::getByName`) are not rewritten, so a `Redirect` member must not escape that way.
         */
        private fun checkHandle(handle: Handle, context: String) {
            val owner = handle.owner
            if (isForeign(owner) && PluginSecurityPolicy.isMemberGuarded(owner)) {
                if (GuardedMemberPolicy.ruleFor(MemberRef(owner, handle.name, handle.desc)) != MemberRule.Allow) {
                    violations += "Disallowed MethodHandle '${dotted(owner)}.${handle.name}${handle.desc}' $context in $where"
                }
                return
            }
            if (!ownerAllowed(owner)) {
                violations += "Disallowed MethodHandle '${dotted(owner)}' $context in $where"
            }
        }

        /** Constant pool loads: MethodHandle, Type, or String reflection targets. */
        private fun checkLdc(insn: LdcInsnNode) {
            when (val cst = insn.cst) {
                is Handle -> checkHandle(cst, "loaded via LDC")
                is Type -> if (cst.sort == Type.OBJECT && !ownerAllowed(cst.internalName)) {
                    violations += "Disallowed Type '${dotted(cst.internalName)}' loaded via LDC in $where"
                }
                is String -> {
                    val targetStr = cst.trim().replace('/', '.')
                    if (isDisallowedReflectionTarget(targetStr)) {
                        violations += "Disallowed reflective class reference '$targetStr' in $where"
                    }
                }
            }
        }
    }

    private fun isDisallowedReflectionTarget(name: String): Boolean =
        DISALLOWED_REFLECTION_TARGETS.any {
            name.equals(it, ignoreCase = true) || name.startsWith("$it.") || name.startsWith("$it$")
        }

    private fun dotted(internalName: String) = internalName.replace('/', '.')
}

/**
 * Thrown by [PluginSecurityVerifier] when an untrusted plugin references disallowed classes or
 * members. Subclasses [SecurityException] and carries the distinct violation list so callers can
 * present a structured, human-readable summary.
 */
class PluginSecurityBlockedException(
    val violations: List<String>,
    message: String,
) : SecurityException(message)

class RequiresPermissionException(val permissionName: String, message: String) : SecurityException(message)
