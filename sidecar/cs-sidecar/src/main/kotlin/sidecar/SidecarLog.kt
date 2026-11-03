package sidecar

import java.io.ByteArrayOutputStream
import java.io.OutputStream

/**
 * The sidecar's own log lines, printed to stderr as `[sidecar] LEVEL message`; the app reads the
 * level from the line. Debug lines are built and printed only while the app has debug logging on.
 */
object SidecarLog {
    @Volatile
    var debugEnabled: Boolean = false

    fun debug(message: () -> String) {
        if (debugEnabled) print("DEBUG", message())
    }

    fun info(message: String) = print("INFO", message)

    fun warn(message: String) = print("WARN", message)

    /** Logs an error; the stack trace follows on the lines after it. */
    fun error(message: String, failure: Throwable? = null) {
        print("ERROR", message)
        failure?.printStackTrace()
    }

    private fun print(level: String, message: String) = System.err.println("[sidecar] $level $message")
}

/** Words of a line searched for its level, like the app does. */
private const val LEVEL_WORDS = 4

/**
 * Sits in front of stderr and drops DEBUG and TRACE lines, and the lines that continue them
 * (stack traces, printed web pages), while debug logging is off. Plugins print a lot of debug
 * output; this keeps it from costing anything when nobody asked for it.
 */
class DebugLineFilter(private val target: OutputStream) : OutputStream() {
    private val line = ByteArrayOutputStream()
    private var inDebugBlock = false

    @Synchronized
    override fun write(byte: Int) {
        if (byte == '\n'.code) endLine() else line.write(byte)
    }

    @Synchronized
    override fun write(bytes: ByteArray, offset: Int, length: Int) {
        for (index in offset until offset + length) write(bytes[index].toInt())
    }

    @Synchronized
    override fun flush() = target.flush()

    private fun endLine() {
        val text = line.toString(Charsets.UTF_8)
        line.reset()
        inDebugBlock = levelOf(text)?.let { it == "DEBUG" || it == "TRACE" } ?: inDebugBlock
        if (inDebugBlock && !SidecarLog.debugEnabled) return
        target.write((text + "\n").toByteArray(Charsets.UTF_8))
        target.flush()
    }

    private fun levelOf(text: String): String? =
        text.split(' ', '\t').filter { it.isNotEmpty() }.take(LEVEL_WORDS)
            .map { it.trim('[', ']', ':') }
            .firstOrNull { it in LEVELS }

    private companion object {
        val LEVELS = setOf("ERROR", "SEVERE", "WARN", "WARNING", "INFO", "DEBUG", "TRACE")
    }
}
