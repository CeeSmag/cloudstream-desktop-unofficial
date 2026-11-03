package sidecar

import com.lagradost.cloudstream3.APIHolder
import com.lagradost.cloudstream3.MainAPI
import com.lagradost.cloudstream3.HomePageResponse
import com.lagradost.cloudstream3.LoadResponse
import com.lagradost.cloudstream3.MainPageRequest
import com.lagradost.cloudstream3.SearchResponse
import com.lagradost.cloudstream3.SubtitleFile
import com.lagradost.cloudstream3.utils.ExtractorLink
import com.lagradost.runtime.loader.ExtensionLoader
import kotlinx.coroutines.runBlocking
import java.io.File
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executors
import java.util.concurrent.Future
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException
import java.util.concurrent.atomic.AtomicLong

data class SearchPage(val items: List<SearchResponse>, val hasNext: Boolean)

data class MainPageEntry(val name: String, val data: String, val horizontalImages: Boolean)

data class ProviderInfo(
    val name: String,
    val mainUrl: String,
    val lang: String,
    val types: List<String>,
    val hasMainPage: Boolean,
    val mainPage: List<MainPageEntry>,
)

data class PlayableLinks(
    val success: Boolean,
    val links: List<ExtractorLink>,
    val subtitles: List<SubtitleFile>,
)

/** How long link collection may take before it returns what it has. */
private const val LINK_DEADLINE_SECONDS = 30L

/** Marks that no stream has arrived yet. */
private const val NO_LINK_YET = -1L

/** How often the log says that link collection is still going. */
private const val PROGRESS_SECONDS = 5L

/** Longest part of a request payload written to the log. */
private const val MAX_LOGGED_DATA = 200

/** Thin, protocol-agnostic facade over the loaded CloudStream providers. */
class ProviderService {
    private val linkWorkers = Executors.newCachedThreadPool { task ->
        Thread(task, "load-links").apply { isDaemon = true }
    }

    fun loadExtension(path: String, trusted: Boolean): List<ProviderInfo> {
        SidecarLog.debug { "loadExtension path=$path trusted=$trusted" }
        val file = File(path)
        require(file.isFile) { "extension file not found: $path" }
        if (trusted) ExtensionLoader.addTrusted(file)
        val before = APIHolder.allProviders.toSet()
        ExtensionLoader.loadAndInit(file)
        val added = APIHolder.allProviders.filter { it !in before }.map(::describe)
        SidecarLog.info("loaded ${file.name}: providers=${added.map { it.name }}")
        return added
    }

    /** Paginated search (pages start at 1), preferring the newer API and falling back to the legacy one. */
    fun search(providerName: String, query: String, page: Int): SearchPage = runBlocking {
        SidecarLog.debug { "search provider=$providerName query=\"$query\" page=$page" }
        val provider = findProvider(providerName)
        val found = try {
            val result = provider.search(query, page)
            SearchPage(result?.items.orEmpty(), result?.hasNext ?: false)
        } catch (_: NotImplementedError) {
            SidecarLog.debug { "search provider=$providerName has no paged search; using the legacy one" }
            SearchPage(provider.search(query).orEmpty(), false)
        }
        SidecarLog.debug { "search provider=$providerName found ${found.items.size} items, more=${found.hasNext}" }
        found
    }

    fun mainPage(providerName: String, page: Int, entry: MainPageEntry): HomePageResponse? = runBlocking {
        SidecarLog.debug { "mainPage provider=$providerName page=$page row=\"${entry.name}\"" }
        val provider = findProvider(providerName)
        val response = provider.getMainPage(page, MainPageRequest(entry.name, entry.data, entry.horizontalImages))
        SidecarLog.debug { "mainPage provider=$providerName row=\"${entry.name}\" rows=${response?.items?.size ?: 0}" }
        response
    }

    fun load(providerName: String, url: String): LoadResponse? = runBlocking {
        SidecarLog.debug { "load provider=$providerName url=$url" }
        val response = findProvider(providerName).load(url)
        SidecarLog.debug { "load provider=$providerName url=$url found=${response != null}" }
        response
    }

    /**
     * Collects the streams of one title. Providers query many hosts and some never answer, so
     * once [LINK_DEADLINE_SECONDS] have passed and at least one stream is known, the collection
     * stops and returns what was gathered. Progress is reported to the log every few seconds.
     */
    fun loadLinks(providerName: String, data: String): PlayableLinks {
        SidecarLog.debug { "loadLinks provider=$providerName data=${data.take(MAX_LOGGED_DATA)}" }
        val links = java.util.Collections.synchronizedList(mutableListOf<ExtractorLink>())
        val subtitles = java.util.Collections.synchronizedList(mutableListOf<SubtitleFile>())
        val provider = findProvider(providerName)
        val startedAt = System.nanoTime()
        val firstLinkAt = AtomicLong(NO_LINK_YET)
        val collecting = linkWorkers.submit<Boolean> {
            runBlocking {
                provider.loadLinks(data, false, { subtitles.add(it) }, {
                    firstLinkAt.compareAndSet(NO_LINK_YET, System.nanoTime())
                    links.add(it)
                })
            }
        }
        val success = awaitLinks(collecting, providerName) { links.size }
        logLinkSummary(providerName, startedAt, firstLinkAt.get(), links.size)
        return PlayableLinks(success, links.toList(), subtitles.toList())
    }

    private fun logLinkSummary(providerName: String, startedAt: Long, firstLinkAt: Long, streams: Int) {
        val total = TimeUnit.NANOSECONDS.toSeconds(System.nanoTime() - startedAt)
        val first = if (firstLinkAt == NO_LINK_YET) "never" else "${TimeUnit.NANOSECONDS.toSeconds(firstLinkAt - startedAt)}s"
        SidecarLog.info("$providerName: $streams streams in ${total}s (first stream after $first)")
    }

    private fun awaitLinks(collecting: Future<Boolean>, providerName: String, streamsSoFar: () -> Int): Boolean {
        val startedAt = System.nanoTime()
        while (true) {
            try {
                return collecting.get(PROGRESS_SECONDS, TimeUnit.SECONDS)
            } catch (_: TimeoutException) {
                val waited = TimeUnit.NANOSECONDS.toSeconds(System.nanoTime() - startedAt)
                SidecarLog.debug { "$providerName: still collecting after ${waited}s, ${streamsSoFar()} streams so far" }
                if (waited >= LINK_DEADLINE_SECONDS && streamsSoFar() > 0) {
                    collecting.cancel(true)
                    SidecarLog.warn("$providerName: stopped collecting after ${waited}s; hosts that had not answered were left out")
                    return true
                }
            } catch (failure: ExecutionException) {
                throw failure.cause ?: failure
            }
        }
    }

    /** Resolves a hoster URL through the library's extractor registry. */
    fun extract(url: String, referer: String?): PlayableLinks = runBlocking {
        SidecarLog.debug { "extract url=$url referer=$referer" }
        val links = java.util.Collections.synchronizedList(mutableListOf<ExtractorLink>())
        val subtitles = java.util.Collections.synchronizedList(mutableListOf<SubtitleFile>())
        val matched = com.lagradost.cloudstream3.utils.loadExtractor(url, referer, { subtitles.add(it) }, { links.add(it) })
        SidecarLog.debug { "extract url=$url matched=$matched streams=${links.size}" }
        PlayableLinks(matched, links.toList(), subtitles.toList())
    }

    private fun findProvider(name: String): MainAPI =
        APIHolder.allProviders.firstOrNull { it.name == name }
            ?: error("provider not loaded: $name")

    private fun describe(api: MainAPI) =
        ProviderInfo(
            api.name, api.mainUrl, api.lang, api.supportedTypes.map { it.name }, api.hasMainPage,
            api.mainPage.map { MainPageEntry(it.name, it.data, it.horizontalImages) },
        )
}
