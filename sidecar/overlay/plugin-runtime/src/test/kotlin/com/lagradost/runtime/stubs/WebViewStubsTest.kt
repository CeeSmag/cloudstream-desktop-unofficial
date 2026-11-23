package com.lagradost.runtime.stubs

import android.content.Context
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebSettings
import android.webkit.WebStorage
import android.webkit.WebView
import android.widget.AbsoluteLayout
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Test
import java.util.concurrent.Callable
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertSame
import kotlin.test.assertTrue

/** Contract and concurrency tests for WebView, WebSettings, WebChromeClient, WebStorage, MotionEvent, and View. */
class WebViewStubsTest {

    @AfterEach
    fun cleanupStaticHooks() {
        WebView.loadUrlHandler = null
        WebStorage.getInstance().deleteAllData()
    }

    @Test
    fun `motion event obtain exposes action coordinates source and idempotent recycle`() {
        val action = MotionEvent.ACTION_POINTER_DOWN or (2 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT)
        val ev = MotionEvent.obtain(100L, 120L, action, 42.5f, 88.0f, 1)
        assertEquals(100L, ev.downTime)
        assertEquals(120L, ev.eventTime)
        assertEquals(action, ev.action)
        assertEquals(MotionEvent.ACTION_POINTER_DOWN, ev.actionMasked)
        assertEquals(2, ev.actionIndex)
        assertEquals(42.5f, ev.x)
        assertEquals(88.0f, ev.y)
        assertEquals(1, ev.metaState)
        ev.setSource(4098)
        assertEquals(4098, ev.source)

        val copy = MotionEvent.obtain(ev)
        assertEquals(ev.source, copy.source)
        assertFalse(ev.isRecycled)
        ev.recycle()
        ev.recycle()
        assertTrue(ev.isRecycled)
    }

    @Test
    fun `webview inherits viewgroup and view contracts including removeAllViews touch scroll and layout`() {
        val wv = WebView(Context())
        assertTrue(AbsoluteLayout::class.java.isAssignableFrom(WebView::class.java))
        assertTrue(ViewGroup::class.java.isAssignableFrom(WebView::class.java))
        assertTrue(View::class.java.isAssignableFrom(WebView::class.java))

        val child = View()
        wv.addView(child)
        assertEquals(1, wv.childCount)
        wv.removeAllViews()
        assertEquals(0, wv.childCount)
        assertNull(child.parent)

        val clicked = AtomicInteger(0)
        wv.setOnClickListener { clicked.incrementAndGet() }
        val up = MotionEvent.obtain(10L, 20L, MotionEvent.ACTION_UP, 5f, 6f, 0)
        assertTrue(wv.dispatchTouchEvent(up))
        assertEquals(1, clicked.get())

        val intercepted = AtomicBoolean(false)
        wv.setOnTouchListener { _, event ->
            intercepted.set(event === up)
            true
        }
        assertTrue(wv.dispatchTouchEvent(up))
        assertTrue(intercepted.get())
        assertEquals(1, clicked.get(), "consumed touch does not fall through to onTouchEvent")

        wv.scrollBy(12, 34)
        wv.scrollBy(-2, 6)
        assertEquals(10, wv.scrollX)
        assertEquals(40, wv.scrollY)

        wv.layout(10, 20, 410, 820)
        assertEquals(400, wv.width)
        assertEquals(800, wv.height)
    }

    @Test
    fun `webview retains settings progress headers chrome client and javascript interfaces`() {
        val wv = WebView(Context())
        val settings = wv.settings
        assertSame(settings, wv.settings)
        settings.javaScriptEnabled = false
        settings.userAgentString = "CustomUA/1.0"
        assertFalse(wv.settings.javaScriptEnabled)
        assertEquals("CustomUA/1.0", wv.settings.userAgentString)

        val defaultUa = WebSettings.getDefaultUserAgent(Context())
        assertTrue(defaultUa.contains("Mozilla/5.0"))
        assertEquals(defaultUa, WebSettings().userAgentString)

        assertEquals(0, wv.progress)
        val seen = AtomicReference<String?>()
        WebView.loadUrlHandler = java.util.function.BiConsumer { _, url -> seen.set(url) }
        wv.loadUrl("https://example.test/watch", mapOf("Referer" to "https://example.test/"))
        assertEquals("https://example.test/watch", wv.url)
        assertEquals("https://example.test/watch", seen.get())

        val progressEvents = mutableListOf<Int>()
        val titles = mutableListOf<String>()
        val chrome = object : WebChromeClient() {
            override fun onProgressChanged(view: WebView?, newProgress: Int) {
                super.onProgressChanged(view, newProgress)
                progressEvents += newProgress
            }

            override fun onReceivedTitle(view: WebView?, title: String?) {
                super.onReceivedTitle(view, title)
                if (title != null) titles += title
            }
        }
        wv.webChromeClient = chrome
        assertSame(chrome, wv.webChromeClient)
        chrome.onProgressChanged(wv, 50)
        chrome.onReceivedTitle(wv, "Ready")
        assertEquals(listOf(50), progressEvents)
        assertEquals(listOf("Ready"), titles)

        wv.addJavascriptInterface(Any(), "Bridge")
        wv.removeJavascriptInterface("Bridge")
        wv.removeJavascriptInterface("Missing")
        wv.destroy()
        assertNull(wv.url)
        assertEquals(0, wv.progress)
    }

    @Test
    fun `view subclass can call protected onWindowVisibilityChanged and cancel posted callbacks`() {
        val visibleValues = mutableListOf<Int>()
        val custom = object : WebView(Context()) {
            override fun onWindowVisibilityChanged(visibility: Int) {
                super.onWindowVisibilityChanged(visibility)
                visibleValues += visibility
            }

            fun triggerVisibility(v: Int) = onWindowVisibilityChanged(v)
        }
        custom.triggerVisibility(View.VISIBLE)
        assertEquals(listOf(View.VISIBLE), visibleValues)

        val handler = custom.handler
        assertNotNull(handler)
        assertSame(handler, custom.handler)
        val ran = AtomicBoolean(false)
        val task = Runnable { ran.set(true) }
        custom.postDelayed(task, 5_000L)
        custom.removeCallbacks(task)
        assertFalse(ran.get())
    }

    @Test
    fun `webstorage singleton tracks deletes per origin and clears all data`() {
        val storage = WebStorage.getInstance()
        assertSame(storage, WebStorage.getInstance())
        storage.setQuotaForOrigin("https://a.test", 1024L)
        storage.setQuotaForOrigin("https://b.test", 2048L)

        assertEquals(1024L, readLong { storage.getQuotaForOrigin("https://a.test", it) })
        assertEquals(0L, readLong { storage.getUsageForOrigin("https://a.test", it) })

        storage.deleteOrigin("https://a.test")
        assertEquals(0L, readLong { storage.getQuotaForOrigin("https://a.test", it) })
        assertEquals(2048L, readLong { storage.getQuotaForOrigin("https://b.test", it) })

        storage.deleteAllData()
        val origins = AtomicReference<Map<*, *>?>()
        storage.getOrigins(ValueCallback { origins.set(it) })
        assertTrue(origins.get().orEmpty().isEmpty())
    }

    @Test
    fun `concurrent webstorage and webview state updates remain race-free`() {
        val storage = WebStorage.getInstance()
        val wv = WebView(Context())
        val threads = 8
        val iterations = 250
        val start = CountDownLatch(1)
        val pool = Executors.newFixedThreadPool(threads)
        try {
            val futures = (0 until threads).map { id ->
                pool.submit(
                    Callable {
                        start.await(5, TimeUnit.SECONDS)
                        val firstHandler = wv.handler
                        for (i in 0 until iterations) {
                            val origin = "https://origin-$id-${i % 10}.test"
                            storage.setQuotaForOrigin(origin, (i + 1).toLong())
                            storage.getOrigins(ValueCallback { map -> map.size })
                            if (i % 3 == 0) storage.deleteOrigin(origin)
                            if (i % 50 == 0) storage.deleteAllData()
                            val iface = "js-$id-${i % 5}"
                            wv.addJavascriptInterface(origin, iface)
                            wv.removeJavascriptInterface(iface)
                            assertSame(firstHandler, wv.handler)
                        }
                        true
                    },
                )
            }
            start.countDown()
            for (f in futures) {
                assertTrue(f.get(15, TimeUnit.SECONDS))
            }
        } finally {
            pool.shutdownNow()
        }
    }

    private fun readLong(call: (ValueCallback<Long>) -> Unit): Long {
        val ref = AtomicReference<Long>()
        call(ValueCallback { ref.set(it) })
        return ref.get() ?: -1L
    }
}
