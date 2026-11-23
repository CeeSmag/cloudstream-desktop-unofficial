package apiscanner

import com.lagradost.runtime.loader.ApiResolver

/** What happens to a reference when the plugin runs in the desktop runtime. */
enum class Status {
    /** It links: the runtime provides it. */
    RESOLVED,

    /** The class exists but this member does not: `NoSuchMethodError` / `NoSuchFieldError` when used. */
    MISSING_MEMBER,

    /** The class does not exist: plugins get an empty ghost stub, so any member of it fails when used. */
    GHOST_CLASS,

    /** A call into an Android UI class the transformer replaces by a default value: it silently does nothing. */
    NEUTRALISED,

    /** The class exists but its members cannot be listed (one of their types is missing): a stub bug. */
    BROKEN_STUB,
}

/**
 * Classifies references against the runtime's classes ([loader]: the loader the transformer uses,
 * which has the Android stubs, the CloudStream library and the JDK, and makes up no ghosts).
 *
 * Not thread-safe by design: each worker makes its own (the memo is per instance), so a run shares
 * no mutable state between threads.
 */
class Classifier(private val loader: ClassLoader) {

    private val memo = HashMap<Reference, Status>()

    fun classify(ref: Reference): Status = memo.getOrPut(ref) { decide(ref) }

    private fun decide(ref: Reference): Status {
        val clazz = ApiResolver.classOrNull(loader, ref.owner)
        if (ref.kind == RefKind.CLASS) return if (clazz == null) Status.GHOST_CLASS else Status.RESOLVED
        if (clazz == null) return if (neutralised(ref)) Status.NEUTRALISED else Status.GHOST_CLASS
        val links = try {
            ApiResolver.resolvesExactly(clazz, ref.name, ref.descriptor, isMethod = ref.kind == RefKind.METHOD)
        } catch (_: LinkageError) {
            return Status.BROKEN_STUB
        } catch (_: SecurityException) {
            return Status.BROKEN_STUB
        }
        return when {
            links -> Status.RESOLVED
            neutralised(ref) -> Status.NEUTRALISED
            else -> Status.MISSING_MEMBER
        }
    }

    private fun neutralised(ref: Reference): Boolean = when (ref.kind) {
        RefKind.METHOD -> ApiResolver.isNeutralisedCall(loader, ref.owner, ref.name, ref.descriptor)
        RefKind.FIELD -> ApiResolver.isNeutralisedField(loader, ref.owner, ref.name, ref.descriptor)
        RefKind.CLASS -> false
    }
}
