package sidecar

import com.lagradost.cloudstream3.SubtitleFile
import com.lagradost.cloudstream3.utils.ExtractorLink
import com.lagradost.cloudstream3.utils.newExtractorLink
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

/**
 * Race tests for the streaming link search: threads that report links, threads that poll them, and
 * a viewer who cancels, all at once. Each checks something that must hold for every interleaving.
 */
class LinkSearchesTest {
    private fun link(name: String): ExtractorLink = runBlocking { newExtractorLink("test", name, "https://host.test/$name.mp4") }

    private fun searches(deadlineSeconds: Long = 120, source: LinkSource) =
        LinkSearches({ source }, deadlineSeconds)

    /** Polls until the search is over; returns every link, in the order they were received. */
    private fun collect(searches: LinkSearches, id: String, waitMs: Long): Pair<List<String>, LinkSearchUpdate> {
        val names = mutableListOf<String>()
        var from = 0
        var fromSubtitle = 0
        while (true) {
            val update = searches.poll(id, from, fromSubtitle, waitMs)
            names += update.links.map { it.name }
            from = update.nextLink
            fromSubtitle = update.nextSubtitle
            if (update.done) return names to update
        }
    }

    @Test
    fun `every link reaches the caller no matter how closely the search finishes after the last one`() {
        repeat(150) { round ->
            val total = 40
            val searches = searches { _, _, onLink ->
                for (n in 0 until total) {
                    onLink(link("l$n"))
                    if (n % 7 == 0) Thread.yield()
                }
                true
            }
            val id = searches.start("provider", "data")
            val (names, update) = collect(searches, id, waitMs = round.toLong() % 3)
            assertEquals((0 until total).map { "l$it" }, names, "round $round lost or reordered links")
            assertTrue(update.done && update.success)
        }
    }

    /**
     * The window that once lost the last link: the caller has copied the links, then the plugin
     * adds one more and the search ends, then the caller reads whether the search is over.
     */
    @Test
    fun `a link added and a search finished right after the caller copied the links is not reported as over without it`() {
        val release = CountDownLatch(1)
        val finished = CountDownLatch(1)
        val searches = LinkSearches(
            findSource = {
                LinkSource { _, _, onLink ->
                    onLink(link("first"))
                    release.await()
                    onLink(link("last"))
                    finished.countDown()
                    true
                }
            },
            onSnapshotTaken = {
                if (release.count > 0) {
                    release.countDown()
                    finished.await(5, TimeUnit.SECONDS)
                    Thread.sleep(200) // the search marks itself done right after its plugin returns
                }
            },
        )
        val id = searches.start("provider", "data")
        val (names, update) = collect(searches, id, waitMs = 1_000)
        assertEquals(listOf("first", "last"), names, "the last link was lost")
        assertTrue(update.done)
    }

    @Test
    fun `cancelling stops the plugin's own work and not just the wait for it`() {
        val stopped = CountDownLatch(1)
        val ticks = AtomicInteger()
        val searches = searches { _, _, _ ->
            try {
                while (true) {
                    delay(5)
                    ticks.incrementAndGet()
                }
                @Suppress("UNREACHABLE_CODE")
                true
            } finally {
                stopped.countDown()
            }
        }
        val id = searches.start("provider", "data")
        while (ticks.get() < 3) Thread.sleep(2)

        searches.cancel(id)

        assertTrue(stopped.await(5, TimeUnit.SECONDS), "the plugin kept running after the search was cancelled")
        val after = ticks.get()
        Thread.sleep(100)
        assertTrue(ticks.get() - after <= 1, "the plugin was still working after it was stopped")
    }

    @Test
    fun `cancelling at any moment after starting never leaves a plugin running`() {
        val rounds = 200
        val started = AtomicInteger()
        val stopped = AtomicInteger()
        val searches = searches { _, _, _ ->
            started.incrementAndGet()
            try {
                while (true) delay(2)
                @Suppress("UNREACHABLE_CODE")
                true
            } finally {
                stopped.incrementAndGet()
            }
        }
        repeat(rounds) { round ->
            val id = searches.start("provider", "data")
            if (round % 3 == 0) Thread.yield()
            searches.cancel(id)
        }
        // A search cancelled before its plugin began never runs it; every plugin that did begin must end.
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(20)
        while (stopped.get() != started.get() && System.nanoTime() < deadline) Thread.sleep(10)
        assertEquals(started.get(), stopped.get(), "${started.get() - stopped.get()} plugins were still running")
    }

    @Test
    fun `asking about a search that was cancelled or never existed fails clearly`() {
        val searches = searches { _, _, _ -> true }
        assertFailsWith<IllegalStateException> { searches.poll("nothing", 0, 0, 0) }
        val id = searches.start("provider", "data")
        searches.cancel(id)
        assertFailsWith<IllegalStateException> { searches.poll(id, 0, 0, 0) }
    }

    @Test
    fun `a search that outlives its deadline is stopped and keeps what it found`() {
        val stopped = CountDownLatch(1)
        val searches = searches(deadlineSeconds = 1) { _, _, onLink ->
            try {
                onLink(link("found"))
                delay(60_000)
                true
            } finally {
                stopped.countDown()
            }
        }
        val id = searches.start("provider", "data")
        val (names, update) = collect(searches, id, waitMs = 200)
        assertEquals(listOf("found"), names)
        assertTrue(update.done && update.success, "what was found before the deadline counts as a success")
        assertTrue(stopped.await(5, TimeUnit.SECONDS), "the plugin was not stopped at the deadline")
    }

    @Test
    fun `a plugin that fails is reported with its error and the links it found first`() {
        val searches = searches { _, _, onLink ->
            onLink(link("one"))
            onLink(link("two"))
            error("the host exploded")
        }
        val id = searches.start("provider", "data")
        val (names, update) = collect(searches, id, waitMs = 100)
        assertEquals(listOf("one", "two"), names)
        assertTrue(update.done)
        assertTrue(update.failure.orEmpty().contains("the host exploded"), "failure was ${update.failure}")
    }

    @Test
    fun `waiting for news returns as soon as a link arrives and not before`() {
        val searches = searches { _, _, onLink ->
            delay(200)
            onLink(link("late"))
            delay(5_000)
            true
        }
        val id = searches.start("provider", "data")
        val started = System.nanoTime()
        val update = searches.poll(id, 0, 0, waitMs = 3_000)
        val waitedMs = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started)
        assertEquals(listOf("late"), update.links.map { it.name })
        assertTrue(waitedMs in 100..2_500, "returned after $waitedMs ms")
        searches.cancel(id)
    }

    @Test
    fun `many searches at once keep their links apart`() {
        val searches = LinkSearches({ name ->
            LinkSource { _, _, onLink ->
                for (n in 0 until 25) {
                    onLink(link("$name-$n"))
                    if (n % 5 == 0) Thread.yield()
                }
                true
            }
        })
        val results = (0 until 12).map { number ->
            val found = java.util.concurrent.atomic.AtomicReference<List<String>>()
            val thread = Thread {
                val id = searches.start("search$number", "data")
                found.set(collect(searches, id, waitMs = 1).first)
            }
            thread.start()
            number to (thread to found)
        }
        for ((number, pair) in results) {
            pair.first.join(20_000)
            assertEquals((0 until 25).map { "search$number-$it" }, pair.second.get(), "search $number got another search's links")
        }
    }

    @Test
    fun `subtitles are reported with their own cursor`() {
        val searches = searches { _, onSubtitle, onLink ->
            onSubtitle(SubtitleFile("English", "https://host.test/en.srt"))
            onLink(link("a"))
            onSubtitle(SubtitleFile("Arabic", "https://host.test/ar.srt"))
            true
        }
        val id = searches.start("provider", "data")
        var update: LinkSearchUpdate
        val languages = mutableListOf<String>()
        var fromSubtitle = 0
        do {
            update = searches.poll(id, 0, fromSubtitle, 50)
            languages += update.subtitles.map { it.lang }
            fromSubtitle = update.nextSubtitle
        } while (!update.done)
        assertEquals(listOf("English", "Arabic"), languages)
    }
}
