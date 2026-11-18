package com.lagradost.runtime.loader

/**
 * Marker written into transformed plugin JARs as `META-INF/shadowui.version`.
 * [ExtensionLoader] only reuses a cached `*-jvm.jar` when this exact value is present,
 * so bumping it forces a re-transpile + [PluginBytecodeTransformer] pass (raw-socket remap).
 */
object ShadowUiTransform {
    /** Keep in sync with what [PluginBytecodeTransformer] writes after a successful transform. */
    const val VERSION: String = "4"
}
