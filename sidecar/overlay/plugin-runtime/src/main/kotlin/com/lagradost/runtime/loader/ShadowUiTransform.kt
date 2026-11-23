package com.lagradost.runtime.loader

/**
 * Marker written into transformed plugin JARs as `META-INF/shadowui.version`.
 * [ExtensionLoader] only reuses a cached `*-jvm.jar` when this exact value is present,
 * so bumping it forces a re-transpile + [PluginBytecodeTransformer] pass.
 *
 * History: 4 = raw-socket and Thread remap; 5 = member-guarded `InetAddress` calls
 * ([GuardedCallRewriter]); 6 = WebView/MotionEvent/Fragment/BottomSheet stubs replace previously
 * neutralised calls.
 */
object ShadowUiTransform {
    /** Keep in sync with what [PluginBytecodeTransformer] writes after a successful transform. */
    const val VERSION: String = "6"
}
