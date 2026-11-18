package com.lagradost.runtime.loader.stubs

import java.util.concurrent.atomic.AtomicInteger

/**
 * Replaces [java.lang.Thread] in plugin bytecode (composition, not a subclass — avoids clashing
 * with [Thread]'s static methods under `@JvmStatic`).
 *
 * Forces daemon + normal priority and caps concurrent plugin threads so FlixProxy can run its
 * accept loop without unlocking unrestricted host [Thread] for untrusted plugins.
 */
class ThreadStub {
    private val thread: Thread
    private var counted: Boolean = false

    constructor() {
        thread = Thread(countedRunnable(null))
    }

    constructor(target: Runnable?) {
        thread = Thread(countedRunnable(target))
    }

    constructor(name: String?) {
        thread = Thread(countedRunnable(null), name)
    }

    constructor(target: Runnable?, name: String?) {
        thread = Thread(countedRunnable(target), name)
    }

    // ThreadGroup stays denied; ignore any group the plugin passed so remapped descriptors link.
    constructor(group: ThreadGroup?, target: Runnable?) {
        thread = Thread(null as ThreadGroup?, countedRunnable(target))
    }

    constructor(group: ThreadGroup?, name: String?) {
        thread = Thread(null as ThreadGroup?, countedRunnable(null), name)
    }

    constructor(group: ThreadGroup?, target: Runnable?, name: String?) {
        thread = Thread(null as ThreadGroup?, countedRunnable(target), name)
    }

    constructor(group: ThreadGroup?, target: Runnable?, name: String?, stackSize: Long) {
        thread = Thread(null as ThreadGroup?, countedRunnable(target), name, stackSize)
    }

    constructor(
        group: ThreadGroup?,
        target: Runnable?,
        name: String?,
        stackSize: Long,
        inheritThreadLocals: Boolean,
    ) {
        thread = Thread(null as ThreadGroup?, countedRunnable(target), name, stackSize, inheritThreadLocals)
    }

    private fun countedRunnable(target: Runnable?): Runnable =
        Runnable {
            try {
                target?.run()
            } finally {
                if (counted) {
                    counted = false
                    LIVE.decrementAndGet()
                }
            }
        }

    @Synchronized
    fun start() {
        thread.isDaemon = true
        if (thread.priority > Thread.NORM_PRIORITY) {
            thread.priority = Thread.NORM_PRIORITY
        }
        val n = LIVE.incrementAndGet()
        if (n > MAX_PLUGIN_THREADS) {
            LIVE.decrementAndGet()
            throw SecurityException(
                "Plugin Security: too many threads (max $MAX_PLUGIN_THREADS).",
            )
        }
        counted = true
        try {
            thread.start()
        } catch (failure: RuntimeException) {
            counted = false
            LIVE.decrementAndGet()
            throw failure
        } catch (failure: Error) {
            counted = false
            LIVE.decrementAndGet()
            throw failure
        }
    }

    fun run() {
        thread.run()
    }

    fun join() {
        thread.join()
    }

    fun join(millis: Long) {
        thread.join(millis)
    }

    fun join(millis: Long, nanos: Int) {
        thread.join(millis, nanos)
    }

    fun interrupt() {
        thread.interrupt()
    }

    fun isInterrupted(): Boolean = thread.isInterrupted

    fun isAlive(): Boolean = thread.isAlive

    fun isDaemon(): Boolean = thread.isDaemon

    fun setDaemon(on: Boolean) {
        thread.isDaemon = on
    }

    fun getName(): String = thread.name

    fun setName(name: String?) {
        thread.name = name
    }

    fun getPriority(): Int = thread.priority

    fun setPriority(newPriority: Int) {
        thread.priority = newPriority.coerceAtMost(Thread.NORM_PRIORITY)
    }

    fun getId(): Long = thread.id

    fun getState(): Thread.State = thread.state

    fun getStackTrace(): Array<StackTraceElement> = thread.stackTrace

    fun setUncaughtExceptionHandler(eh: Thread.UncaughtExceptionHandler?) {
        thread.uncaughtExceptionHandler = eh
    }

    fun getUncaughtExceptionHandler(): Thread.UncaughtExceptionHandler? = thread.uncaughtExceptionHandler

    companion object {
        const val MAX_PLUGIN_THREADS: Int = 32
        private val LIVE = AtomicInteger(0)
        private val HOST_VIEW = ThreadStub()

        @JvmStatic
        fun sleep(millis: Long) {
            Thread.sleep(millis)
        }

        @JvmStatic
        fun sleep(millis: Long, nanos: Int) {
            Thread.sleep(millis, nanos)
        }

        @JvmStatic
        fun yield() {
            Thread.yield()
        }

        @JvmStatic
        fun interrupted(): Boolean = Thread.interrupted()

        @JvmStatic
        fun holdsLock(obj: Any): Boolean = Thread.holdsLock(obj)

        @JvmStatic
        fun dumpStack() {
            Thread.dumpStack()
        }

        @JvmStatic
        fun onSpinWait() {
            Thread.onSpinWait()
        }

        /** Remapped call sites expect [ThreadStub]; host workers fall back to a dormant view. */
        @JvmStatic
        fun currentThread(): ThreadStub = HOST_VIEW

        @JvmStatic
        fun setDefaultUncaughtExceptionHandler(eh: Thread.UncaughtExceptionHandler?) {
            Thread.setDefaultUncaughtExceptionHandler(eh)
        }

        @JvmStatic
        fun getDefaultUncaughtExceptionHandler(): Thread.UncaughtExceptionHandler? =
            Thread.getDefaultUncaughtExceptionHandler()

        @JvmStatic
        fun getAllStackTraces(): Map<Thread, Array<StackTraceElement>> = Thread.getAllStackTraces()
    }
}
