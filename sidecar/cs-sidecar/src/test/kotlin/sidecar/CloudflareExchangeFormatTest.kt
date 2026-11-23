package sidecar

import com.lagradost.runtime.loader.stubs.CloudflareExchangeFormat
import com.lagradost.runtime.loader.stubs.CloudflareExchangeFormat.Ask
import com.lagradost.runtime.loader.stubs.CloudflareExchangeFormat.Source
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** The verification file texts, without files or threads. */
class CloudflareExchangeFormatTest {
    @Test
    fun `an ordinary request names the challenged request only`() {
        assertEquals(
            "id=a1\nhost=site.test\nurl=https://site.test/x\nmethod=GET\n",
            CloudflareExchangeFormat.requestText("a1", "site.test", "https://site.test/x", "GET", Ask()),
        )
    }

    @Test
    fun `a request says which saved pass was refused and whether only the window helps`() {
        val text = CloudflareExchangeFormat.requestText("a1", "h.test", "https://h.test/", "POST", Ask("r1-2", needsBrowser = true))
        assertEquals("id=a1\nhost=h.test\nurl=https://h.test/\nmethod=POST\nrejected=r1-2\nneed=browser\n", text)
        val unsafe = CloudflareExchangeFormat.requestText("a1", "h.test", "https://h.test/", "GET", Ask("../x\nneed=any"))
        assertTrue(unsafe.lines().contains("rejected=unnamed"), unsafe)
        assertFalse(unsafe.contains("need=any"), "an id cannot inject a line")
    }

    @Test
    fun `ids follow the app's rule`() {
        assertTrue(CloudflareExchangeFormat.isSafeId("r1a-2"))
        assertTrue(CloudflareExchangeFormat.isSafeId("a".repeat(64)))
        for (bad in listOf("", "a b", "a_b", "../x", "a".repeat(65), "é")) {
            assertFalse(CloudflareExchangeFormat.isSafeId(bad), bad)
        }
    }

    @Test
    fun `a saved answer carries its source, id and lifetime`() {
        val clearance = CloudflareExchangeFormat.parseClearance(
            "ok=true\nuserAgent=A/1\nsource=saved\npass=r1-3\nvalidUntil=1700000000000\ncookie=cf_clearance=v\n", 5L,
        )
        assertEquals(Source.SAVED, clearance?.source)
        assertEquals("r1-3", clearance?.passId)
        assertEquals(1_700_000_000_000L, clearance?.validUntil)
        assertEquals(5L, clearance?.obtainedAt)
        assertEquals(mapOf("cf_clearance" to "v"), clearance?.cookies)
    }

    @Test
    fun `a browser answer has no pass id and a bad lifetime is ignored`() {
        val browser = CloudflareExchangeFormat.parseClearance("ok=true\nuserAgent=A\npass=r1-3\nvalidUntil=soon\ncookie=a=b\n", 0L)
        assertEquals(Source.BROWSER, browser?.source)
        assertNull(browser?.passId, "only a saved answer names a pass")
        assertNull(browser?.validUntil)
        val unsafe = CloudflareExchangeFormat.parseClearance("ok=true\nuserAgent=A\nsource=saved\npass=a b\ncookie=a=b\n", 0L)
        assertEquals(Source.SAVED, unsafe?.source)
        assertNull(unsafe?.passId)
    }

    @Test
    fun `an answer still needs success, an agent and cookies unless browser-only`() {
        assertNull(CloudflareExchangeFormat.parseClearance("ok=false\nreason=closed\n", 0L))
        assertNull(CloudflareExchangeFormat.parseClearance("ok=true\nuserAgent=A\n", 0L))
        assertNull(CloudflareExchangeFormat.parseClearance("ok=true\ncookie=a=b\n", 0L))
        val browserOnly = CloudflareExchangeFormat.parseClearance("ok=true\nuserAgent=A\ntransport=browser\ncookie=a=b\n", 0L)
        assertEquals(true, browserOnly?.browserOnly)
        assertEquals(emptyMap(), browserOnly?.cookies)
    }

    @Test
    fun `a clearance ends at its lifetime, and one without a lifetime never does`() {
        val ends = CloudflareExchangeFormat.parseClearance("ok=true\nuserAgent=A\nvalidUntil=100\ncookie=a=b\n", 0L)!!
        assertFalse(ends.isExpired(99))
        assertTrue(ends.isExpired(100))
        val open = CloudflareExchangeFormat.parseClearance("ok=true\nuserAgent=A\ncookie=a=b\n", 0L)!!
        assertFalse(open.isExpired(Long.MAX_VALUE))
    }

    @Test
    fun `a refusal says why and whether to ask again next time`() {
        assertEquals("closed", CloudflareExchangeFormat.reason("ok=false\nreason= closed \n"))
        assertEquals("the app's answer had no usable cookies", CloudflareExchangeFormat.reason("ok=false\n"))
        assertTrue(CloudflareExchangeFormat.isRetry("ok=false\nretry=true\n"))
        assertFalse(CloudflareExchangeFormat.isRetry("ok=false\nretry=false\n"))
    }

    @Test
    fun `a redirect answer parses HTTP redirect statuses and preserves query parameters`() {
        for (code in listOf(301, 302, 303, 307, 308)) {
            val redirect = CloudflareExchangeFormat.parseRedirect(
                "ok=true\ntransport=redirect\nstatus=$code\nlocation=https://cinevood.garden/search?s=odyssey&page=2\n",
            )
            assertEquals(CloudflareExchangeFormat.Redirect(code, "https://cinevood.garden/search?s=odyssey&page=2"), redirect)
        }
        assertNull(CloudflareExchangeFormat.parseClearance("ok=true\ntransport=redirect\nstatus=301\nlocation=https://cinevood.garden/\n", 0L))
        assertNull(CloudflareExchangeFormat.parseRedirect("ok=false\ntransport=redirect\nstatus=301\nlocation=https://cinevood.garden/\n"))
        assertNull(CloudflareExchangeFormat.parseRedirect("ok=true\ntransport=browser\nstatus=301\nlocation=https://cinevood.garden/\n"))
        assertNull(CloudflareExchangeFormat.parseRedirect("ok=true\ntransport=redirect\nstatus=304\nlocation=https://cinevood.garden/\n"))
        assertNull(CloudflareExchangeFormat.parseRedirect("ok=true\ntransport=redirect\nstatus=200\nlocation=https://cinevood.garden/\n"))
        assertNull(CloudflareExchangeFormat.parseRedirect("ok=true\ntransport=redirect\nstatus=301\nlocation=file:///etc/passwd\n"))
        assertNull(CloudflareExchangeFormat.parseRedirect("ok=true\ntransport=redirect\nstatus=301\n"))
    }
}
