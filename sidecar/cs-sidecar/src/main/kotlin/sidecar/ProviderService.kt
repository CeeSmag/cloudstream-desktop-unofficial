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
import java.util.concurrent.TimeUnit

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

/** The blocking `load_links` call stops once this many seconds have passed and a stream is known. */
private const val LINK_DEADLINE_SECONDS = 30L

/** How long the blocking `load_links` waits for news before it checks the clock again. */
private const val BLOCKING_POLL_MS = 1000L

/** Longest part of a request payload written to the log. */
private const val MAX_LOGGED_DATA = 200

/** Thin, protocol-agnostic facade over the loaded CloudStream providers. */
class ProviderService {
    private val linkSearches = LinkSearches(findSource = { name -> findProvider(name).asLinkSource() })

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
     * The blocking way to get the streams of a title: waits for the whole search. Providers query
     * many hosts and some never answer, so once [LINK_DEADLINE_SECONDS] have passed and at least one
     * stream is known the search is stopped and what was found is returned. The app prefers
     * [startLinkSearch] and [pollLinkSearch], which report streams while the search still runs.
     */
    fun loadLinks(providerName: String, data: String): PlayableLinks {
        SidecarLog.debug { "loadLinks provider=$providerName data=${data.take(MAX_LOGGED_DATA)}" }
        val id = linkSearches.start(providerName, data)
        val links = mutableListOf<ExtractorLink>()
        val subtitles = mutableListOf<SubtitleFile>()
        var fromLink = 0
        var fromSubtitle = 0
        val startedAt = System.nanoTime()
        while (true) {
            val update = linkSearches.poll(id, fromLink, fromSubtitle, BLOCKING_POLL_MS)
            links += update.links
            subtitles += update.subtitles
            fromLink = update.nextLink
            fromSubtitle = update.nextSubtitle
            if (update.done) {
                val failure = update.failure
                if (failure != null && links.isEmpty()) error(failure)
                return PlayableLinks(update.success || links.isNotEmpty(), links, subtitles)
            }
            val waited = TimeUnit.NANOSECONDS.toSeconds(System.nanoTime() - startedAt)
            if (waited >= LINK_DEADLINE_SECONDS && links.isNotEmpty()) {
                linkSearches.cancel(id)
                SidecarLog.warn("$providerName: stopped collecting after ${waited}s; hosts that had not answered were left out")
                return PlayableLinks(true, links, subtitles)
            }
        }
    }

    /** Starts looking for the streams of a title and returns the id of the search at once. */
    fun startLinkSearch(providerName: String, data: String): String {
        SidecarLog.debug { "startLinkSearch provider=$providerName data=${data.take(MAX_LOGGED_DATA)}" }
        return linkSearches.start(providerName, data)
    }

    /** The streams and subtitles search [id] found after the given positions. */
    fun pollLinkSearch(id: String, fromLink: Int, fromSubtitle: Int, waitMs: Long): LinkSearchUpdate =
        linkSearches.poll(id, fromLink, fromSubtitle, waitMs)

    /** Stops search [id]. */
    fun cancelLinkSearch(id: String) = linkSearches.cancel(id)

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
