package com.lagradost.runtime.security

/**
 * Centralized security policy for CloudStream Desktop plugins.
 * Enforces a strict Default Deny (Whitelist-Only) security model.
 */
object PluginSecurityPolicy {

    private val LANGUAGE_CORE = setOf(
        "kotlin.", "kotlinx.", "java.util.", "java.text.", "java.time.",
        "java.math.", "java.security.", "javax.crypto.", "javax.net.ssl.",
    )

    val SAFE_JAVA_LANG_CLASSES = setOf(
        // Core Primitives & Wrappers
        "java.lang.Object", "java.lang.String", "java.lang.CharSequence", "java.lang.Number",
        "java.lang.Integer", "java.lang.Long", "java.lang.Double", "java.lang.Float",
        "java.lang.Short", "java.lang.Byte", "java.lang.Boolean", "java.lang.Character", "java.lang.Void",
        // Language & Computation Constructs
        "java.lang.Enum", "java.lang.Comparable", "java.lang.Iterable", "java.lang.Cloneable",
        "java.lang.Appendable", "java.lang.StringBuilder", "java.lang.StringBuffer",
        "java.lang.Math", "java.lang.StrictMath", "java.lang.Class", "java.lang.System",
        "java.lang.Runtime", "java.lang.AutoCloseable", "java.lang.Runnable", "java.lang.ThreadLocal",
        // Standard Annotations
        "java.lang.Deprecated", "java.lang.Override", "java.lang.SuppressWarnings",
        // Standard Exceptions and Errors
        "java.lang.Throwable", "java.lang.Exception", "java.lang.RuntimeException",
        "java.lang.IllegalArgumentException", "java.lang.IllegalStateException",
        "java.lang.NullPointerException", "java.lang.IndexOutOfBoundsException",
        "java.lang.ArrayIndexOutOfBoundsException", "java.lang.StringIndexOutOfBoundsException",
        "java.lang.UnsupportedOperationException", "java.lang.IllegalAccessException",
        "java.lang.NoSuchMethodException", "java.lang.NoSuchFieldException",
        "java.lang.ClassNotFoundException", "java.lang.NoClassDefFoundError",
        "java.lang.Error", "java.lang.AssertionError", "java.lang.NumberFormatException",
        "java.lang.ArithmeticException", "java.lang.ClassCastException", "java.lang.SecurityException",
        "java.lang.TypeNotPresentException", "java.lang.ArrayStoreException",
        "java.lang.NegativeArraySizeException", "java.lang.IllegalMonitorStateException",
        "java.lang.NoSuchFieldError", "java.lang.NoSuchMethodError",
        "java.lang.IncompatibleClassChangeError", "java.lang.LinkageError",
        "java.lang.IllegalAccessError", "java.lang.AbstractMethodError",
        "java.lang.InstantiationError", "java.lang.ExceptionInInitializerError",
    )

    private val SAFE_REFLECT = setOf(
        "java.lang.reflect.Type", "java.lang.reflect.ParameterizedType",
        "java.lang.reflect.GenericArrayType", "java.lang.reflect.WildcardType",
        "java.lang.reflect.TypeVariable", "java.lang.reflect.InvocationTargetException",
        "java.lang.reflect.Array", "java.lang.reflect.Member", "java.lang.reflect.Modifier",
        "java.lang.reflect.Method", "java.lang.reflect.Field",
        "java.lang.reflect.Constructor", "java.lang.reflect.AccessibleObject",
    )

    // java.util.concurrent.Executors is routed to ExecutorsStub (bounded daemon pools) by CompatPluginClassLoader.
    private val DANGEROUS_UTIL_CLASSES = setOf(
        "java.util.ServiceLoader",
        "java.util.jar.JarFile",
    )

    private val CLOUDSTREAM_ECOSYSTEM = setOf(
        "com.lagradost.cloudstream3.", "com.lagradost.nicehttp.", "com.lagradost.api.",
        "com.lagradost.safefile.", "org.jsoup.", "com.fleeksoft.ksoup.",
        "com.fasterxml.jackson.", "com.google.gson.", "org.json.", "org.mozilla.",
        "com.evilsnow.rhino.", "org.schabi.newpipe.extractor.", "dev.whyoleg.cryptography.",
        "io.ktor.", "android.", "androidx.", "com.android.", "com.google.android.",
        "okhttp3.", "okio.",
    )

    private val NETWORK_SAFE_SUBSET = setOf(
        "java.net.URI", "java.net.URL", "java.net.URLDecoder", "java.net.URLEncoder",
        "java.net.HttpCookie", "java.net.CookieManager", "java.net.IDN",
        "java.net.MalformedURLException", "java.net.URISyntaxException",
        "java.net.UnknownHostException", "java.net.SocketTimeoutException",
        "java.net.ConnectException", "java.net.ProtocolException",
    )

    private val SAFE_INVOKE = setOf(
        "java.lang.invoke.LambdaMetafactory", "java.lang.invoke.MethodType",
        "java.lang.invoke.CallSite", "java.lang.invoke.ConstantCallSite",
        "java.lang.invoke.MutableCallSite", "java.lang.invoke.VolatileCallSite",
        "java.lang.invoke.StringConcatFactory",
    )

    private val SAFE_IO = setOf(
        "java.io.InputStream", "java.io.OutputStream",
        "java.io.ByteArrayInputStream", "java.io.ByteArrayOutputStream",
        "java.io.BufferedInputStream", "java.io.BufferedOutputStream",
        "java.io.DataInputStream", "java.io.DataOutputStream",
        "java.io.Reader", "java.io.Writer", "java.io.StringReader", "java.io.StringWriter",
        "java.io.BufferedReader", "java.io.BufferedWriter",
        "java.io.InputStreamReader", "java.io.OutputStreamWriter",
        "java.io.Closeable", "java.io.AutoCloseable", "java.io.Flushable",
        "java.io.IOException", "java.io.EOFException", "java.io.InterruptedIOException",
        "java.io.UnsupportedEncodingException", "java.io.CharConversionException",
        "java.io.UTFDataFormatException", "java.io.Serializable", "java.io.Externalizable",
        "java.io.ObjectStreamException", "java.io.NotSerializableException",
        "java.io.InvalidClassException", "java.io.PrintStream",
        "java.io.File", "java.io.FileInputStream", "java.io.FileOutputStream",
        "java.io.FileReader", "java.io.FileWriter",
    )

    private val SAFE_NIO_CLASSES = setOf(
        "java.nio.ByteBuffer", "java.nio.ByteOrder", "java.nio.CharBuffer",
        "java.nio.ShortBuffer", "java.nio.IntBuffer", "java.nio.LongBuffer",
        "java.nio.FloatBuffer", "java.nio.DoubleBuffer", "java.nio.Buffer",
        "java.nio.BufferUnderflowException", "java.nio.BufferOverflowException",
        "java.nio.ReadOnlyBufferException", "java.nio.InvalidMarkException",
    )

    private val EXPLICIT_DENY = setOf(
        "java.lang.Process", "java.lang.ProcessBuilder", "java.lang.ProcessBuilder\$Redirect",
        "java.lang.Compiler", "java.lang.SecurityManager", "java.lang.Thread",
        "java.lang.ThreadGroup", "java.lang.InheritableThreadLocal", "java.lang.ClassLoader",
        "java.lang.instrument.Instrumentation", "java.lang.management.ManagementFactory",
        "java.nio.file.Files", "java.nio.file.Path", "java.nio.file.Paths",
        "java.nio.file.FileSystem", "java.nio.file.FileSystems",
        "java.awt.Desktop", "java.awt.Robot", "java.net.NetworkInterface", "java.io.RandomAccessFile",
    )

    private val RAW_SOCKET_CLASSES = setOf(
        "java.net.Socket", "java.net.ServerSocket", "java.net.DatagramSocket",
        "java.net.HttpURLConnection", "java.net.URLConnection", "javax.net.ssl.HttpsURLConnection",
    )

    private val BLOCKED_PREFIXES = listOf(
        "com.lagradost.common.", "com.lagradost.cloudstream3.desktop.", "com.lagradost.runtime.",
        "com.sun.jna.", "org.bytedeco.", "app.cash.sqldelight.",
        "javax.script.", "javax.naming.", "sun.", "jdk.", "com.sun.", "com.oracle.",
        "org.apache.", "org.w3c.", "org.xml.", "org.ietf.", "org.omg.",
    )

    private val SYSTEM_OR_HOST_PREFIXES = listOf(
        "java.", "javax.", "sun.", "com.sun.", "jdk.", "com.oracle.",
        "org.ietf.", "org.omg.", "org.w3c.", "org.xml.", "org.apache.",
        "com.lagradost.common.", "com.lagradost.cloudstream3.desktop.",
        "com.lagradost.runtime.", "app.cash.sqldelight.", "org.bytedeco.", "com.sun.jna.",
    )

    fun isClassAllowed(className: String, hasSocketPermission: Boolean = false, isTrusted: Boolean = false): Boolean {
        if (isTrusted && (className == "java.lang.Thread" || className == "java.lang.ThreadGroup")) return true
        if (EXPLICIT_DENY.contains(className)) return false
        if (className.startsWith("com.lagradost.runtime.loader.stubs.")) return true
        if (BLOCKED_PREFIXES.any { className.startsWith(it) }) return false
        if (isTrusted) return true
        if (RAW_SOCKET_CLASSES.contains(className)) return hasSocketPermission
        return isAllowedStandardOrEcosystemClass(className)
    }

    private fun isAllowedStandardOrEcosystemClass(className: String): Boolean = when {
        className.startsWith("java.lang.invoke.") -> SAFE_INVOKE.contains(className)
        className.startsWith("java.lang.reflect.") -> SAFE_REFLECT.contains(className)
        className.startsWith("java.lang.annotation.") || className.startsWith("java.lang.ref.") -> true
        className.startsWith("java.lang.") -> SAFE_JAVA_LANG_CLASSES.contains(className)
        className.startsWith("java.util.") && DANGEROUS_UTIL_CLASSES.contains(className) -> false
        className.startsWith("java.io.") -> SAFE_IO.contains(className)
        className.startsWith("java.nio.charset.") -> true
        className.startsWith("java.nio.") -> SAFE_NIO_CLASSES.contains(className)
        LANGUAGE_CORE.any { className.startsWith(it) } -> true
        CLOUDSTREAM_ECOSYSTEM.any { className.startsWith(it) } -> true
        else -> NETWORK_SAFE_SUBSET.contains(className)
    }

    fun isAsmOwnerAllowed(internalName: String, isTrusted: Boolean = false): Boolean {
        val cleanInternal = internalName.trimStart('[').removePrefix("L").removeSuffix(";")
        val dotName = cleanInternal.replace('/', '.')
        return isClassAllowed(dotName, hasSocketPermission = false, isTrusted = isTrusted)
    }

    fun isSystemOrHostPackage(className: String): Boolean =
        SYSTEM_OR_HOST_PREFIXES.any { className.startsWith(it) }
}
