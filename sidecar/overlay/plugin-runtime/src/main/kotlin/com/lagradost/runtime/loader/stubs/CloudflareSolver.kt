package com.lagradost.runtime.loader.stubs

import com.lagradost.runtime.loader.stubs.CloudflareExchangeFormat.Ask
import com.lagradost.runtime.loader.stubs.CloudflareExchangeFormat.Source
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
 * The app keeps passes from earlier checks and may answer at once with one (`source=saved`). The site can
 * refuse a saved pass (the viewer's IP or the browser changed, or the site raised its protection): that says
 * nothing about this connection, so the request asks again naming the pass (`rejected=`), the app retires it
 * and checks the site in the window. Only a fresh pass from the window that is refused means the site does
 * not accept this connection's cookies at all.
 *
 * File format: [CloudflareExchangeFormat].
 */
object CloudflareSolver {
    /**
     * What passed the check for one origin; [browserOnly] means there are no cookies, only the browser window.
     * [source] says whether the app answered from a saved pass ([passId] names it), [validUntil] when its
     * clearance ends (epoch ms; null when the app did not say).
     */
    data class Clearance(
        val cookies: Map<String, String>,
        val userAgent: String,
        val obtainedAt: Long,
        val browserOnly: Boolean = false,
        val source: Source = Source.BROWSER,
        val passId: String? = null,
        val validUntil: Long? = null,
    ) {
        /** True once its clearance has ended: sending it would only be challenged again. */
        fun isExpired(now: Long): Boolean = validUntil != null && validUntil <= now
    }

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

    /** How many saved passes one request may find refused before it gives up (each is retired by the app). */
    private const val MAX_SAVED_REFUSALS = 3

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

    /**
     * A saved pass the site refused, per origin, until the next question to the app names it. Whichever
     * request asks next (they may share one fresh check) reports it, once, so the app retires that pass.
     */
    private val refusedSaved = ConcurrentHashMap<Origin, String>()

    /** The interceptor to add to every HTTP client plugins use. */
    val interceptor: Interceptor = Interceptor { chain -> intercept(chain) }

    /**
     * Attaches an origin's clearance to the outbound request for that origin, on every hop. It is looked
     * up per hop, never carried on the request, so a redirect to another origin cannot receive it.
     */
    val clearanceScope: Interceptor = Interceptor { chain ->
        val request = chain.request()
        val known = liveClearance(originOf(request.url))
        chain.proceed(if (known != null && !known.browserOnly) withClearance(request, known) else request)
    }

    /** The origin's clearance, unless it has ended (then it is dropped, and the site will be asked again). */
    private fun liveClearance(origin: Origin): Clearance? {
        val known = clearances[origin] ?: return null
        if (!known.isExpired(clock())) return known
        if (clearances.remove(origin, known)) log("INFO", "the pass for ${origin.host} has ended; not sending it")
        return null
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
        clearances[origin]?.takeIf { it.obtainedAt < startedAt }?.let { dropRefused(origin, it) }
        return retryWithClearance(chain, challenged, dir, solve(challenged, startedAt))
    }

    /** The site refused [clearance]: it is dropped, and if it was a saved pass the app hears which. */
    private fun dropRefused(origin: Origin, clearance: Clearance) {
        if (!clearances.remove(origin, clearance) || clearance.source != Source.SAVED) return
        refusedSaved[origin] = clearance.passId ?: CloudflareExchangeFormat.UNNAMED_PASS
        log("WARN", "${origin.host} refused the saved pass ${clearance.passId}; the app will check it again")
    }

    /**
     * Repeats [request] with [first] (and, while the site refuses saved passes, with what the app answers
     * instead). A refused saved pass is named to the app, which retires it and checks the site again; the
     * re-check is asked with the time the refused request was sent, so requests refused at once share it.
     */
    private fun retryWithClearance(chain: Interceptor.Chain, request: Request, dir: File, first: Clearance): Response {
        val origin = originOf(request.url)
        var used = first
        var refusals = 0
        while (true) {
            if (used.browserOnly) return viaBrowserOnly(request, dir)
            val sentAt = clock()
            val retried = chain.proceed(request)
            if (used.source != Source.SAVED || !isChallenge(retried)) return afterRetry(request, dir, retried)
            retried.close()
            dropRefused(origin, used)
            if (++refusals > MAX_SAVED_REFUSALS) break
            used = solve(request, sentAt)
        }
        log("ERROR", "${origin.host} refused $refusals saved passes in a row; giving up on this request")
        throw IOException("Cloudflare verification of ${origin.host} was not completed: the site refused every saved pass")
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
        solve(request, clock(), Ask(needsBrowser = true))
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
    private fun solve(challenged: Request, sentAt: Long, ask: Ask = Ask()): Clearance {
        val origin = originOf(challenged.url)
        val host = origin.host
        refuseIfDeclined(origin)
        val waitingSince = clock()
        synchronized(challengeLocks.computeIfAbsent(origin) { Any() }) {
            liveClearance(origin)?.takeIf { it.obtainedAt >= sentAt }?.let {
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
            val answer = askApp(origin, challenged, ask.copy(rejectedPass = ask.rejectedPass ?: refusedSaved.remove(origin)))
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
    private fun askApp(origin: Origin, challenged: Request, ask: Ask): Answer {
        val dir = exchangeDir ?: return Answer(null, "verification is off", false)
        val host = origin.host
        val url = challenged.url.newBuilder().fragment(null).build()
        val id = UUID.randomUUID().toString()
        val request = File(dir, "request-$id.txt")
        val result = File(dir, "result-$id.txt")
        try {
            dir.mkdirs()
            val pending = File(dir, "request-$id.tmp")
            pending.writeText(CloudflareExchangeFormat.requestText(id, host, url.toString(), challenged.method, ask))
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
            val from = if (clearance.source == Source.SAVED) "a saved pass (${clearance.passId})" else "the browser"
            log("INFO", "the app verified $host after ${tookMs / 1000}s from $from: cookies=${clearance.cookies.keys} userAgent=${clearance.userAgent}")
            return Answer(clearance, "", false)
        }
        return Answer(null, CloudflareExchangeFormat.reason(text), !CloudflareExchangeFormat.isRetry(text))
    }

    /** Reads what the app wrote (see [CloudflareExchangeFormat.parseClearance]), obtained now. */
    fun parseResult(text: String): Clearance? = CloudflareExchangeFormat.parseClearance(text, clock())

    /** Forgets everything learned; used by tests. */
    fun reset() {
        clearances.clear()
        challengeLocks.clear()
        declined.clear()
        retryAnswers.clear()
        refusedSaved.clear()
        browserOnly.clear()
        clock = { System.currentTimeMillis() }
    }
}
