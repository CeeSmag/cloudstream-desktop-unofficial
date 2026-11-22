package com.lagradost.runtime.loader.stubs

import okhttp3.Interceptor
import okhttp3.HttpUrl
import okhttp3.Request
import okhttp3.Response
import java.io.File
import java.io.IOException
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/**
 * Passes Cloudflare's "verify you are human" check for plugins, at the network layer.
 *
 * A request that comes back as a Cloudflare challenge is not given to the plugin. Instead the app is
 * asked, through small files in [exchangeDir], to open a browser window on the site; when the viewer
 * has passed the check there, the app writes the cookies and the browser's `User-Agent` back, and the
 * effective request is repeated with them.
 *
 * What is shared is decided by what owns it. A clearance, a "not verified" answer, the one check in
 * flight and the choice of the browser window all belong to an [Origin] (scheme, host, port), so a page's
 * dozens of requests to different paths of one site cost one verification. The request file still names the
 * exact challenged URL (the window opens on it), and a clearance is attached per outbound hop to its own
 * origin only ([clearanceScope]), so it never reaches a redirect destination.
 *
 * Some sites (AnimePahe among them) do not accept those cookies over this program's own HTTP client:
 * Cloudflare also looks at how the connection is made. For such a site the requests are made by the
 * browser window itself ([BrowserFetch]), which Cloudflare accepts, and the plugin gets the browser's answer.
 *
 * The plugin never sees the challenge, so no plugin has to know about any of this. When the check is not
 * passed the request fails at once with an [IOException] that says why, instead of leaving the plugin waiting.
 *
 * Every step is written to stderr as `[cloudflare] LEVEL message`; the app keeps those lines in the
 * main log and in `cloudflare-logs.txt`, so what happened to a request can be followed from the log.
 *
 * File format (one `key=value` per line, no JSON so that no library is needed):
 * - `request-<id>.txt` written here: `id`, `host`, `url`, `method`.
 * - `result-<id>.txt` written by the app: `ok=true|false`, `userAgent`, `reason`, `retry=true` (not the
 *   viewer's choice, so ask again next time), `transport=browser` (the browser was not challenged, so there
 *   are no cookies to send: the site is reached through the browser window) and one `cookie=name=value`
 *   per cookie.
 */
object CloudflareSolver {
    /** What passed the check for one origin; [browserOnly] means there are no cookies, only the browser window. */
    data class Clearance(
        val cookies: Map<String, String>,
        val userAgent: String,
        val obtainedAt: Long,
        val browserOnly: Boolean = false,
    )

    /** The owner of a clearance: what Cloudflare's cookie and the browser window are valid for. */
    private data class Origin(val scheme: String, val host: String, val port: Int)

    private fun originOf(url: HttpUrl) = Origin(url.scheme, url.host, url.port)

    /** The app's answer to a verification request. */
    private data class Answer(val clearance: Clearance?, val reason: String, val remember: Boolean)

    /** Folder shared with the app; null (the default) turns the whole feature off. */
    @Volatile
    var exchangeDir: File? = null

    /** How long the viewer gets to pass the check before the request is given up on. */
    @Volatile
    var solveTimeoutMs: Long = 4 * 60 * 1000L

    /** How long a host that was not verified is left alone, so a page's many requests do not ask again and again. */
    @Volatile
    var declineMemoryMs: Long = 5 * 60 * 1000L

    /** How often a waiting request says it is still waiting. */
    @Volatile
    var heartbeatMs: Long = 10_000L

    /** The clock that orders requests and answers; tests replace it to make a race exact. */
    @Volatile
    var clock: () -> Long = { System.currentTimeMillis() }

    private const val POLL_MS = 300L

    private val clearances = ConcurrentHashMap<Origin, Clearance>()
    private val declined = ConcurrentHashMap<Origin, Declined>()
    private val challengeLocks = ConcurrentHashMap<Origin, Any>()

    /** Origins whose requests go through their verified browser document: the HTTP client alone is refused. */
    private val browserOnly = ConcurrentHashMap.newKeySet<Origin>()

    private data class Declined(val until: Long, val reason: String)

    /**
     * The last "ask again next time" answer per origin (the app only wants the viewer's go-ahead) and when
     * it came. Requests that queued for the origin while it was being asked share that answer, so one page
     * load with dozens of requests costs one round trip to the app, not one each.
     */
    private val retryAnswers = ConcurrentHashMap<Origin, Declined>()

    /** The interceptor to add to every HTTP client plugins use. */
    val interceptor: Interceptor = Interceptor { chain -> intercept(chain) }

    /**
     * Attaches an origin's clearance to the outbound request for that origin, on every hop. It is looked
     * up per hop, never carried on the request, so a redirect to another origin cannot receive it.
     */
    val clearanceScope: Interceptor = Interceptor { chain ->
        val request = chain.request()
        val known = clearances[originOf(request.url)]
        chain.proceed(if (known != null && !known.browserOnly) withClearance(request, known) else request)
    }

    /** One log line the app picks up by its `[cloudflare]` tag. */
    fun log(level: String, message: String) = System.err.println("[cloudflare] $level $message")

    private fun intercept(chain: Interceptor.Chain): Response {
        val original = chain.request()
        val dir = exchangeDir
        if (dir != null && originOf(original.url) in browserOnly) return viaBrowser(original, dir)
        val startedAt = clock()
        val response = chain.proceed(original)
        if (dir == null || !isChallenge(response)) return response
        val challenged = response.request
        val origin = originOf(challenged.url)
        log("INFO", "challenge from ${origin.host}: ${describe(response)} for ${challenged.method}")
        response.close()
        // A clearance obtained after this request began was not the one it carried, so it is not expired.
        clearances[origin]?.takeIf { it.obtainedAt < startedAt }?.let { clearances.remove(origin, it) }
        val fresh = solve(challenged, startedAt)
        if (fresh.browserOnly) return viaBrowserOnly(challenged, dir)
        return afterRetry(challenged, dir, chain.proceed(challenged))
    }

    /** The browser was not challenged: there are no cookies to send, so the origin is served by its window. */
    private fun viaBrowserOnly(request: Request, dir: File): Response {
        val origin = originOf(request.url)
        log("INFO", "${origin.host} does not challenge the browser, only this connection; its requests are made by the browser window")
        browserOnly.add(origin)
        return viaBrowser(request, dir)
    }

    /** The retried request: a good answer is the goal; a challenge again means the cookies alone are not accepted. */
    private fun afterRetry(request: Request, dir: File, retried: Response): Response {
        val origin = originOf(request.url)
        if (!isChallenge(retried)) {
            log("INFO", "retried request to ${origin.host} with the verification cookies: ${describe(retried)}; the site accepted them")
            return retried
        }
        retried.close()
        clearances.remove(origin)
        log(
            "WARN",
            "${origin.host} did not accept the verification cookies over this connection (${describe(retried)}); " +
                "from now on its requests are made by the browser window",
        )
        browserOnly.add(origin)
        return viaBrowser(request, dir)
    }

    /**
     * Makes [request] in the origin's browser window; verifies again once if the window is gone or its pass
     * has expired. If a fresh verification still leaves no way in, the origin is declined for a while.
     */
    private fun viaBrowser(request: Request, dir: File): Response {
        val origin = originOf(request.url)
        val first = try {
            BrowserFetch.fetch(request, dir)
        } catch (gone: BrowserUnavailable) {
            log("WARN", "the browser window for ${origin.host} is gone (${gone.message}); verifying again")
            null
        }
        if (first != null && !isChallenge(first)) return first
        first?.close()
        if (first != null) log("WARN", "the browser got a Cloudflare challenge again for ${origin.host}; verifying again")
        solve(request, clock())
        val second = try {
            BrowserFetch.fetch(request, dir)
        } catch (gone: BrowserUnavailable) {
            throw unreachable(origin, "no browser window can make its requests (${gone.message})")
        }
        if (!isChallenge(second)) return second
        second.close()
        throw unreachable(origin, "Cloudflare still showed its check even in the browser window")
    }

    /** Neither the cookies nor the browser window get in: stop asking for a while and say so once. */
    private fun unreachable(origin: Origin, why: String): IOException {
        browserOnly.remove(origin)
        clearances.remove(origin)
        val reason = "$why; ${origin.host} cannot be reached"
        declined[origin] = Declined(System.currentTimeMillis() + declineMemoryMs, reason)
        log("ERROR", "$reason; not asking again for ${declineMemoryMs / 1000}s")
        return IOException("Cloudflare verification of ${origin.host} was not completed: $reason")
    }

    /** True when [response] is a Cloudflare challenge page (not a plain block, which no browser can pass). */
    fun isChallenge(response: Response): Boolean =
        response.header("cf-mitigated").equals("challenge", ignoreCase = true)

    private fun describe(response: Response) =
        "status ${response.code}, server=${response.header("server")}, cf-mitigated=${response.header("cf-mitigated")}"

    /** [request] with the cookies and browser identity that passed the check. */
    fun withClearance(request: Request, clearance: Clearance): Request {
        val sent = request.header("Cookie").orEmpty().split(";")
            .map(String::trim)
            .filterNot { "cf_clearance" in clearance.cookies && it.substringBefore("=").trim() == "cf_clearance" }
            .joinToString("; ")
        val already = sent.split(";").map { it.substringBefore("=").trim() }.toSet()
        val extra = clearance.cookies.filterKeys { it !in already }.map { "${it.key}=${it.value}" }
        val cookie = (listOf(sent).filter { it.isNotBlank() } + extra).joinToString("; ")
        val builder = request.newBuilder().header("User-Agent", clearance.userAgent)
        return (if (cookie.isEmpty()) builder else builder.header("Cookie", cookie)).build()
    }

    /**
     * The origin of [challenged] has been verified, asking the app if needed. Requests to one origin share
     * the one check in flight whatever their paths; [sentAt] is when the request began, so a clearance
     * obtained since then is used instead of asking again.
     */
    private fun solve(challenged: Request, sentAt: Long): Clearance {
        val origin = originOf(challenged.url)
        val host = origin.host
        refuseIfDeclined(origin)
        val waitingSince = clock()
        synchronized(challengeLocks.computeIfAbsent(origin) { Any() }) {
            clearances[origin]?.takeIf { it.obtainedAt >= sentAt }?.let {
                log("INFO", "$host was verified while this request waited; using that")
                return it
            }
            refuseIfDeclined(origin)
            // Strictly later: a request that starts in the same millisecond as the answer came after it
            // (it did not queue for it), so it asks the app again.
            retryAnswers[origin]?.takeIf { it.until > waitingSince }?.let {
                log("INFO", "$host was asked about while this request waited (${it.reason}); failing at once")
                throw IOException("Cloudflare verification of $host was not completed: ${it.reason}")
            }
            val answer = askApp(origin, challenged)
            val clearance = answer.clearance
            if (clearance == null) {
                if (!answer.remember) retryAnswers[origin] = Declined(clock(), answer.reason)
                if (answer.remember) declined[origin] = Declined(System.currentTimeMillis() + declineMemoryMs, answer.reason)
                log("WARN", "verification of $host not done: ${answer.reason}" + (if (answer.remember) "; not asking again for ${declineMemoryMs / 1000}s" else ""))
                throw IOException("Cloudflare verification of $host was not completed: ${answer.reason}")
            }
            clearances[origin] = clearance
            return clearance
        }
    }

    private fun refuseIfDeclined(origin: Origin) {
        val earlier = declined[origin] ?: return
        if (System.currentTimeMillis() >= earlier.until) {
            declined.remove(origin, earlier)
            return
        }
        log("INFO", "${origin.host} was not verified a moment ago (${earlier.reason}); failing at once, not asking again yet")
        throw IOException("Cloudflare verification of ${origin.host} was not completed: ${earlier.reason}")
    }

    /** Writes the request file for the exact challenged request and waits for the answer. */
    private fun askApp(origin: Origin, challenged: Request): Answer {
        val dir = exchangeDir ?: return Answer(null, "verification is off", false)
        val host = origin.host
        val url = challenged.url.newBuilder().fragment(null).build()
        val id = UUID.randomUUID().toString()
        val request = File(dir, "request-$id.txt")
        val result = File(dir, "result-$id.txt")
        try {
            dir.mkdirs()
            val pending = File(dir, "request-$id.tmp")
            pending.writeText("id=$id\nhost=$host\nurl=$url\nmethod=${challenged.method}\n")
            if (!pending.renameTo(request)) throw IOException("could not publish Cloudflare verification request $id")
            log("INFO", "asked the app to verify $host (request $id, answer expected within ${solveTimeoutMs / 1000}s)")
            return waitForResult(result, host)
        } finally {
            request.delete()
            result.delete()
        }
    }

    private fun waitForResult(result: File, host: String): Answer {
        val startedAt = System.currentTimeMillis()
        val giveUpAt = startedAt + solveTimeoutMs
        var nextBeat = startedAt + heartbeatMs
        while (System.currentTimeMillis() < giveUpAt) {
            if (result.isFile) return interpret(result.readText(), host, System.currentTimeMillis() - startedAt)
            if (System.currentTimeMillis() >= nextBeat) {
                log("INFO", "still waiting for the viewer to verify $host (${(System.currentTimeMillis() - startedAt) / 1000}s)")
                nextBeat += heartbeatMs
            }
            Thread.sleep(POLL_MS)
        }
        log("WARN", "no answer from the app for $host after ${solveTimeoutMs / 1000}s (is the app's verification watcher running?)")
        return Answer(null, "no answer within ${solveTimeoutMs / 1000}s", true)
    }

    private fun interpret(text: String, host: String, tookMs: Long): Answer {
        val clearance = parseResult(text)
        if (clearance != null) {
            log("INFO", "the app verified $host after ${tookMs / 1000}s: cookies=${clearance.cookies.keys} userAgent=${clearance.userAgent}")
            return Answer(clearance, "", false)
        }
        val lines = text.lines()
        val reason = lines.firstOrNull { it.startsWith("reason=") }?.removePrefix("reason=")?.trim()
            ?: "the app's answer had no usable cookies"
        // "retry=true": the viewer has not said no (the app only wants a go-ahead first), so ask again next time.
        return Answer(null, reason, lines.none { it.trim() == "retry=true" })
    }

    /**
     * Reads what the app wrote; null for `ok=false` or an answer without an agent. Cookies are needed
     * unless the app says `transport=browser` (the browser was not challenged, so it has none to give).
     */
    fun parseResult(text: String): Clearance? {
        val lines = text.lines().filter { it.contains("=") }
        if (lines.none { it.trim() == "ok=true" }) return null
        val cookies = lines.filter { it.startsWith("cookie=") }
            .map { it.removePrefix("cookie=") }
            .filter { it.contains("=") }
            .associate { it.substringBefore("=").trim() to it.substringAfter("=").trim() }
        val agent = lines.firstOrNull { it.startsWith("userAgent=") }?.removePrefix("userAgent=")?.trim().orEmpty()
        val browserOnly = lines.any { it.trim() == "transport=browser" }
        if (agent.isEmpty() || (cookies.isEmpty() && !browserOnly)) return null
        return Clearance(if (browserOnly) emptyMap() else cookies, agent, System.currentTimeMillis(), browserOnly)
    }

    /** Forgets everything learned; used by tests. */
    fun reset() {
        clearances.clear()
        challengeLocks.clear()
        declined.clear()
        retryAnswers.clear()
        browserOnly.clear()
        clock = { System.currentTimeMillis() }
    }
}
