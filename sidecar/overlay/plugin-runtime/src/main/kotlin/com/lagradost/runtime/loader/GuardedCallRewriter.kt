package com.lagradost.runtime.loader

import com.lagradost.runtime.security.GuardedMemberPolicy
import com.lagradost.runtime.security.GuardedMemberPolicy.MemberRef
import com.lagradost.runtime.security.GuardedMemberPolicy.MemberRule
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes

/**
 * Rewrites plugin call sites of [GuardedMemberPolicy] `Redirect` members to their static guard,
 * e.g. `invokestatic InetAddress.getByName` -> `invokestatic InetAddressStub.getByName` and
 * `invokevirtual Inet4Address.getHostName()` -> `invokestatic InetAddressStub.getHostName(InetAddress)`.
 *
 * Used by [PluginBytecodeTransformer] (the JAR that [com.lagradost.runtime.security.PluginSecurityVerifier]
 * checks) and by [CompatPluginClassLoader] (every class as it loads), so a JAR from an older cache
 * is still guarded. A rewritten site names the stub, so running both passes is harmless.
 *
 * Stateless.
 */
object GuardedCallRewriter {

    /**
     * Emits the guarded call into [mv] and returns `true` when the call is a `Redirect` member;
     * returns `false` (emitting nothing) for anything else.
     */
    fun tryRewrite(mv: MethodVisitor, opcode: Int, owner: String, name: String, descriptor: String): Boolean {
        if (!GuardedMemberPolicy.isGuardedOwner(owner)) return false
        val rule = GuardedMemberPolicy.ruleFor(MemberRef(owner, name, descriptor))
        if (rule !is MemberRule.Redirect) return false
        // A site whose opcode does not match the member kind is left alone; the verifier refuses it.
        val expected = if (rule.instance) Opcodes.INVOKEVIRTUAL else Opcodes.INVOKESTATIC
        if (opcode != expected) return false
        mv.visitMethodInsn(Opcodes.INVOKESTATIC, rule.owner, rule.name, rule.descriptor, false)
        return true
    }
}
