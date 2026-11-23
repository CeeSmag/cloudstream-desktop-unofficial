package com.lagradost.runtime.security

/**
 * Member-level policy for JDK types that are safe only in part.
 *
 * [PluginSecurityPolicy] decides per class. That is too coarse for types such as
 * [java.net.InetAddress]: building and comparing addresses is pure computation, while
 * `isReachable` probes the network, `getLocalHost` leaks the machine name and `getHostName`
 * does reverse DNS. For a **guarded owner** every member is classified here:
 *
 * - [MemberRule.Allow]: the plugin may call it directly.
 * - [MemberRule.Redirect]: [com.lagradost.runtime.loader.GuardedCallRewriter] rewrites the call
 *   site to a static guard method before verification. The verifier refuses a direct call, so a
 *   call the rewriter missed fails closed instead of running unguarded.
 * - not listed: denied (default deny; members added by a newer JDK stay denied until reviewed).
 *
 * The same table drives the rewriter and [PluginSecurityVerifier], so the two cannot drift.
 * Stateless and immutable: safe to use from any thread.
 */
object GuardedMemberPolicy {

    /** A field or method of a guarded owner, by JVM internal names (`java/net/InetAddress`). */
    data class MemberRef(val owner: String, val name: String, val descriptor: String)

    sealed interface MemberRule {
        data object Allow : MemberRule

        /**
         * Call static [name]/[descriptor] on [owner] instead. For an [instance] member the
         * receiver becomes the stub's first argument.
         */
        data class Redirect(
            val owner: String,
            val name: String,
            val descriptor: String,
            val instance: Boolean,
        ) : MemberRule
    }

    const val INET_ADDRESS_STUB = "com/lagradost/runtime/loader/stubs/InetAddressStub"

    private const val INET = "java/net/InetAddress"
    private const val INET4 = "java/net/Inet4Address"
    private const val INET6 = "java/net/Inet6Address"
    private const val PROXY = "java/net/Proxy"

    /** Owners whose static and instance members are shared through inheritance from [INET]. */
    private val INET_HIERARCHY = setOf(INET, INET4, INET6)

    /** Public members of [Object]; javac/kotlinc name the static receiver type as their owner. */
    private val OBJECT_MEMBERS: Set<Pair<String, String>> = setOf(
        "equals" to "(Ljava/lang/Object;)Z",
        "hashCode" to "()I",
        "toString" to "()Ljava/lang/String;",
        "getClass" to "()Ljava/lang/Class;",
        "notify" to "()V",
        "notifyAll" to "()V",
        "wait" to "()V",
        "wait" to "(J)V",
        "wait" to "(JI)V",
    )

    private val INET_ALLOWED: Set<Pair<String, String>> = OBJECT_MEMBERS + setOf(
        "getByAddress" to "([B)Ljava/net/InetAddress;",
        "getByAddress" to "(Ljava/lang/String;[B)Ljava/net/InetAddress;",
        "getLoopbackAddress" to "()Ljava/net/InetAddress;",
        "getAddress" to "()[B",
        "getHostAddress" to "()Ljava/lang/String;",
        "isAnyLocalAddress" to "()Z",
        "isLoopbackAddress" to "()Z",
        "isLinkLocalAddress" to "()Z",
        "isSiteLocalAddress" to "()Z",
        "isMulticastAddress" to "()Z",
        "isMCGlobal" to "()Z",
        "isMCNodeLocal" to "()Z",
        "isMCLinkLocal" to "()Z",
        "isMCSiteLocal" to "()Z",
        "isMCOrgLocal" to "()Z",
    )

    private val INET6_ONLY_ALLOWED: Set<Pair<String, String>> = setOf(
        "getScopeId" to "()I",
        "isIPv4CompatibleAddress" to "()Z",
        "getByAddress" to "(Ljava/lang/String;[BI)Ljava/net/Inet6Address;",
    )

    /** Static members: same descriptor on the stub. */
    private val INET_STATIC_REDIRECTS: Set<Pair<String, String>> = setOf(
        "getByName" to "(Ljava/lang/String;)Ljava/net/InetAddress;",
        "getAllByName" to "(Ljava/lang/String;)[Ljava/net/InetAddress;",
        "getLocalHost" to "()Ljava/net/InetAddress;",
    )

    /** Instance members: the stub takes the receiver as its first argument. */
    private val INET_INSTANCE_REDIRECTS: Set<Pair<String, String>> = setOf(
        "getHostName" to "()Ljava/lang/String;",
        "getCanonicalHostName" to "()Ljava/lang/String;",
        "isReachable" to "(I)Z",
        "isReachable" to "(Ljava/net/NetworkInterface;II)Z",
    )

    /**
     * `Proxy.NO_PROXY` and read-only accessors. `<init>` stays denied (no proxies to arbitrary
     * hosts) and `address()` too (it hands out a [java.net.SocketAddress], a denied type).
     */
    private val PROXY_ALLOWED: Set<Pair<String, String>> = OBJECT_MEMBERS + setOf(
        "NO_PROXY" to "Ljava/net/Proxy;",
        "type" to "()Ljava/net/Proxy\$Type;",
    )

    /** Guarded owners in dotted form, for runtime class loading checks. */
    val GUARDED_CLASS_NAMES: Set<String> =
        (INET_HIERARCHY + PROXY).map { it.replace('/', '.') }.toSet()

    fun isGuardedOwner(internalName: String): Boolean =
        internalName in INET_HIERARCHY || internalName == PROXY

    /** The rule for one member, or `null` when it is denied (also for owners that are not guarded). */
    fun ruleFor(ref: MemberRef): MemberRule? {
        val member = ref.name to ref.descriptor
        return when (ref.owner) {
            in INET_HIERARCHY -> inetRule(ref.owner, member)
            PROXY -> if (member in PROXY_ALLOWED) MemberRule.Allow else null
            else -> null
        }
    }

    private fun inetRule(owner: String, member: Pair<String, String>): MemberRule? = when {
        member in INET_ALLOWED -> MemberRule.Allow
        owner == INET6 && member in INET6_ONLY_ALLOWED -> MemberRule.Allow
        member in INET_STATIC_REDIRECTS ->
            MemberRule.Redirect(INET_ADDRESS_STUB, member.first, member.second, instance = false)
        member in INET_INSTANCE_REDIRECTS ->
            MemberRule.Redirect(
                INET_ADDRESS_STUB,
                member.first,
                member.second.replaceFirst("(", "(L$INET;"),
                instance = true,
            )
        else -> null
    }

    /** Every listed member, for drift tests and diagnostics. */
    fun allRules(): Map<MemberRef, MemberRule> {
        val rules = LinkedHashMap<MemberRef, MemberRule>()
        val inetMembers = INET_ALLOWED + INET_STATIC_REDIRECTS + INET_INSTANCE_REDIRECTS
        for (owner in INET_HIERARCHY) {
            val members = if (owner == INET6) inetMembers + INET6_ONLY_ALLOWED else inetMembers
            for ((name, desc) in members) {
                val ref = MemberRef(owner, name, desc)
                ruleFor(ref)?.let { rules[ref] = it }
            }
        }
        for ((name, desc) in PROXY_ALLOWED) {
            rules[MemberRef(PROXY, name, desc)] = MemberRule.Allow
        }
        return rules
    }
}
