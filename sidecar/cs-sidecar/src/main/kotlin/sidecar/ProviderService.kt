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

/** Thin, protocol-agnostic facade over the loaded CloudStream providers. */
class ProviderService {
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

    fun loadLinks(providerName: String, data: String): PlayableLinks = runBlocking {
        val links = java.util.Collections.synchronizedList(mutableListOf<ExtractorLink>())
        val subtitles = java.util.Collections.synchronizedList(mutableListOf<SubtitleFile>())
        val success = findProvider(providerName).loadLinks(data, false, { subtitles.add(it) }, { links.add(it) })
        PlayableLinks(success, links.toList(), subtitles.toList())
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
