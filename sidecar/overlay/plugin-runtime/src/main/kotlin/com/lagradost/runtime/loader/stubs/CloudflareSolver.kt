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
 * effective request is repeated with them. Later requests to that destination carry them from the start.
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
 *   viewer's choice, so ask again next time), and one `cookie=name=value` per cookie.
 */
object CloudflareSolver {
    /** What passed the check for one host. */
    data class Clearance(val cookies: Map<String, String>, val userAgent: String, val obtainedAt: Long)

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
    private data class ChallengeKey(val url: HttpUrl, val method: String)
    private data class ClearanceScope(val key: ChallengeKey, val clearance: Clearance)
    private fun key(request: Request) = ChallengeKey(request.url.newBuilder().fragment(null).build(), request.method)

    private val clearances = ConcurrentHashMap<ChallengeKey, Clearance>()
    private val declined = ConcurrentHashMap<ChallengeKey, Declined>()
    private val challengeLocks = ConcurrentHashMap<ChallengeKey, Any>()

    /** Requests routed through their verified browser document when the HTTP client was refused. */
    private val browserOnly = ConcurrentHashMap.newKeySet<ChallengeKey>()

    private data class Declined(val until: Long, val reason: String)

    /**
     * The last "ask again next time" answer per host (the app only wants the viewer's go-ahead) and when
     * it came. Requests that queued for the host while it was being asked share that answer, so one page
     * load with dozens of requests costs one round trip to the app, not one each.
     */
    private val retryAnswers = ConcurrentHashMap<ChallengeKey, Declined>()

    /** The interceptor to add to every HTTP client plugins use. */
    val interceptor: Interceptor = Interceptor { chain -> intercept(chain) }

    /** Apply the browser identity only to the final outbound request for the challenged target. */
    val clearanceScope: Interceptor = Interceptor { chain ->
        val request = chain.request()
        val scope = request.tag(ClearanceScope::class.java)
        chain.proceed(if (scope != null && scope.key == key(request)) withClearance(request, scope.clearance) else request)
    }

    private fun scopeClearance(request: Request, clearance: Clearance): Request =
        request.newBuilder().tag(ClearanceScope::class.java, ClearanceScope(key(request), clearance)).build()

    /** One log line the app picks up by its `[cloudflare]` tag. */
    fun log(level: String, message: String) = System.err.println("[cloudflare] $level $message")

    private fun intercept(chain: Interceptor.Chain): Response {
        val original = chain.request()
        val dir = exchangeDir
        val originalKey = key(original)
        if (dir != null && originalKey in browserOnly) return viaBrowser(original, dir)
        val known = clearances[originalKey]
        val response = chain.proceed(if (known != null) scopeClearance(original, known) else original)
        if (dir == null || !isChallenge(response)) return response
        val challenged = response.request
        val challengedKey = key(challenged)
        log(
            "INFO",
            "challenge from ${challenged.url.host}: ${describe(response)} for ${challenged.method}",
        )
        response.close()
        if (known != null && originalKey == challengedKey) clearances.remove(challengedKey, known)
        val fresh = solve(challengedKey)
        return afterRetry(challenged, dir, chain.proceed(scopeClearance(challenged, fresh)))
    }

    /** The retried request: a good answer is the goal; a challenge again means the cookies alone are not accepted. */
    private fun afterRetry(request: Request, dir: File, retried: Response): Response {
        val host = request.url.host
        if (!isChallenge(retried)) {
            log("INFO", "retried request to $host with the verification cookies: ${describe(retried)}; the site accepted them")
            return retried
        }
        retried.close()
        clearances.remove(key(request))
        log(
            "WARN",
            "$host did not accept the verification cookies over this connection (${describe(retried)}); " +
                "from now on its requests are made by the browser window",
        )
        browserOnly.add(key(request))
        return viaBrowser(request, dir)
    }

    /** Makes [request] in the host's browser window; verifies again once if the window is gone or its pass has expired. */
    private fun viaBrowser(request: Request, dir: File): Response {
        val host = request.url.host
        val first = try {
            BrowserFetch.fetch(request, dir)
        } catch (gone: BrowserUnavailable) {
            log("WARN", "the browser window for $host is gone (${gone.message}); verifying again")
            null
        }
        if (first != null && !isChallenge(first)) return first
        first?.close()
        if (first != null) log("WARN", "the browser got a Cloudflare challenge again for $host; verifying again")
        solve(key(request))
        val second = BrowserFetch.fetch(request, dir)
        if (!isChallenge(second)) return second
        second.close()
        browserOnly.remove(key(request))
        val reason = "Cloudflare still showed its check for $host even in the browser window"
        log("ERROR", reason)
        throw IOException(reason)
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

    /** Requests for the same effective URL and method can share one in-flight answer. */
    private fun solve(key: ChallengeKey): Clearance {
        val host = key.url.host
        refuseIfDeclined(key)
        val waitingSince = clock()
        synchronized(challengeLocks.computeIfAbsent(key) { Any() }) {
            clearances[key]?.takeIf { it.obtainedAt >= waitingSince }?.let {
                log("INFO", "$host was verified while this request waited; using that")
                return it
            }
            refuseIfDeclined(key)
            // Strictly later: a request that starts in the same millisecond as the answer came after it
            // (it did not queue for it), so it asks the app again.
            retryAnswers[key]?.takeIf { it.until > waitingSince }?.let {
                log("INFO", "$host was asked about while this request waited (${it.reason}); failing at once")
                throw IOException("Cloudflare verification of $host was not completed: ${it.reason}")
            }
            val answer = askApp(key)
            val clearance = answer.clearance
            if (clearance == null) {
                if (!answer.remember) retryAnswers[key] = Declined(clock(), answer.reason)
                if (answer.remember) declined[key] = Declined(System.currentTimeMillis() + declineMemoryMs, answer.reason)
                log("WARN", "verification of $host not done: ${answer.reason}" + (if (answer.remember) "; not asking again for ${declineMemoryMs / 1000}s" else ""))
                throw IOException("Cloudflare verification of $host was not completed: ${answer.reason}")
            }
            clearances[key] = clearance
            return clearance
        }
    }

    private fun refuseIfDeclined(key: ChallengeKey) {
        val earlier = declined[key] ?: return
        if (System.currentTimeMillis() >= earlier.until) {
            declined.remove(key, earlier)
            return
        }
        val host = key.url.host
        log("INFO", "$host was not verified a moment ago (${earlier.reason}); failing at once, not asking again yet")
        throw IOException("Cloudflare verification of $host was not completed: ${earlier.reason}")
    }

    /** Writes the request file and waits for the answer. */
    private fun askApp(key: ChallengeKey): Answer {
        val dir = exchangeDir ?: return Answer(null, "verification is off", false)
        val host = key.url.host
        val id = UUID.randomUUID().toString()
        val request = File(dir, "request-$id.txt")
        val result = File(dir, "result-$id.txt")
        try {
            dir.mkdirs()
            val pending = File(dir, "request-$id.tmp")
            pending.writeText("id=$id\nhost=$host\nurl=${key.url}\nmethod=${key.method}\n")
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

    /** Reads what the app wrote; null for `ok=false` or an answer without a cookie or an agent. */
    fun parseResult(text: String): Clearance? {
        val lines = text.lines().filter { it.contains("=") }
        if (lines.none { it.trim() == "ok=true" }) return null
        val cookies = lines.filter { it.startsWith("cookie=") }
            .map { it.removePrefix("cookie=") }
            .filter { it.contains("=") }
            .associate { it.substringBefore("=").trim() to it.substringAfter("=").trim() }
        val agent = lines.firstOrNull { it.startsWith("userAgent=") }?.removePrefix("userAgent=")?.trim().orEmpty()
        if (cookies.isEmpty() || agent.isEmpty()) return null
        return Clearance(cookies, agent, System.currentTimeMillis())
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
