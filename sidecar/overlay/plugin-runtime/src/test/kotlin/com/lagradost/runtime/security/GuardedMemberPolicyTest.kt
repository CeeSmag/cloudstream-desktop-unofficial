package com.lagradost.runtime.security

import com.lagradost.runtime.loader.stubs.InetAddressStub
import com.lagradost.runtime.loader.stubs.ReflectionStub
import com.lagradost.runtime.security.GuardedMemberPolicy.MemberRef
import com.lagradost.runtime.security.GuardedMemberPolicy.MemberRule
import org.junit.jupiter.api.Test
import org.objectweb.asm.Type
import java.lang.reflect.Modifier
import java.net.HttpURLConnection
import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.net.Proxy
import java.net.URLConnection
import javax.net.ssl.HttpsURLConnection
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.test.fail

/**
 * Drift guards for [GuardedMemberPolicy]: the table must match the running JDK, never allow a
 * dangerous member, never hand a denied type to plugin code, and point only at real stub methods.
 */
class GuardedMemberPolicyTest {

    private val guardedClasses = listOf(
        InetAddress::class.java,
        Inet4Address::class.java,
        Inet6Address::class.java,
        Proxy::class.java,
        URLConnection::class.java,
        HttpURLConnection::class.java,
        HttpsURLConnection::class.java,
    )

    /**
     * Members of guarded classes that were reviewed and are denied on purpose. Anything public in
     * `java.net` / `javax.net.ssl` that is neither listed in the policy nor here fails
     * [everyPublicJdkMemberIsReviewed], so a JDK upgrade that adds a member forces a decision
     * instead of silently denying or allowing.
     */
    private val reviewedDenied: Set<String> = setOf(
        // Hands out a NetworkInterface (denied type).
        "java/net/Inet6Address.getScopedInterface()Ljava/net/NetworkInterface;",
        "java/net/Inet6Address.getByAddress(Ljava/lang/String;[BLjava/net/NetworkInterface;)Ljava/net/Inet6Address;",
        // JDK 22+: pure literal parser, not needed while IpLiteral covers getByName.
        "java/net/InetAddress.ofLiteral(Ljava/lang/String;)Ljava/net/InetAddress;",
        "java/net/Inet4Address.ofLiteral(Ljava/lang/String;)Ljava/net/Inet4Address;",
        "java/net/Inet6Address.ofLiteral(Ljava/lang/String;)Ljava/net/InetAddress;",
        "java/net/Inet4Address.ofPosixLiteral(Ljava/lang/String;)Ljava/net/Inet4Address;",
        // Proxy: arbitrary proxies and their SocketAddress stay out of plugin hands.
        "java/net/Proxy.<init>(Ljava/net/Proxy\$Type;Ljava/net/SocketAddress;)V",
        "java/net/Proxy.address()Ljava/net/SocketAddress;",
    ) + httpConnectionReviewedDenied()

    private fun httpConnectionReviewedDenied(): Set<String> {
        val urlConnDenied = listOf(
            "getContent()Ljava/lang/Object;",
            "getContent([Ljava/lang/Class;)Ljava/lang/Object;",
            "getPermission()Ljava/security/Permission;",
            "getDefaultUseCaches()Z",
            "getDefaultUseCaches(Ljava/lang/String;)Z",
            "setDefaultUseCaches(Z)V",
            "setDefaultUseCaches(Ljava/lang/String;Z)V",
            "getDefaultAllowUserInteraction()Z",
            "setDefaultAllowUserInteraction(Z)V",
            "getDefaultRequestProperty(Ljava/lang/String;)Ljava/lang/String;",
            "setDefaultRequestProperty(Ljava/lang/String;Ljava/lang/String;)V",
            "getFileNameMap()Ljava/net/FileNameMap;",
            "setFileNameMap(Ljava/net/FileNameMap;)V",
            "setContentHandlerFactory(Ljava/net/ContentHandlerFactory;)V",
        )
        val httpOnlyDenied = listOf(
            "setFollowRedirects(Z)V",
            "getFollowRedirects()Z",
            "setAuthenticator(Ljava/net/Authenticator;)V",
        )
        val httpsOnlyDenied = listOf(
            "setSSLSocketFactory(Ljavax/net/ssl/SSLSocketFactory;)V",
            "getSSLSocketFactory()Ljavax/net/ssl/SSLSocketFactory;",
            "setDefaultSSLSocketFactory(Ljavax/net/ssl/SSLSocketFactory;)V",
            "getDefaultSSLSocketFactory()Ljavax/net/ssl/SSLSocketFactory;",
            "setHostnameVerifier(Ljavax/net/ssl/HostnameVerifier;)V",
            "getHostnameVerifier()Ljavax/net/ssl/HostnameVerifier;",
            "setDefaultHostnameVerifier(Ljavax/net/ssl/HostnameVerifier;)V",
            "getDefaultHostnameVerifier()Ljavax/net/ssl/HostnameVerifier;",
            "getSSLSession()Ljava/util/Optional;",
        )
        val out = LinkedHashSet<String>()
        for (owner in listOf("java/net/URLConnection", "java/net/HttpURLConnection", "javax/net/ssl/HttpsURLConnection")) {
            for (m in urlConnDenied) out += "$owner.$m"
        }
        for (owner in listOf("java/net/HttpURLConnection", "javax/net/ssl/HttpsURLConnection")) {
            for (m in httpOnlyDenied) out += "$owner.$m"
        }
        for (m in httpsOnlyDenied) out += "javax/net/ssl/HttpsURLConnection.$m"
        return out
    }

    @Test
    fun `network and host-revealing members are never allowed directly`() {
        val inetOwners = listOf("java/net/InetAddress", "java/net/Inet4Address", "java/net/Inet6Address")
        val risky = listOf(
            "getByName" to "(Ljava/lang/String;)Ljava/net/InetAddress;",
            "getAllByName" to "(Ljava/lang/String;)[Ljava/net/InetAddress;",
            "getLocalHost" to "()Ljava/net/InetAddress;",
            "getHostName" to "()Ljava/lang/String;",
            "getCanonicalHostName" to "()Ljava/lang/String;",
            "isReachable" to "(I)Z",
            "isReachable" to "(Ljava/net/NetworkInterface;II)Z",
        )
        for (owner in inetOwners) {
            for ((name, desc) in risky) {
                val rule = GuardedMemberPolicy.ruleFor(MemberRef(owner, name, desc))
                assertTrue(rule is MemberRule.Redirect, "$owner.$name$desc must be redirected, was $rule")
            }
        }
        for (denied in reviewedDenied) {
            val ref = parse(denied)
            assertNull(GuardedMemberPolicy.ruleFor(ref), "$denied must stay denied")
        }
    }

    @Test
    fun `unknown members and unguarded owners are denied`() {
        assertNull(GuardedMemberPolicy.ruleFor(MemberRef("java/net/InetAddress", "madeUp", "()V")))
        assertNull(GuardedMemberPolicy.ruleFor(MemberRef("java/net/InetAddress", "getAddress", "()I")))
        assertNull(GuardedMemberPolicy.ruleFor(MemberRef("java/net/NetworkInterface", "getName", "()Ljava/lang/String;")))
        // Inet6-only members do not leak to the other owners.
        assertNull(GuardedMemberPolicy.ruleFor(MemberRef("java/net/Inet4Address", "getScopeId", "()I")))
        assertEquals(MemberRule.Allow, GuardedMemberPolicy.ruleFor(MemberRef("java/net/Inet6Address", "getScopeId", "()I")))
    }

    @Test
    fun `every listed member exists on the JDK with that descriptor`() {
        for (ref in GuardedMemberPolicy.allRules().keys) {
            val cls = Class.forName(ref.owner.replace('/', '.'))
            val found = if (ref.descriptor.startsWith("(")) {
                cls.methods.any { it.name == ref.name && Type.getMethodDescriptor(it) == ref.descriptor }
            } else {
                cls.fields.any { it.name == ref.name && Type.getDescriptor(it.type) == ref.descriptor }
            }
            assertTrue(found, "policy lists ${ref.owner}.${ref.name}${ref.descriptor}, which this JDK does not have")
        }
    }

    @Test
    fun `everyPublicJdkMemberIsReviewed`() {
        val unreviewed = mutableListOf<String>()
        for (cls in guardedClasses) {
            val owner = Type.getInternalName(cls)
            val members = cls.methods.map { it.name to Type.getMethodDescriptor(it) } +
                cls.constructors.map { "<init>" to Type.getConstructorDescriptor(it) } +
                cls.fields.map { it.name to Type.getDescriptor(it.type) }
            for ((name, desc) in members) {
                val key = "$owner.$name$desc"
                if (GuardedMemberPolicy.ruleFor(MemberRef(owner, name, desc)) == null && key !in reviewedDenied) {
                    unreviewed += key
                }
            }
        }
        assertEquals(emptyList(), unreviewed, "classify these in GuardedMemberPolicy or add them to reviewedDenied")
    }

    @Test
    fun `allowed members neither take nor return a denied type`() {
        for ((ref, rule) in GuardedMemberPolicy.allRules()) {
            if (rule != MemberRule.Allow) continue
            val types = if (ref.descriptor.startsWith("(")) {
                Type.getArgumentTypes(ref.descriptor).toList() + Type.getReturnType(ref.descriptor)
            } else {
                listOf(Type.getType(ref.descriptor))
            }
            for (type in types) {
                val element = if (type.sort == Type.ARRAY) type.elementType else type
                if (element.sort != Type.OBJECT) continue
                assertTrue(
                    PluginSecurityPolicy.isClassAllowed(element.className),
                    "${ref.owner}.${ref.name}${ref.descriptor} exposes denied type ${element.className}",
                )
            }
        }
    }

    @Test
    fun `every redirect targets a public static stub method with that exact descriptor`() {
        val redirects = GuardedMemberPolicy.allRules().values.filterIsInstance<MemberRule.Redirect>()
        assertTrue(redirects.isNotEmpty())
        val stubMethods = InetAddressStub::class.java.methods
            .filter { Modifier.isStatic(it.modifiers) }
            .map { it.name to Type.getMethodDescriptor(it) }
            .toSet()
        for (redirect in redirects) {
            assertEquals(GuardedMemberPolicy.INET_ADDRESS_STUB, redirect.owner)
            if (redirect.name to redirect.descriptor !in stubMethods) {
                fail("InetAddressStub lacks static ${redirect.name}${redirect.descriptor}")
            }
            if (redirect.instance) {
                assertTrue(redirect.descriptor.startsWith("(Ljava/net/InetAddress;"), "receiver first: $redirect")
            }
        }
    }

    @Test
    fun `guarded classes load at runtime but stay out of reflection allowlists`() {
        for (name in GuardedMemberPolicy.GUARDED_CLASS_NAMES) {
            assertTrue(PluginSecurityPolicy.isClassAllowed(name), name)
        }
        assertTrue(PluginSecurityPolicy.isClassAllowed("java.net.Proxy\$Type"))
        // Neighbours stay denied.
        for (name in listOf("java.net.NetworkInterface", "java.net.InetSocketAddress", "java.net.SocketAddress")) {
            assertTrue(!PluginSecurityPolicy.isClassAllowed(name), "$name must stay denied")
        }
    }

    @Test
    fun `reflection cannot reach guarded members`() {
        // Member rules are enforced on bytecode; reflection would bypass them, so it stays default-denied.
        for (cls in guardedClasses) {
            assertFalse(ReflectionStub.isReflectionAllowed(cls, "isReachable"), "reflection on ${cls.name}")
        }
        val isReachable = InetAddress::class.java.getMethod("isReachable", Int::class.javaPrimitiveType)
        assertFailsWith<SecurityException> {
            ReflectionStub.invoke(isReachable, InetAddress.getLoopbackAddress(), arrayOf(1))
        }
    }

    private fun parse(key: String): MemberRef {
        val dot = key.indexOf('.')
        val owner = key.substring(0, dot)
        val rest = key.substring(dot + 1)
        val split = rest.indexOfFirst { it == '(' }.takeIf { it >= 0 } ?: rest.indexOf('L')
        return MemberRef(owner, rest.substring(0, split), rest.substring(split))
    }
}
