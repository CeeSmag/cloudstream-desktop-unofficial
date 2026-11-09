package com.lagradost.runtime.loader.stubs

import okhttp3.Headers
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer
import java.io.File
import java.io.IOException
import java.util.Base64
import java.util.UUID

/** The browser window for a site is gone (the app restarted, or it was closed); verifying again brings it back. */
class BrowserUnavailable(message: String) : IOException(message)

/**
 * Makes a request through the real browser window that passed Cloudflare's check.
 *
 * Cloudflare accepts the window's own requests but not the same cookies sent over Java's HTTP client
 * (it looks at how the connection is made, not only at the cookie). So the request is handed to the
 * app, which runs it with `fetch` inside the window and sends back what the browser received.
 *
 * Files in [CloudflareSolver.exchangeDir], one `key=value` per line:
 * - `fetch-<id>.txt` written here: `id`, `host`, `method`, `url`, `referer`, `header=Name: value` (any number),
 *   `body=<base64>`.
 * - `fetchresult-<id>.txt` written by the app: `ok=true|false`, `reason`, `status`, `url`,
 *   `header=Name: value` (any number), `body=<base64>`; `gone=true` when the window does not exist.
 */
object BrowserFetch {
    /** How long one request may take in the browser. */
    @Volatile
    var timeoutMs: Long = 90_000L

    private const val POLL_MS = 50L

    /** Headers a page may not set on `fetch`; the browser sends its own. */
    private val FORBIDDEN = setOf(
        "user-agent", "cookie", "host", "content-length", "accept-encoding", "connection", "origin",
        "referer", "te", "upgrade", "via", "expect", "date", "dnt", "keep-alive", "transfer-encoding",
    )

    /** What the browser did not decode for us; the body below is already plain. */
    private val DROPPED_RESPONSE_HEADERS = setOf("content-encoding", "content-length", "transfer-encoding")

    /** Runs [request] in the browser window of its host and returns what the browser got. */
    fun fetch(request: Request, dir: File): Response {
        val id = UUID.randomUUID().toString()
        val asked = File(dir, "fetch-$id.txt")
        val answer = File(dir, "fetchresult-$id.txt")
        try {
            dir.mkdirs()
            val pending = File(dir, "fetch-$id.tmp")
            pending.writeText(encode(id, request))
            pending.renameTo(asked)
            CloudflareSolver.log("INFO", "browser fetch ${request.method} ${request.url.host}${request.url.encodedPath} (request $id)")
            return decode(request, waitFor(answer, id))
        } finally {
            asked.delete()
            answer.delete()
        }
    }

    private fun waitFor(answer: File, id: String): String {
        val giveUpAt = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < giveUpAt) {
            if (answer.isFile) return answer.readText()
            Thread.sleep(POLL_MS)
        }
        CloudflareSolver.log("WARN", "browser fetch $id: no answer from the app within ${timeoutMs / 1000}s")
        throw IOException("the browser window did not answer within ${timeoutMs / 1000}s")
    }

    /** The request file's text. */
    fun encode(id: String, request: Request): String {
        val lines = mutableListOf("id=$id", "host=${request.url.host}", "method=${request.method}", "url=${request.url}")
        request.header("Referer")?.let { lines += "referer=${it.replace("\n", "")}" }
        for (index in 0 until request.headers.size) {
            val name = request.headers.name(index)
            if (name.lowercase() in FORBIDDEN || name.lowercase().startsWith("sec-") || name.lowercase().startsWith("proxy-")) continue
            lines += "header=$name: ${request.headers.value(index).replace("\n", " ")}"
        }
        request.body?.let { body ->
            val buffer = Buffer()
            body.writeTo(buffer)
            lines += "body=" + Base64.getEncoder().encodeToString(buffer.readByteArray())
        }
        return lines.joinToString("\n") + "\n"
    }

    /** The response the app's answer describes. */
    fun decode(request: Request, text: String): Response {
        val lines = text.lines()
        fun value(key: String) = lines.firstOrNull { it.startsWith("$key=") }?.removePrefix("$key=")?.trim()
        if (value("ok") != "true") {
            val reason = value("reason") ?: "the browser could not make the request"
            if (value("gone") == "true") throw BrowserUnavailable(reason)
            throw IOException("browser fetch failed: $reason")
        }
        val headers = Headers.Builder()
        lines.filter { it.startsWith("header=") }.map { it.removePrefix("header=") }.forEach { header ->
            val name = header.substringBefore(":").trim()
            if (name.isNotEmpty() && name.lowercase() !in DROPPED_RESPONSE_HEADERS) headers.add(name, header.substringAfter(":").trim())
        }
        val body = Base64.getDecoder().decode(value("body").orEmpty())
        val type = headers.build()["Content-Type"]?.toMediaTypeOrNull()
        return Response.Builder()
            .request(request).protocol(Protocol.HTTP_1_1)
            .code(value("status")?.toIntOrNull() ?: 200).message("")
            .headers(headers.build()).body(body.toResponseBody(type)).build()
    }
}
