package sidecar

import com.lagradost.runtime.loader.stubs.BrowserFetch
import com.lagradost.runtime.loader.stubs.CloudflareSolver
import com.sun.net.httpserver.HttpServer
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
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
    private class FakeApp(val exchange: File, val verifications: List<String>, val fetches: List<String> = emptyList()) {
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
            Thread.sleep(100)
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
        val workers = (1..12).map {
            thread { start.await(); runCatching { get(shared) }.onFailure { failures.add(it.message.orEmpty()) } }
        }
        start.countDown()
        workers.forEach { it.join(20_000) }
        app.stop()
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
        val workers = (1..12).map { thread { start.await(); results.add(get(shared).first) } }
        start.countDown()
        workers.forEach { it.join(20_000) }
        app.stop()
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
}
