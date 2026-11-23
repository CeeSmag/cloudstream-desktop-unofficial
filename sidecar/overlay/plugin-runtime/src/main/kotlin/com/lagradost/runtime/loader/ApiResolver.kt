package com.lagradost.runtime.loader

import org.objectweb.asm.Type
import java.lang.reflect.Method
import java.util.concurrent.ConcurrentHashMap

/**
 * How a plugin's reference to a class member fares against the classes the sandbox gives it
 * (Android stubs, the CloudStream library, the JDK), through a class loader of that sandbox.
 *
 * Two questions, two answers:
 * - [resolvesExactly]: would the JVM link it (name and full descriptor, return type included,
 *   searched as JVMS 5.4.3 does)? `false` means `NoSuchMethodError` / `NoSuchFieldError` at run time.
 * - [resolvesLoosely]: the transformer's older, lenient test (arguments may be supertypes, the
 *   return type is ignored), which decides [isNeutralisedCall] / [isNeutralisedField]: calls into
 *   Android UI classes that do not resolve are replaced by a default value instead of failing.
 *
 * [PluginBytecodeTransformer] and the API scanner both ask here, so the scanner reports exactly
 * what the sandbox does. Thread-safe (the one cache, of the lenient answers keyed by names as the
 * transformer always had, is a concurrent map of immutable answers).
 */
object ApiResolver {

    /** Where the transformer neutralises unresolved calls (UI code a desktop has no use for). */
    fun isUiClass(owner: String): Boolean =
        owner.startsWith("android/widget/") ||
            owner.startsWith("android/view/") ||
            owner.startsWith("android/graphics/") ||
            owner.startsWith("android/app/") ||
            owner.startsWith("android/text/") ||
            owner.startsWith("androidx/")

    /** True when the transformer replaces this call by a default value (it does not resolve). */
    fun isNeutralisedCall(loader: ClassLoader, owner: String, name: String, descriptor: String): Boolean =
        name != "<init>" && isUiClass(owner) && name != "getSharedPreferences" &&
            !resolvesLoosely(loader, owner, name, descriptor, wantMethod = true)

    /** True when the transformer replaces this field access by a default value. */
    fun isNeutralisedField(loader: ClassLoader, owner: String, name: String, descriptor: String): Boolean =
        isUiClass(owner) && !resolvesLoosely(loader, owner, name, descriptor, wantMethod = false)

    /** The class, or `null` when the loader has none (a ghost stub would stand in for it). */
    fun classOrNull(loader: ClassLoader, owner: String): Class<*>? =
        try {
            Class.forName(owner.replace('/', '.'), false, loader)
        } catch (_: ClassNotFoundException) {
            null
        } catch (_: LinkageError) {
            null
        }

    private val looseCache = ConcurrentHashMap<String, Boolean>()

    /** The transformer's lenient test (unchanged behaviour). */
    fun resolvesLoosely(
        loader: ClassLoader,
        owner: String,
        name: String,
        descriptor: String,
        wantMethod: Boolean,
    ): Boolean = looseCache.computeIfAbsent("$owner#$name#$descriptor#$wantMethod") {
        try {
            val clazz = Class.forName(owner.replace('/', '.'), false, loader)
            when {
                !wantMethod -> fieldNamed(clazz, name)
                name == "<init>" -> constructorLoosely(clazz, descriptor, loader)
                else -> methodLoosely(clazz, name, descriptor, loader)
            }
        } catch (_: Throwable) {
            false
        }
    }

    /**
     * JVM linkage: a method or constructor ([isMethod]) or a field with exactly this name and
     * descriptor in [clazz], its superclasses or its interfaces. Throws what reflection throws when
     * the class's members cannot be listed (a member's type is missing). Not cached here (a cache
     * keyed by classes would keep their loaders alive); callers that ask often memoise themselves.
     */
    fun resolvesExactly(clazz: Class<*>, name: String, descriptor: String, isMethod: Boolean): Boolean =
        when {
            !isMethod -> fieldExactly(clazz, name, descriptor)
            name == "<init>" -> clazz.declaredConstructors.any { Type.getConstructorDescriptor(it) == descriptor }
            else -> methodExactly(clazz, name, descriptor)
        }

    private fun methodExactly(clazz: Class<*>, name: String, descriptor: String): Boolean =
        lineage(clazz).any { type ->
            type.declaredMethods.any { it.name == name && Type.getMethodDescriptor(it) == descriptor }
        }

    private fun fieldExactly(clazz: Class<*>, name: String, descriptor: String): Boolean =
        lineage(clazz).any { type ->
            type.declaredFields.any { it.name == name && Type.getDescriptor(it.type) == descriptor }
        }

    /** The class, its superclasses, then every interface they implement (each once). */
    private fun lineage(clazz: Class<*>): Sequence<Class<*>> {
        val supers = generateSequence(clazz) { it.superclass }.toList()
        val interfaces = LinkedHashSet<Class<*>>()
        fun collect(type: Class<*>) {
            for (each in type.interfaces) {
                if (interfaces.add(each)) collect(each)
            }
        }
        supers.forEach(::collect)
        return (supers + interfaces).asSequence()
    }

    private fun methodLoosely(clazz: Class<*>, name: String, descriptor: String, loader: ClassLoader): Boolean {
        if (clazz.methods.any { it.name == name && looseMatch(it, descriptor, loader) }) return true
        return generateSequence(clazz) { it.superclass }
            .takeWhile { it != Any::class.java }
            .any { type -> type.declaredMethods.any { it.name == name && looseMatch(it, descriptor, loader) } }
    }

    private fun looseMatch(method: Method, descriptor: String, loader: ClassLoader): Boolean {
        if (Type.getMethodDescriptor(method) == descriptor) return true
        return argumentsAccept(method.parameterTypes, descriptor, loader)
    }

    private fun constructorLoosely(clazz: Class<*>, descriptor: String, loader: ClassLoader): Boolean =
        clazz.declaredConstructors.any {
            Type.getConstructorDescriptor(it) == descriptor || argumentsAccept(it.parameterTypes, descriptor, loader)
        }

    private fun argumentsAccept(params: Array<Class<*>>, descriptor: String, loader: ClassLoader): Boolean {
        val expected = Type.getArgumentTypes(descriptor)
        if (expected.size != params.size) return false
        return expected.indices.all { i ->
            val wanted = classOf(expected[i], loader)
            wanted != null && params[i].isAssignableFrom(wanted)
        }
    }

    private fun fieldNamed(clazz: Class<*>, name: String): Boolean {
        if (clazz.fields.any { it.name == name }) return true
        return generateSequence(clazz) { it.superclass }
            .takeWhile { it != Any::class.java }
            .any { type -> type.declaredFields.any { it.name == name } }
    }

    private fun classOf(type: Type, loader: ClassLoader): Class<*>? = when (type.sort) {
        Type.VOID -> java.lang.Void.TYPE
        Type.BOOLEAN -> java.lang.Boolean.TYPE
        Type.CHAR -> java.lang.Character.TYPE
        Type.BYTE -> java.lang.Byte.TYPE
        Type.SHORT -> java.lang.Short.TYPE
        Type.INT -> java.lang.Integer.TYPE
        Type.FLOAT -> java.lang.Float.TYPE
        Type.LONG -> java.lang.Long.TYPE
        Type.DOUBLE -> java.lang.Double.TYPE
        Type.ARRAY -> classOf(type.elementType, loader)?.let { java.lang.reflect.Array.newInstance(it, 0)::class.java }
        Type.OBJECT -> classOrNull(loader, type.internalName)
        else -> null
    }
}
