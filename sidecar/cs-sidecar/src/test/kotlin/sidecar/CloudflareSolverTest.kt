package sidecar

import com.lagradost.runtime.loader.stubs.BrowserFetch
import com.lagradost.runtime.loader.stubs.CloudflareSolver
import com.lagradost.runtime.loader.stubs.URLStub
import com.sun.net.httpserver.HttpServer
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.ResponseBody
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer
import okio.BufferedSource
import java.io.File
import java.io.IOException
import java.net.InetSocketAddress
import java.nio.file.Files
import java.util.Base64
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.concurrent.thread
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Cloudflare verification at the network layer: what counts as a challenge, how the answer is sent
 * with later requests, the file exchange with the app (played here by a thread), the browser window
 * that makes the requests for sites that do not accept the cookies, and the race where many requests
 * to one host arrive at once and must cost the viewer one verification, not many.
 */
class CloudflareSolverTest {
    private lateinit var exchange: File

    @BeforeTest
    fun setUp() {
        exchange = Files.createTempDirectory("cs_cf_test").toFile()
        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        CloudflareSolver.solveTimeoutMs = 10_000
        CloudflareSolver.declineMemoryMs = 60_000
        BrowserFetch.timeoutMs = 3_000
    }

    @AfterTest
    fun tearDown() {
        CloudflareSolver.exchangeDir = null
        CloudflareSolver.reset()
        exchange.deleteRecursively()
    }

    private fun response(code: Int, headers: Map<String, String> = emptyMap(), body: String = ""): Response =
        Response.Builder()
            .request(Request.Builder().url("https://site.test/").build())
            .protocol(Protocol.HTTP_1_1).code(code).message("m")
            .apply { headers.forEach { (name, value) -> header(name, value) } }
            .body(body.toResponseBody()).build()

    private val passed = "ok=true\nuserAgent=Agent/9\ncookie=cf_clearance=ok\n"
    private val browserAnswer = "ok=true\nstatus=200\nheader=Content-Type: text/plain\nbody=" +
        Base64.getEncoder().encodeToString("from the browser".toByteArray()) + "\n"

    @Test
    fun `a Cloudflare challenge page is recognised and a plain block or a good answer is not`() {
        val cloudflare = mapOf("server" to "cloudflare")
        assertFalse(CloudflareSolver.isChallenge(response(403, cloudflare, "<title>Just a moment...</title>")))
        assertFalse(CloudflareSolver.isChallenge(response(503, cloudflare, "x challenge-platform x")))
        assertTrue(CloudflareSolver.isChallenge(response(200, mapOf("cf-mitigated" to "challenge"))))
        assertFalse(CloudflareSolver.isChallenge(response(403, cloudflare, "Access denied")))
        assertFalse(CloudflareSolver.isChallenge(response(403, emptyMap(), "Just a moment")))
        assertFalse(CloudflareSolver.isChallenge(response(200, cloudflare, "Just a moment")))
    }

    @Test
    fun `the answer is sent as cookies and browser identity without repeating a cookie the plugin already sends`() {
        val clearance = CloudflareSolver.Clearance(mapOf("cf_clearance" to "abc", "other" to "1"), "Agent/1", 0)
        val plain = Request.Builder().url("https://site.test/a").header("Cookie", "other=2; mine=3").build()
        val sent = CloudflareSolver.withClearance(plain, clearance)
        assertEquals("Agent/1", sent.header("User-Agent"))
        assertEquals("other=2; mine=3; cf_clearance=abc", sent.header("Cookie"))
        assertEquals("cf_clearance=abc; other=1", CloudflareSolver.withClearance(Request.Builder().url("https://s.test/").build(), clearance).header("Cookie"))
    }

    @Test
    fun `an answer needs success, a cookie and an agent`() {
        val good = CloudflareSolver.parseResult("ok=true\nuserAgent=Agent/2\ncookie=cf_clearance=x=y\ncookie=a=b\n")
        assertEquals(mapOf("cf_clearance" to "x=y", "a" to "b"), good?.cookies)
        assertEquals("Agent/2", good?.userAgent)
        assertNull(CloudflareSolver.parseResult("ok=false\nreason=closed\n"))
        assertNull(CloudflareSolver.parseResult("ok=true\nuserAgent=Agent/2\n"))
        assertNull(CloudflareSolver.parseResult("ok=true\ncookie=a=b\n"))
    }

    /**
     * The app. `verifications` are the answers to verification requests and `fetches` the answers to
     * browser-fetch requests, each used in turn (the last one for every later request); it counts both.
     */
    private class FakeApp(
        val exchange: File,
        val verifications: List<String>,
        val fetches: List<String> = emptyList(),
        val delayMs: Long = 100,
    ) {
        val verified = AtomicInteger()
        val fetched = AtomicInteger()
        /** The text of every verification request, in the order they were answered. */
        val requests = CopyOnWriteArrayList<String>()
        private val handled = mutableSetOf<String>()
        private val worker = thread(isDaemon = true) {
            while (!Thread.currentThread().isInterrupted) {
                exchange.listFiles()?.forEach { file -> answer(file) }
                Thread.sleep(20)
            }
        }

        private fun answer(file: File) {
            val name = file.name
            val kind = when {
                name.startsWith("request-") && name.endsWith(".txt") -> "request-" to "result-"
                name.startsWith("fetch-") && name.endsWith(".txt") -> "fetch-" to "fetchresult-"
                else -> return
            }
            if (!handled.add(name)) return
            if (kind.first == "request-") requests.add(runCatching { file.readText() }.getOrDefault(""))
            val counter = if (kind.first == "request-") verified else fetched
            val answers = if (kind.first == "request-") verifications else fetches
            val text = answers[minOf(counter.getAndIncrement(), answers.size - 1)]
            Thread.sleep(delayMs)
            File(exchange, kind.second + name.removePrefix(kind.first)).writeText(text)
        }

        fun stop() = worker.interrupt()
    }

    /** A site behind Cloudflare: 200 once the request carries cf_clearance=ok, else the challenge. */
    private fun client(): OkHttpClient = OkHttpClient.Builder()
        .addInterceptor(CloudflareSolver.interceptor)
        .addInterceptor(CloudflareSolver.clearanceScope)
        .addInterceptor { chain ->
            val accepted = chain.request().header("Cookie").orEmpty().contains("cf_clearance=ok")
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                .code(if (accepted) 200 else 403).message("m").header("server", "cloudflare")
                .apply { if (!accepted) header("cf-mitigated", "challenge") }
                .body((if (accepted) "content" else "Just a moment...").toResponseBody()).build()
        }.build()

    /** A site that never accepts the cookies, only its own browser. */
    private fun pickyClient(): OkHttpClient = OkHttpClient.Builder()
        .addInterceptor(CloudflareSolver.interceptor)
        .addInterceptor { chain ->
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(403).message("m")
                .header("server", "cloudflare").header("cf-mitigated", "challenge").body("Just a moment...".toResponseBody()).build()
        }.build()

    private fun get(client: OkHttpClient, host: String = "site.test", path: String = "/api"): Pair<Int, String> =
        client.newCall(Request.Builder().url("https://$host$path").build()).execute().use { it.code to it.body.string() }

    @Test
    fun `a challenge is passed through the app and the request repeated`() {
        val app = FakeApp(exchange, listOf(passed))
        val (code, body) = get(client())
        app.stop()
        assertEquals(200 to "content", code to body)
        assertEquals(1, app.verified.get())
        // Later requests carry the cookie from the start: no second verification.
        assertEquals(200, get(client()).first)
        assertEquals(1, app.verified.get())
        assertTrue(exchange.listFiles().orEmpty().isEmpty(), "request and result files are cleaned up")
    }

    @Test
    fun `a redirected challenge sends the effective request and retries only that target`() {
        val origin = "https://start.test/out"
        val target = "https://destination.test/path?token=fixture"
        val app = Thread {
            val stop = System.nanoTime() + 5_000_000_000
            while (System.nanoTime() < stop) {
                val request = exchange.listFiles().orEmpty().firstOrNull { it.name.startsWith("request-") && it.extension == "txt" }
                if (request != null) {
                    request.copyTo(File(exchange, "observed.txt"))
                    File(exchange, request.name.replaceFirst("request-", "result-")).writeText(passed)
                    return@Thread
                }
                Thread.yield()
            }
        }
        val client = OkHttpClient.Builder().addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope).addInterceptor { chain ->
            val request = chain.request()
            val effective = if (request.url.toString() == origin) request.newBuilder().url(target).build() else request
            val accepted = request.url.toString() == target && request.header("Cookie") == "cf_clearance=ok"
            Response.Builder().request(effective).protocol(Protocol.HTTP_1_1)
                .code(if (accepted) 200 else 403).message("fixture")
                .apply { if (!accepted) header("cf-mitigated", "challenge") }
                .body("fixture".toResponseBody()).build()
        }.build()
        app.start()
        try {
            val result = client.newCall(Request.Builder().url(origin).build()).execute().use { it.code }
            assertEquals(200, result)
            val message = File(exchange, "observed.txt").readLines()
            assertTrue("host=destination.test" in message)
            assertTrue("method=GET" in message)
            assertTrue("url=$target" in message)
        } finally {
            app.join(6_000)
        }
    }

    @Test
    fun `verified browser credentials are scoped to the challenged hop of a real redirect`() {
        val origin = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        val landingCookie = AtomicReference<String?>("landing not reached")
        origin.createContext("/entry") { request ->
            if (request.requestHeaders.getFirst("Cookie") == "cf_clearance=ok") {
                request.responseHeaders.set("Location", "http://localhost:${origin.address.port}/landing")
                request.sendResponseHeaders(302, -1)
            } else {
                request.responseHeaders.set("cf-mitigated", "challenge")
                request.sendResponseHeaders(403, -1)
            }
            request.close()
        }
        origin.createContext("/landing") { request ->
            landingCookie.set(request.requestHeaders.getFirst("Cookie"))
            request.sendResponseHeaders(200, -1)
            request.close()
        }
        val app = FakeApp(exchange, listOf(passed))
        val client = OkHttpClient.Builder().addInterceptor(CloudflareSolver.interceptor)
            .addNetworkInterceptor(CloudflareSolver.clearanceScope).build()
        origin.start()
        try {
            val entry = Request.Builder().url("http://127.0.0.1:${origin.address.port}/entry").build()
            assertEquals(200, client.newCall(entry).execute().use { it.code })
            assertEquals(1, app.verified.get())
            assertNull(landingCookie.get())
        } finally {
            origin.stop(0)
            app.stop()
        }
    }

    @Test
    fun `a viewer who does not verify makes the request fail at once with the reason, and the host is left alone for a while`() {
        val app = FakeApp(exchange, listOf("ok=false\nreason=the window was closed\n"))
        val first = assertFailsWith<IOException> { get(client()) }
        assertTrue(first.message.orEmpty().contains("the window was closed"), first.message)
        assertFailsWith<IOException> { get(client()) }
        app.stop()
        assertEquals(1, app.verified.get(), "the second request must not ask again")
    }

    @Test
    fun `a refusal that is not the viewer's choice is asked again next time`() {
        val app = FakeApp(exchange, listOf("ok=false\nreason=waiting for the go-ahead\nretry=true\n", passed))
        assertFailsWith<IOException> { get(client()) }
        assertEquals(200, get(client()).first, "the second request asks again and succeeds")
        app.stop()
        assertEquals(2, app.verified.get())
    }

    @Test
    fun `a request that starts in the same millisecond as a refusal is asked again, not refused with the old answer`() {
        CloudflareSolver.clock = { 1_000L } // time stands still: the worst case of the race
        val app = FakeApp(exchange, listOf("ok=false\nreason=waiting for the go-ahead\nretry=true\n", passed))
        assertFailsWith<IOException> { get(client()) }
        assertEquals(200, get(client()).first, "the second request came after the refusal, so it asks again")
        app.stop()
        assertEquals(2, app.verified.get())
    }

    @Test
    fun `requests queued while the app is asked for a go-ahead share that answer`() {
        val app = FakeApp(exchange, listOf("ok=false\nreason=waiting for the go-ahead\nretry=true\n"))
        val shared = client()
        val failures = CopyOnWriteArrayList<String>()
        val start = CountDownLatch(1)
        val log = lines {
            val workers = (1..12).map {
                thread { start.await(); runCatching { get(shared) }.onFailure { failures.add(it.message.orEmpty()) } }
            }
            start.countDown()
            workers.forEach { it.join(20_000) }
        }
        app.stop()
        assertTrue(
            "[cloudflare] INFO site.test was asked about while this request waited (waiting for the go-ahead); failing at once" in log,
            "$log",
        )
        assertEquals(12, failures.size, "every request fails with the reason")
        assertTrue(failures.all { it.contains("waiting for the go-ahead") }, "$failures")
        assertTrue(app.verified.get() < 12, "not one round trip per request: ${app.verified.get()}")
    }

    @Test
    fun `no answer from the app within the time fails the request instead of waiting for ever`() {
        CloudflareSolver.solveTimeoutMs = 700
        val error = assertFailsWith<IOException> { get(client()) }
        assertTrue(error.message.orEmpty().contains("no answer"), error.message)
    }

    @Test
    fun `without an exchange folder challenges are left to the plugin`() {
        CloudflareSolver.exchangeDir = null
        assertEquals(403, get(client()).first)
    }

    @Test
    fun `a site that does not accept the cookies is reached through the browser window from then on`() {
        val app = FakeApp(exchange, listOf(passed), listOf(browserAnswer))
        assertEquals(200 to "from the browser", get(pickyClient()).let { it.first to it.second })
        assertEquals(1, app.verified.get())
        assertEquals(1, app.fetched.get())
        // The next request goes straight to the browser: no verification, no attempt over this connection.
        assertEquals("from the browser", get(pickyClient()).second)
        app.stop()
        assertEquals(1, app.verified.get(), "no second verification")
        assertEquals(2, app.fetched.get())
    }

    @Test
    fun `when the browser window is gone the site is verified again and the request made again`() {
        val app = FakeApp(exchange, listOf(passed), listOf("ok=false\ngone=true\nreason=no window for site.test\n", browserAnswer))
        assertEquals("from the browser", get(pickyClient()).second)
        app.stop()
        assertEquals(2, app.verified.get(), "verified once for the cookies, once more for the missing window")
        assertTrue(app.fetched.get() >= 2)
    }

    @Test
    fun `a challenge that even the browser window gets is reported, not repeated for ever`() {
        val challenge = "ok=true\nstatus=403\nheader=server: cloudflare\nheader=cf-mitigated: challenge\nbody=" +
            Base64.getEncoder().encodeToString("Just a moment...".toByteArray()) + "\n"
        val app = FakeApp(exchange, listOf(passed), listOf(challenge))
        val error = assertFailsWith<IOException> { get(pickyClient()) }
        app.stop()
        assertTrue(error.message.orEmpty().contains("even in the browser window"), error.message)
    }

    @Test
    fun `a browser fetch request leaves out what a page may not set and carries the body`() {
        val request = Request.Builder().url("https://site.test/a?x=1")
            .header("User-Agent", "X").header("Cookie", "a=b").header("sec-ch-ua", "x").header("Accept", "text/html")
            .header("X-Requested-With", "XMLHttpRequest").header("Referer", "https://site.test/home")
            .post("a=1".toRequestBody()).build()
        val text = BrowserFetch.encode("id1", request)
        assertTrue(text.contains("method=POST") && text.contains("url=https://site.test/a?x=1"))
        assertTrue(text.contains("referer=https://site.test/home"))
        assertTrue(text.contains("header=Accept: text/html") && text.contains("header=X-Requested-With: XMLHttpRequest"))
        assertFalse(text.contains("User-Agent") || text.contains("Cookie") || text.contains("sec-ch-ua"), text)
        assertTrue(text.contains("body=" + Base64.getEncoder().encodeToString("a=1".toByteArray())))
    }

    @Test
    fun `the browser's answer becomes a response with the body already decoded`() {
        val request = Request.Builder().url("https://site.test/a").build()
        val text = "ok=true\nstatus=201\nheader=Content-Type: application/json\nheader=Content-Encoding: br\nheader=Content-Length: 9\nbody=" +
            Base64.getEncoder().encodeToString("{\"a\":1}".toByteArray()) + "\n"
        BrowserFetch.decode(request, text).use { reply ->
            assertEquals(201, reply.code)
            assertEquals("{\"a\":1}", reply.body.string())
            assertNull(reply.header("Content-Encoding"), "the browser already decoded the body")
            assertEquals("application/json", reply.header("Content-Type"))
        }
    }

    @Test
    fun `many requests to one host at once cost one verification`() {
        val app = FakeApp(exchange, listOf(passed))
        val shared = client()
        val results = CopyOnWriteArrayList<Int>()
        val start = CountDownLatch(1)
        val log = lines {
            val workers = (1..12).map { thread { start.await(); results.add(get(shared).first) } }
            start.countDown()
            workers.forEach { it.join(20_000) }
        }
        app.stop()
        assertTrue("[cloudflare] INFO site.test was verified while this request waited; using that" in log, "$log")
        assertEquals(12, results.size, "every request finishes")
        assertTrue(results.all { it == 200 }, "every request ends with the content: $results")
        assertEquals(1, app.verified.get(), "one verification for all of them")
    }

    @Test
    fun `two hosts are verified separately`() {
        val app = FakeApp(exchange, listOf(passed))
        val shared = client()
        assertEquals(200, get(shared, "one.test").first)
        assertEquals(200, get(shared, "two.test").first)
        app.stop()
        assertEquals(2, app.verified.get())
    }

    @Test
    fun `many requests to different paths of one host cost one verification`() {
        val app = FakeApp(exchange, listOf(passed))
        val shared = client()
        val results = CopyOnWriteArrayList<Int>()
        val start = CountDownLatch(1)
        val workers = (1..12).map { n -> thread { start.await(); results.add(get(shared, path = "/file-$n").first) } }
        start.countDown()
        workers.forEach { it.join(20_000) }
        app.stop()
        assertEquals(12, results.size, "every request finishes")
        assertTrue(results.all { it == 200 }, "every request ends with the content: $results")
        assertEquals(1, app.verified.get(), "one verification for all of them, whatever their paths")
    }

    @Test
    fun `a clearance is used for other paths of its origin without asking again`() {
        val app = FakeApp(exchange, listOf(passed))
        val shared = client()
        assertEquals(200, get(shared, path = "/first").first)
        assertEquals(200, get(shared, path = "/second").first)
        assertEquals(200, get(shared, path = "/third?x=1").first)
        app.stop()
        assertEquals(1, app.verified.get())
    }

    @Test
    fun `concurrent requests to several origins cost one verification each`() {
        val app = FakeApp(exchange, listOf(passed))
        val shared = client()
        val start = CountDownLatch(1)
        val workers = (1..18).map { n ->
            thread { start.await(); get(shared, host = "site${n % 3}.test", path = "/p$n") }
        }
        start.countDown()
        workers.forEach { it.join(20_000) }
        app.stop()
        assertEquals(3, app.verified.get(), "one per origin")
    }

    @Test
    fun `an answer that says the browser was not challenged skips the doomed cookie retry`() {
        val app = FakeApp(exchange, listOf("ok=true\nuserAgent=Agent/9\ntransport=browser\n"), listOf(browserAnswer))
        val overConnection = AtomicInteger()
        val client = OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope)
            .addInterceptor { chain ->
                overConnection.incrementAndGet()
                Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(403).message("m")
                    .header("server", "cloudflare").header("cf-mitigated", "challenge")
                    .body("Just a moment...".toResponseBody()).build()
            }.build()
        assertEquals("from the browser", get(client, path = "/one").second)
        assertEquals("from the browser", get(client, path = "/two").second)
        app.stop()
        assertEquals(1, app.verified.get())
        assertEquals(1, overConnection.get(), "only the first request is made over this connection")
        assertEquals(2, app.fetched.get())
    }

    @Test
    fun `an origin that neither the cookies nor the browser window can reach is declined once`() {
        val gone = "ok=false\ngone=true\nreason=no window for site.test\n"
        val app = FakeApp(exchange, listOf(passed), listOf(gone))
        val first = assertFailsWith<IOException> { get(pickyClient(), path = "/one") }
        val verifiedAfterFirst = app.verified.get()
        val fetchedAfterFirst = app.fetched.get()
        val later = (2..6).map { n -> assertFailsWith<IOException> { get(pickyClient(), path = "/page-$n") } }
        app.stop()
        assertTrue(first.message.orEmpty().contains("cannot be reached"), first.message)
        assertTrue(later.all { it.message.orEmpty().contains("cannot be reached") }, "later requests say why: $later")
        assertEquals(verifiedAfterFirst, app.verified.get(), "later requests make no round trip to the app")
        assertEquals(fetchedAfterFirst, app.fetched.get(), "and no browser fetch")
    }

    @Test
    fun `a clearance is never sent to an origin other than its own`() {
        val app = FakeApp(exchange, listOf(passed))
        val shared = client()
        assertEquals(200, get(shared, host = "one.test", path = "/a").first)
        val seen = AtomicReference<String?>("not seen")
        val other = OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope)
            .addInterceptor { chain ->
                seen.set(chain.request().header("Cookie"))
                Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(200).message("m")
                    .body("ok".toResponseBody()).build()
            }.build()
        assertEquals(200, get(other, host = "two.test", path = "/a").first)
        app.stop()
        assertNull(seen.get(), "one.test's cookie does not reach two.test")
    }

    private fun saved(pass: String, value: String = "stale", validUntil: Long? = null) =
        "ok=true\nuserAgent=Agent/9\nsource=saved\npass=$pass\n" +
            (validUntil?.let { "validUntil=$it\n" } ?: "") + "cookie=cf_clearance=$value\n"

    @Test
    fun `a saved pass the site refuses is named to the app and replaced, and the site stays reachable over this connection`() {
        val app = FakeApp(exchange, listOf(saved("r1-7"), passed))
        assertEquals(200 to "content", get(client()))
        assertEquals(2, app.verified.get())
        assertFalse(app.requests[0].contains("rejected="), app.requests[0])
        assertTrue(app.requests[1].lines().contains("rejected=r1-7"), app.requests[1])
        assertEquals(200, get(client()).first, "the fresh pass is kept")
        app.stop()
        assertEquals(2, app.verified.get())
        assertEquals(0, app.fetched.get(), "a refused saved pass does not make the site browser-only")
    }

    @Test
    fun `a saved pass the site accepts is used like any other`() {
        val app = FakeApp(exchange, listOf(saved("r1-1", value = "ok")))
        assertEquals(200, get(client()).first)
        assertEquals(200, get(client(), path = "/other").first)
        app.stop()
        assertEquals(1, app.verified.get())
    }

    @Test
    fun `refused saved passes are given up on after a bound`() {
        val app = FakeApp(exchange, listOf(saved("r1-1"), saved("r1-2"), saved("r1-3"), saved("r1-4"), saved("r1-5")))
        val error = assertFailsWith<IOException> { get(client()) }
        app.stop()
        assertTrue(error.message.orEmpty().contains("refused every saved pass"), error.message)
        assertEquals(4, app.verified.get(), "the first answer and three re-asks")
        assertEquals(listOf("rejected=r1-1", "rejected=r1-2", "rejected=r1-3"), app.requests.drop(1).map { text ->
            text.lines().first { it.startsWith("rejected=") }
        })
    }

    @Test
    fun `a saved pass without a readable id is still reported as refused`() {
        val app = FakeApp(exchange, listOf("ok=true\nuserAgent=A\nsource=saved\npass=../x\ncookie=cf_clearance=stale\n", passed))
        assertEquals(200, get(client()).first)
        app.stop()
        assertTrue(app.requests[1].lines().contains("rejected=unnamed"), app.requests[1])
    }

    @Test
    fun `a pass whose clearance has ended is not sent, the site is asked again`() {
        val now = java.util.concurrent.atomic.AtomicLong(1_000_000L)
        CloudflareSolver.clock = { now.get() }
        val app = FakeApp(exchange, listOf("ok=true\nuserAgent=A\nvalidUntil=1001000\ncookie=cf_clearance=ok\n", passed))
        val cookies = CopyOnWriteArrayList<String>()
        val client = OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope)
            .addInterceptor { chain ->
                val cookie = chain.request().header("Cookie").orEmpty()
                cookies.add(cookie)
                val accepted = cookie.contains("cf_clearance=ok")
                Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                    .code(if (accepted) 200 else 403).message("m")
                    .apply { if (!accepted) header("cf-mitigated", "challenge") }
                    .body("x".toResponseBody()).build()
            }.build()
        assertEquals(200, get(client).first)
        assertEquals(200, get(client).first, "still valid: sent at once")
        assertEquals(1, app.verified.get())
        now.set(1_001_000L)
        val before = cookies.size
        assertEquals(200, get(client).first)
        app.stop()
        assertEquals("", cookies[before], "the ended pass is not sent")
        assertEquals(2, app.verified.get())
    }

    @Test
    fun `an origin served only by the browser window asks for the window, not for cookies`() {
        val app = FakeApp(exchange, listOf(passed), listOf("ok=false\ngone=true\nreason=no window\n", browserAnswer))
        assertEquals("from the browser", get(pickyClient()).second)
        app.stop()
        assertFalse(app.requests[0].contains("need="), app.requests[0])
        assertTrue(app.requests[1].lines().contains("need=browser"), app.requests[1])
    }

    @Test
    fun `many requests refused with one saved pass name it once and share one fresh check`() {
        val app = FakeApp(exchange, listOf(saved("r1-9"), passed))
        val shared = client()
        val results = CopyOnWriteArrayList<Int>()
        val start = CountDownLatch(1)
        val workers = (1..20).map { n -> thread { start.await(); results.add(get(shared, path = "/p$n").first) } }
        start.countDown()
        workers.forEach { it.join(20_000) }
        app.stop()
        assertEquals(20, results.size, "every request finishes")
        assertTrue(results.all { it == 200 }, "$results")
        assertEquals(2, app.verified.get(), "one saved answer, one fresh check: ${app.requests}")
        assertEquals(1, app.requests.count { it.contains("rejected=r1-9") })
        assertEquals(0, app.fetched.get())
    }

    @Test
    fun `log lines carry the tag and level the app filters on`() {
        val captured = java.io.ByteArrayOutputStream()
        val original = System.err
        System.setErr(java.io.PrintStream(captured, true))
        try {
            CloudflareSolver.log("WARN", "a message")
        } finally {
            System.setErr(original)
        }
        assertEquals("[cloudflare] WARN a message", captured.toString().trim())
    }

    @Test
    fun `reset forgets a refused saved pass that was never named`() {
        val app = FakeApp(exchange, listOf(saved("r1-1"), saved("r1-2"), saved("r1-3"), saved("r1-4")))
        assertFailsWith<IOException> { get(client()) }
        app.stop()
        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        val fresh = FakeApp(exchange, listOf(passed))
        assertEquals(200, get(client()).first)
        fresh.stop()
        assertFalse(fresh.requests.single().contains("rejected="), fresh.requests.single())
    }

    @Test
    fun `reset forgets a go-ahead answer, even for a clock that went back`() {
        val first = FakeApp(exchange, listOf("ok=false\nreason=old answer\nretry=true\n"))
        assertFailsWith<IOException> { get(client()) }
        first.stop()
        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        CloudflareSolver.clock = { 1_000L }
        val second = FakeApp(exchange, listOf(passed))
        assertEquals(200, get(client()).first, "not refused with the answer from before the reset")
        second.stop()
        assertEquals(1, second.verified.get())
    }

    /** Everything [CloudflareSolver.log] writes while [work] runs. */
    private fun stderrOf(work: () -> Unit): String {
        val captured = java.io.ByteArrayOutputStream()
        val original = System.err
        System.setErr(java.io.PrintStream(captured, true))
        try {
            work()
        } finally {
            System.setErr(original)
        }
        return captured.toString()
    }

    /** A site that challenges the first request and fails the retried one with [failure]. */
    private fun failingRetryClient(failure: IOException): OkHttpClient {
        val calls = AtomicInteger()
        return OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope)
            .addInterceptor { chain ->
                if (calls.incrementAndGet() > 1) throw failure
                Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(403).message("m")
                    .header("cf-mitigated", "challenge").body("Just a moment...".toResponseBody()).build()
            }.build()
    }

    @Test
    fun `a retry that fails is logged with the host, the pass and the reason, and the plugin still gets the error`() {
        val app = FakeApp(exchange, listOf(saved("r1-5", value = "ok")))
        val failure = IOException("Canceled")
        lateinit var thrown: IOException
        val logged = stderrOf { thrown = assertFailsWith<IOException> { get(failingRetryClient(failure)) } }
        app.stop()
        assertEquals("Canceled", thrown.message, "the plugin sees the original failure")
        assertTrue(
            logged.lines().any { it == "[cloudflare] WARN retry to site.test with the saved pass r1-5 failed: IOException: Canceled" },
            logged,
        )
        assertFalse(logged.contains("cf_clearance=ok"), "no cookie value in the log")
    }

    @Test
    fun `a failed retry with a fresh pass says so`() {
        val app = FakeApp(exchange, listOf(passed))
        val logged = stderrOf { assertFailsWith<IOException> { get(failingRetryClient(java.net.SocketTimeoutException("connect timed out"))) } }
        app.stop()
        assertTrue(logged.contains("WARN retry to site.test with the fresh pass failed: SocketTimeoutException: connect timed out"), logged)
    }

    // --- The log the app keeps in cloudflare-logs.jsonl: each step of a request's story, word for word. ---

    private fun lines(work: () -> Unit): List<String> = stderrOf(work).lines().filter { it.isNotBlank() }

    @Test
    fun `the log tells a challenge's story from challenge to accepted retry`() {
        val app = FakeApp(exchange, listOf(passed))
        val log = lines { assertEquals(200, get(client()).first) }
        app.stop()
        assertTrue("[cloudflare] INFO challenge from site.test: status 403, server=cloudflare, cf-mitigated=challenge for GET" in log, "$log")
        assertTrue(log.any { it.startsWith("[cloudflare] INFO asked the app to verify site.test (request ") && it.endsWith(", answer expected within 10s)") }, "$log")
        assertTrue("[cloudflare] INFO the app verified site.test after 0s from the browser: cookies=[cf_clearance] userAgent=Agent/9" in log, "$log")
        assertTrue("[cloudflare] INFO retried request to site.test with the verification cookies: status 200, server=cloudflare, cf-mitigated=null; the site accepted them" in log, "$log")
        assertTrue(log.none { it.contains("still waiting") }, "a quick answer is not reported as a wait: $log")
    }

    @Test
    fun `the log names a saved pass, its refusal and a give-up`() {
        val app = FakeApp(exchange, listOf(saved("r1-1"), saved("r1-2"), saved("r1-3"), saved("r1-4")))
        val log = lines { assertFailsWith<IOException> { get(client()) } }
        app.stop()
        assertTrue("[cloudflare] INFO the app verified site.test after 0s from a saved pass (r1-1): cookies=[cf_clearance] userAgent=Agent/9" in log, "$log")
        assertTrue("[cloudflare] WARN site.test refused the saved pass r1-1; the app will check it again" in log, "$log")
        assertTrue("[cloudflare] ERROR site.test refused 4 saved passes in a row; giving up on this request" in log, "$log")
    }

    @Test
    fun `the log says when a site is left alone after a refusal, and not after a go-ahead`() {
        val closed = FakeApp(exchange, listOf("ok=false\nreason=the window was closed\n"))
        val declined = lines {
            assertFailsWith<IOException> { get(client()) }
            assertFailsWith<IOException> { get(client()) }
        }
        closed.stop()
        assertTrue("[cloudflare] WARN verification of site.test not done: the window was closed; not asking again for 60s" in declined, "$declined")
        assertTrue("[cloudflare] INFO site.test was not verified a moment ago (the window was closed); failing at once, not asking again yet" in declined, "$declined")
        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        val waiting = FakeApp(exchange, listOf("ok=false\nreason=waiting for the go-ahead\nretry=true\n"))
        val asked = lines { assertFailsWith<IOException> { get(client()) } }
        waiting.stop()
        assertTrue("[cloudflare] WARN verification of site.test not done: waiting for the go-ahead" in asked, "$asked")
    }

    @Test
    fun `the log says a request is still waiting and when the app never answers`() {
        CloudflareSolver.heartbeatMs = 100
        val slow = FakeApp(exchange, listOf(passed), delayMs = 450)
        val waited = lines { assertEquals(200, get(client()).first) }
        slow.stop()
        assertTrue("[cloudflare] INFO still waiting for the viewer to verify site.test (0s)" in waited, "$waited")
        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        CloudflareSolver.solveTimeoutMs = 700
        val silent = lines { assertFailsWith<IOException> { get(client()) } }
        assertTrue("[cloudflare] WARN no answer from the app for site.test after 0s (is the app's verification watcher running?)" in silent, "$silent")
        assertTrue("[cloudflare] WARN verification of site.test not done: no answer within 0s; not asking again for 60s" in silent, "$silent")
    }

    @Test
    fun `the log follows a site onto the browser window and to the end of the line`() {
        val challenge = "ok=true\nstatus=403\nheader=server: cloudflare\nheader=cf-mitigated: challenge\nbody=" +
            Base64.getEncoder().encodeToString("Just a moment...".toByteArray()) + "\n"
        val app = FakeApp(exchange, listOf(passed), listOf("ok=false\ngone=true\nreason=no window\n", challenge))
        val log = lines { assertFailsWith<IOException> { get(pickyClient()) } }
        app.stop()
        assertTrue("[cloudflare] WARN site.test did not accept the verification cookies over this connection (status 403, server=cloudflare, cf-mitigated=challenge); from now on its requests are made by the browser window" in log, "$log")
        assertTrue("[cloudflare] WARN the browser window for site.test is gone (no window); verifying again" in log, "$log")
        assertTrue("[cloudflare] ERROR Cloudflare still showed its check even in the browser window; site.test cannot be reached; not asking again for 60s" in log, "$log")
        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        val again = FakeApp(exchange, listOf(passed), listOf(challenge))
        val repeated = lines { assertFailsWith<IOException> { get(pickyClient()) } }
        again.stop()
        assertTrue("[cloudflare] WARN the browser got a Cloudflare challenge again for site.test; verifying again" in repeated, "$repeated")
        assertTrue(repeated.none { it.contains("browser window for site.test is gone") }, "a challenge is not a missing window: $repeated")
    }

    @Test
    fun `the log says when the browser alone serves a site and when a pass has ended`() {
        val app = FakeApp(exchange, listOf("ok=true\nuserAgent=Agent/9\ntransport=browser\n"), listOf(browserAnswer))
        val log = lines { get(pickyClient()) }
        app.stop()
        assertTrue("[cloudflare] INFO site.test does not challenge the browser, only this connection; its requests are made by the browser window" in log, "$log")
        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        val now = java.util.concurrent.atomic.AtomicLong(5_000L)
        CloudflareSolver.clock = { now.get() }
        val ending = FakeApp(exchange, listOf("ok=true\nuserAgent=A\nvalidUntil=6000\ncookie=cf_clearance=ok\n", passed))
        get(client())
        now.set(6_000L)
        val ended = lines { get(client()) }
        ending.stop()
        assertTrue("[cloudflare] INFO the pass for site.test has ended; not sending it" in ended, "$ended")
    }

    // --- Every response the solver does not hand to the plugin is closed (a leaked body holds a connection). ---

    /** A body that remembers whether it was closed. */
    private class TrackedBody(text: String) : ResponseBody() {
        @Volatile var closed = false
        private val buffer = Buffer().writeUtf8(text)
        private val length = text.length.toLong()
        override fun contentType() = null
        override fun contentLength() = length
        override fun source(): BufferedSource = buffer
        override fun close() {
            closed = true
            buffer.close()
        }
    }

    /** A site that accepts a request when [accepts] says so, and keeps every body it answered with. */
    private fun trackedClient(bodies: MutableList<TrackedBody>, accepts: (Request) -> Boolean): OkHttpClient =
        OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope)
            .addInterceptor { chain ->
                val accepted = accepts(chain.request())
                val body = TrackedBody(if (accepted) "content" else "Just a moment...").also { bodies.add(it) }
                Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                    .code(if (accepted) 200 else 403).message("m").header("server", "cloudflare")
                    .apply { if (!accepted) header("cf-mitigated", "challenge") }
                    .body(body).build()
            }.build()

    @Test
    fun `every challenge page and refused retry is closed, whichever way the request goes`() {
        val bodies = CopyOnWriteArrayList<TrackedBody>()
        val okCookie: (Request) -> Boolean = { it.header("Cookie").orEmpty().contains("cf_clearance=ok") }
        val app = FakeApp(exchange, listOf(saved("r1-1"), passed), listOf(browserAnswer))
        assertEquals(200, get(trackedClient(bodies, okCookie)).first)
        assertEquals("from the browser", get(trackedClient(bodies) { false }, host = "picky.test").second)
        app.stop()
        assertTrue(bodies.size >= 5, "the scenarios ran: ${bodies.size}")
        assertTrue(bodies.all { it.closed }, "${bodies.count { !it.closed }} of ${bodies.size} bodies left open")
    }

    // --- Races at the edges of the shared check. ---

    @Test
    fun `requests that start in the same millisecond as the answer share the check`() {
        CloudflareSolver.clock = { 1_000L } // time stands still: an answer is never strictly newer
        val app = FakeApp(exchange, listOf(passed))
        val shared = client()
        val results = CopyOnWriteArrayList<Int>()
        val start = CountDownLatch(1)
        val workers = (1..12).map { thread { start.await(); results.add(get(shared).first) } }
        start.countDown()
        workers.forEach { it.join(20_000) }
        app.stop()
        assertEquals(List(12) { 200 }, results.toList())
        assertEquals(1, app.verified.get(), "one verification, even within one millisecond")
    }

    @Test
    fun `requests queued while the viewer declines fail with that answer instead of asking again`() {
        val app = FakeApp(exchange, listOf("ok=false\nreason=the window was closed\n", passed))
        val shared = client()
        val failures = CopyOnWriteArrayList<String>()
        val start = CountDownLatch(1)
        val workers = (1..12).map {
            thread { start.await(); runCatching { get(shared) }.onFailure { failures.add(it.message.orEmpty()) } }
        }
        start.countDown()
        workers.forEach { it.join(20_000) }
        app.stop()
        assertEquals(12, failures.size, "every queued request fails")
        assertTrue(failures.all { it.contains("the window was closed") }, "$failures")
        assertEquals(1, app.verified.get(), "the viewer is asked once")
    }

    @Test
    fun `a saved pass that a later request finds refused is reported at the next question`() {
        // The second request starts after the pass was obtained: it carried the pass, so the challenge refuses it.
        val now = java.util.concurrent.atomic.AtomicLong(1_000L)
        CloudflareSolver.clock = { now.get() }
        val accepted = AtomicReference("first")
        val bodies = CopyOnWriteArrayList<TrackedBody>()
        val shared = trackedClient(bodies) { it.header("Cookie").orEmpty().contains("cf_clearance=${accepted.get()}") }
        val app = FakeApp(exchange, listOf(saved("r1-1", value = "first"), passed.replace("cf_clearance=ok", "cf_clearance=second")))
        assertEquals(200, get(shared).first)
        now.set(2_000L)
        accepted.set("second") // the site raised its protection: the kept saved pass no longer opens it
        assertEquals(200, get(shared, path = "/later").first)
        app.stop()
        assertEquals(2, app.verified.get())
        assertTrue(app.requests[1].lines().contains("rejected=r1-1"), app.requests[1])
    }

    // --- Unchallenged cross-origin redirects (e.g. a domain move behind Cloudflare). ---

    private fun redirectAnswer(status: Int, location: String) =
        "ok=true\ntransport=redirect\nstatus=$status\nlocation=$location\n"

    @Test
    fun `an unchallenged cross-origin redirect is followed without leaking sensitive headers`() {
        val observed = AtomicReference<Request?>()
        val bodies = CopyOnWriteArrayList<TrackedBody>()
        val client = OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope)
            .addInterceptor { chain ->
                val req = chain.request()
                val ok = req.url.host == "cinevood.garden"
                if (ok) observed.set(req)
                val body = TrackedBody(if (ok) "moved-page" else "Just a moment...").also { bodies.add(it) }
                Response.Builder().request(req).protocol(Protocol.HTTP_1_1)
                    .code(if (ok) 200 else 403).message("m").header("server", "cloudflare")
                    .apply { if (!ok) header("cf-mitigated", "challenge") }
                    .body(body).build()
            }.build()
        val app = FakeApp(exchange, listOf(redirectAnswer(301, "https://cinevood.garden/search?s=odyssey")))
        val request = Request.Builder().url("https://cinevood.loan/search?s=odyssey")
            .header("Authorization", "Bearer secret")
            .header("Cookie", "session=secret")
            .header("Proxy-Authorization", "Basic secret")
            .header("Accept", "text/html")
            .build()
        val log = lines {
            client.newCall(request).execute().use { reply ->
                assertEquals(200, reply.code)
                assertEquals("moved-page", reply.body.string())
                assertEquals("https://cinevood.garden/search?s=odyssey", reply.request.url.toString())
            }
        }
        app.stop()
        assertEquals(1, app.verified.get())
        assertTrue(bodies.all { it.closed }, "every intermediate body is closed")
        val forwarded = observed.get()!!
        assertNull(forwarded.header("Authorization"))
        assertNull(forwarded.header("Cookie"))
        assertNull(forwarded.header("Proxy-Authorization"))
        assertEquals("text/html", forwarded.header("Accept"))
        assertTrue(
            "[cloudflare] INFO cinevood.loan was not challenged in the browser and redirected (301) to cinevood.garden; following the redirect" in log,
            "$log",
        )
    }

    @Test
    fun `a redirect target that also challenges is verified under its own origin`() {
        val client = OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope)
            .addInterceptor { chain ->
                val req = chain.request()
                val accepted = req.url.host == "cinevood.garden" && req.header("Cookie").orEmpty().contains("cf_clearance=ok")
                Response.Builder().request(req).protocol(Protocol.HTTP_1_1)
                    .code(if (accepted) 200 else 403).message("m").header("server", "cloudflare")
                    .apply { if (!accepted) header("cf-mitigated", "challenge") }
                    .body((if (accepted) "garden-content" else "Just a moment...").toResponseBody()).build()
            }.build()
        val app = FakeApp(exchange, listOf(redirectAnswer(301, "https://cinevood.garden/home"), passed))
        assertEquals(200 to "garden-content", get(client, host = "cinevood.loan", path = "/home"))
        app.stop()
        assertEquals(2, app.verified.get())
        assertTrue(app.requests[0].lines().contains("host=cinevood.loan"), app.requests[0])
        assertTrue(app.requests[1].lines().contains("host=cinevood.garden"), app.requests[1])
    }

    @Test
    fun `concurrent requests to different paths of a redirecting origin each follow their own path`() {
        val handled = mutableSetOf<String>()
        val worker = thread(isDaemon = true) {
            while (!Thread.currentThread().isInterrupted) {
                exchange.listFiles()?.forEach { file ->
                    if (file.name.startsWith("request-") && file.name.endsWith(".txt") && handled.add(file.name)) {
                        val url = file.readLines().first { it.startsWith("url=") }.removePrefix("url=").trim()
                        val moved = url.replace("https://old.test", "https://new.test")
                        File(exchange, file.name.replaceFirst("request-", "result-")).writeText(redirectAnswer(301, moved))
                    }
                }
                Thread.sleep(15)
            }
        }
        val client = OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor(CloudflareSolver.clearanceScope)
            .addInterceptor { chain ->
                val req = chain.request()
                val ok = req.url.host == "new.test"
                Response.Builder().request(req).protocol(Protocol.HTTP_1_1)
                    .code(if (ok) 200 else 403).message("m")
                    .apply { if (!ok) header("cf-mitigated", "challenge") }
                    .body((if (ok) req.url.encodedPath else "challenge").toResponseBody()).build()
            }.build()
        val start = CountDownLatch(1)
        val mismatches = CopyOnWriteArrayList<String>()
        try {
            val threads = (1..8).map { n ->
                thread {
                    start.await()
                    val expected = "/movie-$n"
                    val (code, body) = get(client, host = "old.test", path = expected)
                    if (code != 200 || body != expected) mismatches.add("$expected -> $code:$body")
                }
            }
            start.countDown()
            threads.forEach { it.join(20_000) }
        } finally {
            worker.interrupt()
        }
        assertTrue(mismatches.isEmpty(), "every path must reach its own redirected page: $mismatches")
    }

    @Test
    fun `redirect loops, excessive redirect hops, and credentialed redirect targets fail cleanly`() {
        val loopApp = FakeApp(exchange, listOf(redirectAnswer(301, "https://loop.test/api")))
        val loopErr = assertFailsWith<IOException> { get(pickyClient(), host = "loop.test", path = "/api") }
        loopApp.stop()
        assertTrue(loopErr.message.orEmpty().contains("redirect loop"), loopErr.message)

        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        val chainApp = FakeApp(
            exchange,
            (1..7).map { n -> redirectAnswer(302, "https://hop$n.test/p") },
        )
        val hopErr = assertFailsWith<IOException> { get(pickyClient(), host = "hop0.test", path = "/p") }
        chainApp.stop()
        assertTrue(hopErr.message.orEmpty().contains("too many redirects"), hopErr.message)

        CloudflareSolver.reset()
        CloudflareSolver.exchangeDir = exchange
        val credApp = FakeApp(exchange, listOf(redirectAnswer(301, "https://user:pass@other.test/p")))
        val credErr = assertFailsWith<IOException> { get(pickyClient(), host = "cred.test", path = "/p") }
        credApp.stop()
        assertTrue(credErr.message.orEmpty().contains("invalid redirect"), credErr.message)
    }

    @Test
    fun `a verified redirect to a local address is blocked by URLStub buildSafeOkHttpClient`() {
        val app = FakeApp(exchange, listOf(redirectAnswer(301, "http://127.0.0.1:9/secret")))
        val builder = OkHttpClient.Builder()
            .addInterceptor(CloudflareSolver.interceptor)
            .addInterceptor { chain ->
                val req = chain.request()
                if (req.url.host == "redirect-ssrf.invalid") {
                    Response.Builder().request(req).protocol(Protocol.HTTP_1_1)
                        .code(403).message("m").header("cf-mitigated", "challenge")
                        .body("Just a moment...".toResponseBody()).build()
                } else {
                    chain.proceed(req)
                }
            }
        val safeClient = URLStub.buildSafeOkHttpClient(builder)
        val error = assertFailsWith<SecurityException> {
            safeClient.newCall(Request.Builder().url("https://redirect-ssrf.invalid/start").build()).execute()
        }
        app.stop()
        assertTrue(error.message.orEmpty().contains("blocked"), error.message)
    }

    @Test
    fun `redirectRequest enforces HTTP redirect method rules`() {
        val post = Request.Builder().url("https://a.test/x").post("body".toRequestBody()).build()
        val downgraded = CloudflareSolver.redirectRequest(post, "https://b.test/y".toHttpUrl(), 302)
        assertEquals("GET", downgraded.method)
        assertNull(downgraded.body)
        val preserved = CloudflareSolver.redirectRequest(post, "https://b.test/y".toHttpUrl(), 307)
        assertEquals("POST", preserved.method)
    }
}

