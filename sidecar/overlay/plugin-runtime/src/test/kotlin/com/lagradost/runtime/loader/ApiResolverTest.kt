package com.lagradost.runtime.loader

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * [ApiResolver] against the real Android stubs and JDK on this module's classpath, through the
 * runtime's own loader as the transformer uses it. (The plugin-facing [SafePluginClassLoader] would
 * make up an empty ghost for a missing Android class, so it never says "missing".)
 */
class ApiResolverTest {

    private val loader: ClassLoader = ApiResolverTest::class.java.classLoader

    private fun exact(owner: String, name: String, descriptor: String, isMethod: Boolean = true): Boolean {
        val clazz = assertNotNull(ApiResolver.classOrNull(loader, owner), owner)
        return ApiResolver.resolvesExactly(clazz, name, descriptor, isMethod)
    }

    @Test
    fun `members that exist resolve exactly, inherited ones included`() {
        assertTrue(exact("android/content/res/Resources", "getAssets", "()Landroid/content/res/AssetManager;"))
        assertTrue(exact("android/content/Context", "getResources", "()Landroid/content/res/Resources;"))
        // Declared on Object, found through the superclass chain.
        assertTrue(exact("android/content/res/AssetManager", "hashCode", "()I"))
        // Interface method of a JDK class (List declares it, ArrayList implements it).
        assertTrue(exact("java/util/ArrayList", "size", "()I"))
        assertTrue(exact("java/util/AbstractList", "spliterator", "()Ljava/util/Spliterator;"))
        assertTrue(exact("android/content/res/AssetManager", "<init>", "()V"))
        assertTrue(exact("android/content/res/AssetManager", "ACCESS_STREAMING", "I", isMethod = false))
    }

    @Test
    fun `a wrong return type or a missing member does not link`() {
        assertFalse(exact("android/content/res/Resources", "getAssets", "()Ljava/lang/Object;"), "return type counts")
        assertFalse(exact("android/content/res/Resources", "getNothing", "()V"))
        assertFalse(exact("android/content/res/AssetManager", "<init>", "(I)V"))
        assertFalse(exact("android/content/res/AssetManager", "ACCESS_STREAMING", "J", isMethod = false))
    }

    @Test
    fun `a class the sandbox has none of is reported missing, where plugins would get a ghost`() {
        assertNull(ApiResolver.classOrNull(loader, "android/net/wifi/WifiNotAStub"))
        assertNull(ApiResolver.classOrNull(loader, "com/example/Nothing"))
        val pluginSide = SafePluginClassLoader(loader, isTrusted = false)
        assertNotNull(ApiResolver.classOrNull(pluginSide, "android/net/wifi/WifiNotAStub"), "ghost stub")
    }

    @Test
    fun `the lenient test accepts supertype arguments and ignores the return type`() {
        // Context.getString(int) exists; asked with another return type it still counts as present.
        assertTrue(ApiResolver.resolvesLoosely(loader, "android/content/Context", "getString", "(I)Ljava/lang/Object;", wantMethod = true))
        assertFalse(ApiResolver.resolvesLoosely(loader, "android/content/Context", "nothing", "()V", wantMethod = true))
    }

    @Test
    fun `only unresolved calls into UI classes are neutralised`() {
        assertTrue(ApiResolver.isUiClass("android/widget/Toast"))
        assertFalse(ApiResolver.isUiClass("android/content/res/Resources"))
        assertTrue(ApiResolver.isNeutralisedCall(loader, "android/widget/NotAWidget", "show", "()V"))
        assertFalse(ApiResolver.isNeutralisedCall(loader, "android/widget/NotAWidget", "<init>", "()V"), "constructors never")
        assertFalse(
            ApiResolver.isNeutralisedCall(loader, "android/content/res/Resources", "getNothing", "()V"),
            "outside UI classes a missing member fails at run time instead",
        )
        assertEquals(
            ApiResolver.isNeutralisedField(loader, "android/widget/NotAWidget", "FLAG", "I"),
            true,
        )
    }
}
