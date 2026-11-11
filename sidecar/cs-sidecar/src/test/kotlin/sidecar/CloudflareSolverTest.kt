package sidecar

import com.lagradost.runtime.loader.stubs.BrowserFetch
import com.lagradost.runtime.loader.stubs.CloudflareSolver
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.util.Base64
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger
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
        assertTrue(CloudflareSolver.isChallenge(response(403, cloudflare, "<title>Just a moment...</title>")))
        assertTrue(CloudflareSolver.isChallenge(response(503, cloudflare, "x challenge-platform x")))
        assertTrue(CloudflareSolver.isChallenge(response(403, mapOf("cf-mitigated" to "challenge"))))
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
        .addInterceptor { chain ->
            val accepted = chain.request().header("Cookie").orEmpty().contains("cf_clearance=ok")
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1)
                .code(if (accepted) 200 else 403).message("m").header("server", "cloudflare")
                .body((if (accepted) "content" else "Just a moment...").toResponseBody()).build()
        }.build()

    /** A site that never accepts the cookies, only its own browser. */
    private fun pickyClient(): OkHttpClient = OkHttpClient.Builder()
        .addInterceptor(CloudflareSolver.interceptor)
        .addInterceptor { chain ->
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(403).message("m")
                .header("server", "cloudflare").header("cf-mitigated", "challenge").body("Just a moment...".toResponseBody()).build()
        }.build()

    private fun get(client: OkHttpClient, host: String = "site.test"): Pair<Int, String> =
        client.newCall(Request.Builder().url("https://$host/api").build()).execute().use { it.code to it.body.string() }

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
}
