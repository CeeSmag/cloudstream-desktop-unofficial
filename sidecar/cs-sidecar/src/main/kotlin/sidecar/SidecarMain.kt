package sidecar

import com.fasterxml.jackson.databind.JsonNode
import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.module.kotlin.jacksonObjectMapper
import java.io.PrintStream

private val mapper: ObjectMapper = jacksonObjectMapper()

/** Protocol: one JSON object per line on stdin, one JSON reply per line on stdout. Logs go to stderr. */
fun main() {
    val protocolOut = PrintStream(System.out, true, "UTF-8")
    System.setOut(System.err) // plugins may println; keep stdout clean for the protocol
    val service = ProviderService()
    System.err.println("[sidecar] ready")
    generateSequence(::readLine).forEach { line ->
        val reply = handleLine(service, line)
        protocolOut.println(mapper.writeValueAsString(reply))
    }
}

private fun handleLine(service: ProviderService, line: String): Map<String, Any?> {
    var id: JsonNode? = null
    return try {
        val request = mapper.readTree(line)
        id = request["id"]
        val params = request["params"] ?: mapper.createObjectNode()
        mapOf("id" to id, "result" to dispatch(service, request["method"].asText(), params))
    } catch (failure: Throwable) {
        System.err.println("[sidecar] request failed: $failure")
        mapOf("id" to id, "error" to mapOf("type" to failure.javaClass.simpleName, "message" to failure.message))
    }
}

private fun dispatch(service: ProviderService, method: String, params: JsonNode): Any? = when (method) {
    "ping" -> "pong"
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
