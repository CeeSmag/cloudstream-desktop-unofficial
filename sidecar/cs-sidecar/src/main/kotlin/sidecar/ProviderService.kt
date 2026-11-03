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

/** Thin, protocol-agnostic facade over the loaded CloudStream providers. */
class ProviderService {
    private val linkWorkers = Executors.newCachedThreadPool { task ->
        Thread(task, "load-links").apply { isDaemon = true }
    }

    fun loadExtension(path: String, trusted: Boolean): List<ProviderInfo> {
        val file = File(path)
        require(file.isFile) { "extension file not found: $path" }
        if (trusted) ExtensionLoader.addTrusted(file)
        val before = APIHolder.allProviders.toSet()
        ExtensionLoader.loadAndInit(file)
        return APIHolder.allProviders.filter { it !in before }.map(::describe)
    }

    /** Paginated search (pages start at 1), preferring the newer API and falling back to the legacy one. */
    fun search(providerName: String, query: String, page: Int): SearchPage = runBlocking {
        val provider = findProvider(providerName)
        try {
            val result = provider.search(query, page)
            SearchPage(result?.items.orEmpty(), result?.hasNext ?: false)
        } catch (_: NotImplementedError) {
            SearchPage(provider.search(query).orEmpty(), false)
        }
    }

    fun mainPage(providerName: String, page: Int, entry: MainPageEntry): HomePageResponse? = runBlocking {
        val provider = findProvider(providerName)
        provider.getMainPage(page, MainPageRequest(entry.name, entry.data, entry.horizontalImages))
    }

    fun load(providerName: String, url: String): LoadResponse? = runBlocking {
        findProvider(providerName).load(url)
    }

    /**
     * Collects the streams of one title. Providers query many hosts and some never answer, so
     * once [LINK_DEADLINE_SECONDS] have passed and at least one stream is known, the collection
     * stops and returns what was gathered. Progress is reported to the log every few seconds.
     */
    fun loadLinks(providerName: String, data: String): PlayableLinks {
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
        System.err.println("[sidecar] $providerName: $streams streams in ${total}s (first stream after $first)")
    }

    private fun awaitLinks(collecting: Future<Boolean>, providerName: String, streamsSoFar: () -> Int): Boolean {
        val startedAt = System.nanoTime()
        while (true) {
            try {
                return collecting.get(PROGRESS_SECONDS, TimeUnit.SECONDS)
            } catch (_: TimeoutException) {
                val waited = TimeUnit.NANOSECONDS.toSeconds(System.nanoTime() - startedAt)
                System.err.println("[sidecar] $providerName: still collecting after ${waited}s, ${streamsSoFar()} streams so far")
                if (waited >= LINK_DEADLINE_SECONDS && streamsSoFar() > 0) {
                    collecting.cancel(true)
                    System.err.println("[sidecar] $providerName: stopped collecting; hosts that had not answered were left out")
                    return true
                }
            } catch (failure: ExecutionException) {
                throw failure.cause ?: failure
            }
        }
    }

    /** Resolves a hoster URL through the library's extractor registry. */
    fun extract(url: String, referer: String?): PlayableLinks = runBlocking {
        val links = java.util.Collections.synchronizedList(mutableListOf<ExtractorLink>())
        val subtitles = java.util.Collections.synchronizedList(mutableListOf<SubtitleFile>())
        val matched = com.lagradost.cloudstream3.utils.loadExtractor(url, referer, { subtitles.add(it) }, { links.add(it) })
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
