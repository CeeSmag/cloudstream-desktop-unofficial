@file:OptIn(ExperimentalWasmInterop::class)

import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlin.wasm.WasmExport
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.delay

@Serializable
data class SearchResult(val title: String, val url: String)

/** Provider-shaped entry point: query in, JSON array out, count returned (-1 on handled failure). */
@WasmExport("plugin_search")
fun pluginSearch(): Int {
    val query = readUtf8(4096) { p, c -> hostReadInput(p, c) } ?: return -1
    return try {
        val page = httpGetDocument("https://mock.test/search?q=$query")
        val results = page.select("div.item a.title").map { SearchResult(it.text(), it.attr("href")) }
        withUtf8(Json.encodeToString(results)) { p, l -> hostWriteOutput(p, l) }
        results.size
    } catch (failure: HostRequestFailed) {
        withUtf8("error: ${failure.message}") { p, l -> hostWriteOutput(p, l) }
        -1
    }
}

/** Same shape as MainAPI.search: a suspend function that runs to completion because host calls are synchronous. */
suspend fun suspendSearch(query: String): Int {
    val page = httpGetDocument("https://mock.test/search?q=$query")
    return page.select("div.item a.title").size
}

@OptIn(kotlinx.coroutines.DelicateCoroutinesApi::class)
@WasmExport("plugin_suspend_search")
fun pluginSuspendSearch(): Int {
    var count = -2
    GlobalScope.launch(Dispatchers.Unconfined, CoroutineStart.UNDISPATCHED) { count = suspendSearch("naruto") }
    return count
}

/** Deliberately throws so the host can prove one bad plugin call cannot take the app down. */
@WasmExport("plugin_crash")
fun pluginCrash(): Int = throw IllegalStateException("simulated plugin bug")

fun main() {}
