package sidecar

import com.fasterxml.jackson.databind.JsonNode
import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.module.kotlin.jacksonObjectMapper
import java.io.PrintStream

private val mapper: ObjectMapper = jacksonObjectMapper()

/** Longest part of a request's parameters written to the log. */
private const val MAX_LOGGED_PARAMS = 300

/** Protocol: one JSON object per line on stdin, one JSON reply per line on stdout. Logs go to stderr. */
fun main() {
    val protocolOut = PrintStream(System.out, true, "UTF-8")
    // Debug output of plugins is dropped here unless the app switched debug logging on.
    System.setErr(PrintStream(DebugLineFilter(System.err), true, "UTF-8"))
    System.setOut(System.err) // plugins may println; keep stdout clean for the protocol
    val service = ProviderService()
    SidecarLog.info("ready")
    generateSequence(::readLine).forEach { line ->
        val reply = handleLine(service, line)
        protocolOut.println(mapper.writeValueAsString(reply))
    }
}

private fun elapsedMillis(startedAt: Long) = (System.nanoTime() - startedAt) / 1_000_000

private fun handleLine(service: ProviderService, line: String): Map<String, Any?> {
    var id: JsonNode? = null
    var method = "?"
    val startedAt = System.nanoTime()
    return try {
        val request = mapper.readTree(line)
        id = request["id"]
        method = request["method"].asText()
        val params = request["params"] ?: mapper.createObjectNode()
        SidecarLog.debug { "request id=$id method=$method params=${params.toString().take(MAX_LOGGED_PARAMS)}" }
        val result = dispatch(service, method, params)
        SidecarLog.debug { "reply id=$id method=$method ok after ${elapsedMillis(startedAt)} ms" }
        mapOf("id" to id, "result" to result)
    } catch (failure: Throwable) {
        SidecarLog.error("request id=$id method=$method failed after ${elapsedMillis(startedAt)} ms: $failure", failure)
        mapOf("id" to id, "error" to mapOf("type" to failure.javaClass.simpleName, "message" to failure.message))
    }
}

private fun dispatch(service: ProviderService, method: String, params: JsonNode): Any? = when (method) {
    "ping" -> "pong"
    "set_debug_logging" -> {
        SidecarLog.debugEnabled = params["enabled"]?.asBoolean() ?: false
        SidecarLog.info("debug logging is now ${if (SidecarLog.debugEnabled) "on" else "off"}")
        null
    }
    "load_extension" -> service.loadExtension(params["path"].asText(), params["trusted"]?.asBoolean() ?: false)
    "search" -> service.search(params["provider"].asText(), params["query"].asText(), params["page"]?.asInt() ?: 1)
    "main_page" -> service.mainPage(
        params["provider"].asText(), params["page"]?.asInt() ?: 1,
        MainPageEntry(params["name"].asText(), params["data"].asText(), params["horizontal"]?.asBoolean() ?: false),
    )
    "load" -> service.load(params["provider"].asText(), params["url"].asText())
    "load_links" -> service.loadLinks(params["provider"].asText(), params["data"].asText())
    "extract" -> service.extract(params["url"].asText(), params["referer"]?.asText())
    "take_added_repositories" -> AddedRepositories.take().map {
        mapOf("name" to it.name, "url" to it.url, "iconUrl" to it.iconUrl)
    }
    else -> error("unknown method: $method")
}
