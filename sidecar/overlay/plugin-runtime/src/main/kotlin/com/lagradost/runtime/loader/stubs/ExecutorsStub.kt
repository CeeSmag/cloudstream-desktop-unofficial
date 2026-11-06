package com.lagradost.runtime.loader.stubs

import java.util.concurrent.Callable
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.ThreadFactory
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/**
 * Sandbox wrapper for [java.util.concurrent.Executors].
 *
 * Caps thread counts per pool, bounds task queues, enforces daemon status and normal priority,
 * and allows idle core threads to time out so untrusted plugins cannot starve host CPU or memory.
 */
object ExecutorsStub {
    const val MAX_POOL_THREADS: Int = 4
    const val MAX_QUEUE_CAPACITY: Int = 256
    private const val IDLE_KEEP_ALIVE_SECONDS: Long = 30L
    private val threadCounter = AtomicInteger(0)

    private val INTERCEPTED_METHODS = setOf(
        "newFixedThreadPool:(I)Ljava/util/concurrent/ExecutorService;",
        "newFixedThreadPool:(ILjava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;",
        "newCachedThreadPool:()Ljava/util/concurrent/ExecutorService;",
        "newCachedThreadPool:(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;",
        "newSingleThreadExecutor:()Ljava/util/concurrent/ExecutorService;",
        "newSingleThreadExecutor:(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;",
        "newScheduledThreadPool:(I)Ljava/util/concurrent/ScheduledExecutorService;",
        "newScheduledThreadPool:(ILjava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ScheduledExecutorService;",
        "newSingleThreadScheduledExecutor:()Ljava/util/concurrent/ScheduledExecutorService;",
        "newSingleThreadScheduledExecutor:(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ScheduledExecutorService;",
        "newWorkStealingPool:()Ljava/util/concurrent/ExecutorService;",
        "newWorkStealingPool:(I)Ljava/util/concurrent/ExecutorService;",
        "defaultThreadFactory:()Ljava/util/concurrent/ThreadFactory;",
        "callable:(Ljava/lang/Runnable;)Ljava/util/concurrent/Callable;",
        "callable:(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Callable;",
    )

    @JvmStatic
    fun hasMethod(name: String, descriptor: String): Boolean =
        INTERCEPTED_METHODS.contains("$name:$descriptor")

    @JvmStatic
    fun safeThreadFactory(delegate: ThreadFactory? = null): ThreadFactory = ThreadFactory { runnable ->
        val guarded = Runnable {
            try {
                runnable.run()
            } catch (_: Throwable) {
                // Swallow uncaught plugin runnable exceptions so worker threads do not die noisily
            }
        }
        val thread = try {
            delegate?.newThread(guarded) ?: Thread(guarded)
        } catch (_: Throwable) {
            Thread(guarded)
        }
        thread.isDaemon = true
        if (thread.priority > Thread.NORM_PRIORITY) {
            thread.priority = Thread.NORM_PRIORITY
        }
        if (thread.name.isNullOrBlank() || thread.name.startsWith("Thread-")) {
            thread.name = "cs-plugin-worker-${threadCounter.incrementAndGet()}"
        }
        thread.uncaughtExceptionHandler = Thread.UncaughtExceptionHandler { _, _ -> }
        thread
    }

    @JvmStatic
    fun newFixedThreadPool(nThreads: Int): ExecutorService =
        newFixedThreadPool(nThreads, null)

    @JvmStatic
    fun newFixedThreadPool(nThreads: Int, threadFactory: ThreadFactory?): ExecutorService {
        val clamped = nThreads.coerceIn(1, MAX_POOL_THREADS)
        return ThreadPoolExecutor(
            clamped,
            clamped,
            IDLE_KEEP_ALIVE_SECONDS,
            TimeUnit.SECONDS,
            LinkedBlockingQueue(MAX_QUEUE_CAPACITY),
            safeThreadFactory(threadFactory),
            ThreadPoolExecutor.CallerRunsPolicy(),
        ).apply {
            allowCoreThreadTimeOut(true)
        }
    }

    @JvmStatic
    fun newCachedThreadPool(): ExecutorService =
        newCachedThreadPool(null)

    @JvmStatic
    fun newCachedThreadPool(threadFactory: ThreadFactory?): ExecutorService =
        newFixedThreadPool(MAX_POOL_THREADS, threadFactory)

    @JvmStatic
    fun newSingleThreadExecutor(): ExecutorService =
        newFixedThreadPool(1, null)

    @JvmStatic
    fun newSingleThreadExecutor(threadFactory: ThreadFactory?): ExecutorService =
        newFixedThreadPool(1, threadFactory)

    @JvmStatic
    fun newScheduledThreadPool(corePoolSize: Int): ScheduledExecutorService =
        newScheduledThreadPool(corePoolSize, null)

    @JvmStatic
    fun newScheduledThreadPool(corePoolSize: Int, threadFactory: ThreadFactory?): ScheduledExecutorService {
        val clamped = corePoolSize.coerceIn(1, MAX_POOL_THREADS)
        return ScheduledThreadPoolExecutor(
            clamped,
            safeThreadFactory(threadFactory),
            ThreadPoolExecutor.CallerRunsPolicy(),
        ).apply {
            setKeepAliveTime(IDLE_KEEP_ALIVE_SECONDS, TimeUnit.SECONDS)
            allowCoreThreadTimeOut(true)
            removeOnCancelPolicy = true
        }
    }

    @JvmStatic
    fun newSingleThreadScheduledExecutor(): ScheduledExecutorService =
        newScheduledThreadPool(1, null)

    @JvmStatic
    fun newSingleThreadScheduledExecutor(threadFactory: ThreadFactory?): ScheduledExecutorService =
        newScheduledThreadPool(1, threadFactory)

    @JvmStatic
    fun newWorkStealingPool(): ExecutorService =
        newFixedThreadPool(MAX_POOL_THREADS, null)

    @JvmStatic
    fun newWorkStealingPool(parallelism: Int): ExecutorService =
        newFixedThreadPool(parallelism, null)

    @JvmStatic
    fun defaultThreadFactory(): ThreadFactory =
        safeThreadFactory(null)

    @JvmStatic
    fun callable(task: Runnable): Callable<Any?> =
        Executors.callable(task)

    @JvmStatic
    fun <T> callable(task: Runnable, result: T): Callable<T> =
        Executors.callable(task, result)
}
