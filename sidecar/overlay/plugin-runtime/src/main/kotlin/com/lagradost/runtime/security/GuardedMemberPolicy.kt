package com.lagradost.runtime.security

/**
 * Member-level policy for JDK types that are safe only in part.
 *
 * [PluginSecurityPolicy] decides per class. That is too coarse for types such as
 * [java.net.InetAddress] or [java.net.HttpURLConnection]: safe instance HTTP calls on an
 * SSRF-guarded [com.lagradost.runtime.loader.stubs.SafeHttpURLConnection] are fine, while
 * subclassing (`<init>`), global JVM static setters, custom `SSLSocketFactory` / `HostnameVerifier`,
 * or `isReachable` probes must stay blocked. For a **guarded owner** every member is classified:
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
    private const val URL_CONN = "java/net/URLConnection"
    private const val HTTP_URL_CONN = "java/net/HttpURLConnection"
    private const val HTTPS_URL_CONN = "javax/net/ssl/HttpsURLConnection"

    /** Owners whose static and instance members are shared through inheritance from [INET]. */
    private val INET_HIERARCHY = setOf(INET, INET4, INET6)

    /** Owners whose instances plugins can only obtain via `URLStub.openConnection`. */
    private val HTTP_CONN_HIERARCHY = setOf(URL_CONN, HTTP_URL_CONN, HTTPS_URL_CONN)

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

    /**
     * Safe per-instance HTTP methods on [URLConnection] (instances are always created by
     * `URLStub.openConnection` and backed by SSRF-guarded OkHttp). Constructors, global static
     * mutators, `getPermission`, and reflective `getContent` stay denied.
     */
    private val URL_CONN_ALLOWED: Set<Pair<String, String>> = OBJECT_MEMBERS + setOf(
        "connect" to "()V",
        "getURL" to "()Ljava/net/URL;",
        "getInputStream" to "()Ljava/io/InputStream;",
        "getOutputStream" to "()Ljava/io/OutputStream;",
        "setConnectTimeout" to "(I)V",
        "getConnectTimeout" to "()I",
        "setReadTimeout" to "(I)V",
        "getReadTimeout" to "()I",
        "setDoInput" to "(Z)V",
        "getDoInput" to "()Z",
        "setDoOutput" to "(Z)V",
        "getDoOutput" to "()Z",
        "setUseCaches" to "(Z)V",
        "getUseCaches" to "()Z",
        "setAllowUserInteraction" to "(Z)V",
        "getAllowUserInteraction" to "()Z",
        "setIfModifiedSince" to "(J)V",
        "getIfModifiedSince" to "()J",
        "setRequestProperty" to "(Ljava/lang/String;Ljava/lang/String;)V",
        "addRequestProperty" to "(Ljava/lang/String;Ljava/lang/String;)V",
        "getRequestProperty" to "(Ljava/lang/String;)Ljava/lang/String;",
        "getRequestProperties" to "()Ljava/util/Map;",
        "getContentLength" to "()I",
        "getContentLengthLong" to "()J",
        "getContentType" to "()Ljava/lang/String;",
        "getContentEncoding" to "()Ljava/lang/String;",
        "getDate" to "()J",
        "getExpiration" to "()J",
        "getLastModified" to "()J",
        "getHeaderField" to "(I)Ljava/lang/String;",
        "getHeaderField" to "(Ljava/lang/String;)Ljava/lang/String;",
        "getHeaderFieldKey" to "(I)Ljava/lang/String;",
        "getHeaderFieldInt" to "(Ljava/lang/String;I)I",
        "getHeaderFieldLong" to "(Ljava/lang/String;J)J",
        "getHeaderFieldDate" to "(Ljava/lang/String;J)J",
        "getHeaderFields" to "()Ljava/util/Map;",
        "guessContentTypeFromName" to "(Ljava/lang/String;)Ljava/lang/String;",
        "guessContentTypeFromStream" to "(Ljava/io/InputStream;)Ljava/lang/String;",
    )

    private val HTTP_STATUS_FIELDS: Set<Pair<String, String>> = setOf(
        "HTTP_OK", "HTTP_CREATED", "HTTP_ACCEPTED", "HTTP_NOT_AUTHORITATIVE", "HTTP_NO_CONTENT",
        "HTTP_RESET", "HTTP_PARTIAL", "HTTP_MULT_CHOICE", "HTTP_MOVED_PERM", "HTTP_MOVED_TEMP",
        "HTTP_SEE_OTHER", "HTTP_NOT_MODIFIED", "HTTP_USE_PROXY", "HTTP_BAD_REQUEST",
        "HTTP_UNAUTHORIZED", "HTTP_PAYMENT_REQUIRED", "HTTP_FORBIDDEN", "HTTP_NOT_FOUND",
        "HTTP_BAD_METHOD", "HTTP_NOT_ACCEPTABLE", "HTTP_PROXY_AUTH", "HTTP_CLIENT_TIMEOUT",
        "HTTP_CONFLICT", "HTTP_GONE", "HTTP_LENGTH_REQUIRED", "HTTP_PRECON_FAILED",
        "HTTP_ENTITY_TOO_LARGE", "HTTP_REQ_TOO_LONG", "HTTP_UNSUPPORTED_TYPE",
        "HTTP_SERVER_ERROR", "HTTP_INTERNAL_ERROR", "HTTP_NOT_IMPLEMENTED", "HTTP_BAD_GATEWAY",
        "HTTP_UNAVAILABLE", "HTTP_GATEWAY_TIMEOUT", "HTTP_VERSION",
    ).map { it to "I" }.toSet()

    /** Safe per-instance members of [java.net.HttpURLConnection] (no global static setters). */
    private val HTTP_URL_CONN_ALLOWED: Set<Pair<String, String>> = URL_CONN_ALLOWED + HTTP_STATUS_FIELDS + setOf(
        "setRequestMethod" to "(Ljava/lang/String;)V",
        "getRequestMethod" to "()Ljava/lang/String;",
        "getResponseCode" to "()I",
        "getResponseMessage" to "()Ljava/lang/String;",
        "getErrorStream" to "()Ljava/io/InputStream;",
        "disconnect" to "()V",
        "usingProxy" to "()Z",
        "setInstanceFollowRedirects" to "(Z)V",
        "getInstanceFollowRedirects" to "()Z",
        "setFixedLengthStreamingMode" to "(I)V",
        "setFixedLengthStreamingMode" to "(J)V",
        "setChunkedStreamingMode" to "(I)V",
    )

    /** Read-only TLS inspection on [javax.net.ssl.HttpsURLConnection]; TLS/hostname setters stay denied. */
    private val HTTPS_URL_CONN_ALLOWED: Set<Pair<String, String>> = HTTP_URL_CONN_ALLOWED + setOf(
        "getCipherSuite" to "()Ljava/lang/String;",
        "getLocalCertificates" to "()[Ljava/security/cert/Certificate;",
        "getServerCertificates" to "()[Ljava/security/cert/Certificate;",
        "getPeerPrincipal" to "()Ljava/security/Principal;",
        "getLocalPrincipal" to "()Ljava/security/Principal;",
    )

    /** Guarded owners in dotted form, for runtime class loading checks. */
    val GUARDED_CLASS_NAMES: Set<String> =
        (INET_HIERARCHY + PROXY + HTTP_CONN_HIERARCHY).map { it.replace('/', '.') }.toSet()

    fun isGuardedOwner(internalName: String): Boolean =
        internalName in INET_HIERARCHY || internalName == PROXY || internalName in HTTP_CONN_HIERARCHY

    /** The rule for one member, or `null` when it is denied (also for owners that are not guarded). */
    fun ruleFor(ref: MemberRef): MemberRule? {
        val member = ref.name to ref.descriptor
        return when (ref.owner) {
            in INET_HIERARCHY -> inetRule(ref.owner, member)
            PROXY -> if (member in PROXY_ALLOWED) MemberRule.Allow else null
            URL_CONN -> if (member in URL_CONN_ALLOWED) MemberRule.Allow else null
            HTTP_URL_CONN -> if (member in HTTP_URL_CONN_ALLOWED) MemberRule.Allow else null
            HTTPS_URL_CONN -> if (member in HTTPS_URL_CONN_ALLOWED) MemberRule.Allow else null
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
        addAllowed(rules, PROXY, PROXY_ALLOWED)
        addAllowed(rules, URL_CONN, URL_CONN_ALLOWED)
        addAllowed(rules, HTTP_URL_CONN, HTTP_URL_CONN_ALLOWED)
        addAllowed(rules, HTTPS_URL_CONN, HTTPS_URL_CONN_ALLOWED)
        return rules
    }

    private fun addAllowed(
        rules: MutableMap<MemberRef, MemberRule>,
        owner: String,
        members: Set<Pair<String, String>>,
    ) {
        for ((name, desc) in members) {
            rules[MemberRef(owner, name, desc)] = MemberRule.Allow
        }
    }
}
