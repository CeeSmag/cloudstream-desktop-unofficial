package sidecar

import com.lagradost.cloudstream3.MainAPI
import com.lagradost.cloudstream3.SubtitleFile
import com.lagradost.cloudstream3.utils.ExtractorLink
import kotlinx.coroutines.runBlocking
import java.util.Collections
import java.util.UUID
import java.util.concurrent.CancellationException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executors
import java.util.concurrent.Future
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException

/** What a running link search found since the caller last asked, and whether it is over. */
data class LinkSearchUpdate(
    val links: List<ExtractorLink>,
    val subtitles: List<SubtitleFile>,
    /** Pass as `fromLink` next time to get only newer links. */
    val nextLink: Int,
    /** Pass as `fromSubtitle` next time to get only newer subtitles. */
    val nextSubtitle: Int,
    val done: Boolean,
    val success: Boolean,
    val failure: String?,
)

/** How long a link search may run at most; hosts that have not answered by then are left out. */
const val SEARCH_DEADLINE_SECONDS = 120L

/** What a search needs from a provider: look for the streams of a title and report each as found. */
fun interface LinkSource {
    suspend fun loadLinks(
        data: String,
        onSubtitle: (SubtitleFile) -> Unit,
        onLink: (ExtractorLink) -> Unit,
    ): Boolean
}

/** The link source of a provider plugin. */
fun MainAPI.asLinkSource() = LinkSource { data, onSubtitle, onLink ->
    loadLinks(data, false, onSubtitle, onLink)
}

/** A finished search nobody collected is forgotten after this long. */
private const val FORGET_AFTER_MS = 10 * 60 * 1000L

/** One provider's search for the streams of a title: links and subtitles are kept as they arrive. */
private class LinkSearch(val providerName: String) {
    val links: MutableList<ExtractorLink> = Collections.synchronizedList(mutableListOf())
    val subtitles: MutableList<SubtitleFile> = Collections.synchronizedList(mutableListOf())
    val startedAt = System.nanoTime()
    private val monitor = Object()

    @Volatile var done = false
    @Volatile var success = false
    @Volatile var failure: String? = null
    @Volatile var finishedAtMs = 0L

    /** The running `loadLinks` call, so a cancelled search can stop it. */
    @Volatile var task: Future<*>? = null

    /** Wakes a caller that waits for news. */
    fun changed() = synchronized(monitor) { monitor.notifyAll() }

    /** Waits up to [waitMs] for a link or subtitle beyond the given positions, or for the end. */
    fun awaitNews(fromLink: Int, fromSubtitle: Int, waitMs: Long) {
        val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(waitMs)
        synchronized(monitor) {
            while (!done && links.size <= fromLink && subtitles.size <= fromSubtitle) {
                val left = TimeUnit.NANOSECONDS.toMillis(deadline - System.nanoTime())
                if (left <= 0) return
                monitor.wait(left)
            }
        }
    }
}

/**
 * Link searches that report what they find while they run, so the app can start playing the first
 * good stream instead of waiting for every host. Providers query many hosts and some never answer.
 */
class LinkSearches(
    private val findSource: (String) -> LinkSource,
    private val deadlineSeconds: Long = SEARCH_DEADLINE_SECONDS,
    /** Runs right after [poll] copied the links, so a test can make the search move on at that exact point. */
    private val onSnapshotTaken: () -> Unit = {},
) {
    private val searches = ConcurrentHashMap<String, LinkSearch>()
    private val workers = Executors.newCachedThreadPool { task ->
        Thread(task, "link-search").apply { isDaemon = true }
    }

    /** Starts a search and returns its id at once. */
    fun start(providerName: String, data: String): String {
        forgetOldSearches()
        val source = findSource(providerName)
        val id = UUID.randomUUID().toString()
        val search = LinkSearch(providerName)
        searches[id] = search
        SidecarLog.info("$providerName: link search $id started")
        workers.submit { run(search, source, data) }
        return id
    }

    /**
     * What search [id] found after the given positions. Waits up to [waitMs] when there is nothing
     * new yet, so the caller does not have to poll in a tight loop.
     */
    fun poll(id: String, fromLink: Int, fromSubtitle: Int, waitMs: Long): LinkSearchUpdate {
        val search = searches[id] ?: error("unknown link search: $id")
        search.awaitNews(fromLink, fromSubtitle, waitMs)
        // Read before the snapshots: anything added after this was not yet followed by the end.
        val finished = search.done
        val links = synchronized(search.links) { search.links.drop(fromLink) }
        val subtitles = synchronized(search.subtitles) { search.subtitles.drop(fromSubtitle) }
        onSnapshotTaken()
        val update = LinkSearchUpdate(
            links = links,
            subtitles = subtitles,
            nextLink = fromLink + links.size,
            nextSubtitle = fromSubtitle + subtitles.size,
            done = finished,
            success = search.success,
            failure = search.failure,
        )
        if (finished && search.links.size == update.nextLink && search.subtitles.size == update.nextSubtitle) {
            searches.remove(id)
        }
        return update
    }

    /** Stops a search that is no longer wanted (the viewer left); what it found is dropped. */
    fun cancel(id: String) {
        val search = searches.remove(id) ?: return
        SidecarLog.info("${search.providerName}: link search $id cancelled with ${search.links.size} streams found")
        search.done = true
        search.task?.cancel(true)
        search.changed()
    }

    private fun run(search: LinkSearch, source: LinkSource, data: String) {
        val collecting = workers.submit<Boolean> {
            runBlocking {
                source.loadLinks(
                    data,
                    { subtitle -> search.subtitles.add(subtitle); search.changed() },
                    { link -> search.links.add(link); search.changed() },
                )
            }
        }
        search.task = collecting
        // Cancelled before the task was recorded.
        if (search.done) collecting.cancel(true)
        try {
            search.success = collecting.get(deadlineSeconds, TimeUnit.SECONDS)
        } catch (_: CancellationException) {
            SidecarLog.info("${search.providerName}: link search stopped before it finished")
        } catch (_: TimeoutException) {
            collecting.cancel(true)
            search.success = search.links.isNotEmpty()
            SidecarLog.warn("${search.providerName}: stopped searching after ${deadlineSeconds}s; hosts that had not answered were left out")
        } catch (failure: ExecutionException) {
            search.failure = (failure.cause ?: failure).toString()
            SidecarLog.error("${search.providerName}: link search failed: ${search.failure}", failure.cause)
        } finally {
            search.finishedAtMs = System.currentTimeMillis()
            search.done = true
            search.changed()
            val seconds = TimeUnit.NANOSECONDS.toSeconds(System.nanoTime() - search.startedAt)
            SidecarLog.info("${search.providerName}: link search finished: ${search.links.size} streams, ${search.subtitles.size} subtitles in ${seconds}s")
        }
    }

    private fun forgetOldSearches() {
        val now = System.currentTimeMillis()
        searches.entries.removeIf { (_, search) -> search.done && search.finishedAtMs > 0 && now - search.finishedAtMs > FORGET_AFTER_MS }
    }
}
