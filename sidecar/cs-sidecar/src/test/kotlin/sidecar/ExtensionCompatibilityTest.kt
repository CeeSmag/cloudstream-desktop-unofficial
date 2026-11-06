package sidecar

import android.content.Context
import androidx.fragment.app.FragmentManager
import com.lagradost.cloudstream3.MainActivity
import com.lagradost.cloudstream3.actions.VideoClickAction
import com.lagradost.cloudstream3.plugins.Plugin
import com.lagradost.cloudstream3.syncproviders.AccountManager
import com.lagradost.cloudstream3.utils.txt
import com.lagradost.runtime.loader.stubs.ExecutorsStub
import com.lagradost.runtime.loader.stubs.URLStub
import com.lagradost.runtime.security.PluginSecurityPolicy
import kotlinx.coroutines.Dispatchers
import java.lang.reflect.Modifier
import java.net.InetAddress
import java.net.URL
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class ExtensionCompatibilityTest {
    @Test
    fun `AccountManager exposes anilist mal and simkl APIs`() {
        assertNotNull(AccountManager.simklApi, "AccountManager.simklApi must be available")
        val fieldAniList = AccountManager::class.java.getDeclaredField("aniListApi")
        assertNotNull(fieldAniList, "AccountManager.aniListApi must exist")
        val fieldMal = AccountManager::class.java.getDeclaredField("malApi")
        assertNotNull(fieldMal, "AccountManager.malApi must exist")
    }

    @Test
    fun `AcraApplication companion object provides context`() {
        val acraClass = Class.forName("com.lagradost.cloudstream3.AcraApplication")
        val companionField = acraClass.getDeclaredField("Companion")
        assertNotNull(companionField, "AcraApplication.Companion must exist for plugins")
        val companion = companionField.get(null)
        val getContextMethod = companion.javaClass.getMethod("getContext")
        val ctx = getContextMethod.invoke(companion)
        assertNotNull(ctx, "AcraApplication.Companion.getContext() must return non-null Context")
    }

    @Test
    fun `PluginManager instance is accessible`() {
        val pmClass = Class.forName("com.lagradost.cloudstream3.plugins.PluginManager")
        val instanceField = pmClass.getDeclaredField("INSTANCE")
        assertNotNull(instanceField, "PluginManager.INSTANCE must exist for plugins")
    }

    @Test
    fun `Plugin exposes stable field for compose plugins`() {
        val stableField = Plugin::class.java.getDeclaredField("\$stable")
        assertNotNull(stableField, "Plugin.\$stable field must exist for Compose plugins")
    }

    @Test
    fun `SyncWatchType enum contains required values`() {
        val syncWatchTypeClass = Class.forName("com.lagradost.cloudstream3.ui.SyncWatchType")
        assertTrue(syncWatchTypeClass.isEnum, "SyncWatchType must be an enum")
        val values = syncWatchTypeClass.enumConstants.map { it.toString() }
        assertTrue("WATCHING" in values, "SyncWatchType.WATCHING must exist")
        assertTrue("COMPLETED" in values, "SyncWatchType.COMPLETED must exist")
    }

    @Test
    fun `Context provides application info and content resolver`() {
        val context = Context()
        val appInfo = context.applicationInfo
        assertNotNull(appInfo, "Context.getApplicationInfo() must return non-null ApplicationInfo")
        val contentResolver = context.contentResolver
        assertNotNull(contentResolver, "Context.getContentResolver() must return non-null ContentResolver")
        assertNotNull(context.getString(0), "Context.getString() must return non-null")
    }

    @Test
    fun `MainActivity exposes bookmarks updated and reload events`() {
        val event = MainActivity.bookmarksUpdatedEvent
        assertNotNull(event, "MainActivity.bookmarksUpdatedEvent must be accessible")
        val companionEvent = MainActivity.Companion.bookmarksUpdatedEvent
        assertNotNull(companionEvent, "MainActivity.Companion.bookmarksUpdatedEvent must be accessible")
        assertNotNull(MainActivity.Companion.reloadHomeEvent, "MainActivity.Companion.reloadHomeEvent must be accessible")
    }

    @Test
    fun `FragmentManager has callbacks class and getFragments`() {
        val cbClass = FragmentManager.FragmentLifecycleCallbacks::class.java
        assertFalse(cbClass.isInterface, "FragmentLifecycleCallbacks must be an abstract class, not an interface")
        assertTrue(Modifier.isAbstract(cbClass.modifiers), "FragmentLifecycleCallbacks must be abstract")

        val fm = object : FragmentManager() {}
        val fragments = fm.fragments
        assertNotNull(fragments, "FragmentManager.getFragments() must not be null")
    }

    @Test
    fun `VideoClickAction supports zero-argument instantiation`() {
        val action = VideoClickAction()
        assertNotNull(action, "VideoClickAction() zero-arg constructor must exist")
    }

    @Test
    fun `TextUtil creates UiText`() {
        val text = txt("Hello")
        assertNotNull(text, "txt(String) must return non-null UiText")
        val context = Context()
        assertEquals("Hello", text.asString(context))
    }

    @Test
    fun `Dispatchers Main is available for plugins`() {
        val mainDispatcher = Dispatchers.Main
        assertNotNull(mainDispatcher, "Dispatchers.Main must be present")
    }

    @Test
    fun `Gson TypeToken is available for plugins`() {
        val typeTokenClass = Class.forName("com.google.gson.reflect.TypeToken")
        assertNotNull(typeTokenClass, "com.google.gson.reflect.TypeToken must be available")
    }

    @Test
    fun `PluginSecurityPolicy allows Executors for untrusted plugins while blocking OS execution`() {
        assertTrue(
            PluginSecurityPolicy.isClassAllowed("java.util.concurrent.Executors", isTrusted = false),
            "Executors must be allowed for untrusted plugins and routed through ExecutorsStub",
        )
        assertFalse(
            PluginSecurityPolicy.isClassAllowed("java.lang.ProcessBuilder", isTrusted = false),
            "ProcessBuilder must remain blocked",
        )
        assertFalse(
            PluginSecurityPolicy.isClassAllowed("java.lang.ProcessBuilder", isTrusted = true),
            "ProcessBuilder must remain blocked even for trusted plugins",
        )
        assertFalse(
            PluginSecurityPolicy.isClassAllowed("java.awt.Desktop", isTrusted = false),
            "Desktop must remain blocked",
        )
    }

    @Test
    fun `ExecutorsStub caps concurrent worker threads and enforces daemon priority under stress`() {
        val pool = ExecutorsStub.newFixedThreadPool(100)
        try {
            val totalTasks = 200
            val completed = AtomicInteger(0)
            val allDaemon = AtomicBoolean(true)
            val workerThreads = ConcurrentHashMap.newKeySet<String>()
            val doneLatch = CountDownLatch(totalTasks)
            for (i in 0 until totalTasks) {
                pool.execute {
                    val current = Thread.currentThread()
                    if (!current.isDaemon || current.priority > Thread.NORM_PRIORITY) {
                        allDaemon.set(false)
                    }
                    workerThreads.add(current.name)
                    Thread.yield()
                    completed.incrementAndGet()
                    doneLatch.countDown()
                }
            }
            assertTrue(doneLatch.await(10, TimeUnit.SECONDS), "all tasks must complete without deadlock")
            assertEquals(totalTasks, completed.get(), "every submitted task must execute once")
            assertTrue(allDaemon.get(), "every plugin worker thread must be daemon with normal priority")
            assertTrue(
                workerThreads.size <= ExecutorsStub.MAX_POOL_THREADS,
                "worker thread count (${workerThreads.size}) exceeded cap (${ExecutorsStub.MAX_POOL_THREADS})",
            )
        } finally {
            pool.shutdownNow()
        }
    }

    @Test
    fun `URLStub blocks local private and file URLs while allowing public addresses`() {
        val blockedUrls = listOf(
            "file:///C:/Windows/win.ini",
            "ftp://example.com/file.txt",
            "http://localhost:8080/admin",
            "http://127.0.0.1:9000/api",
            "http://10.0.0.1/router",
            "http://172.16.0.1/internal",
            "http://192.168.1.1/setup",
            "http://169.254.169.254/latest/meta-data/",
            "http://100.64.0.1/cgnat",
            "http://[::1]:8080/",
        )
        for (raw in blockedUrls) {
            assertFailsWith<SecurityException>("Expected $raw to be blocked by URLStub") {
                URLStub.validateUrl(URL(raw))
            }
        }
        val publicIp = InetAddress.getByAddress(byteArrayOf(1, 1, 1, 1))
        assertFalse(URLStub.isPrivateOrLocalAddress(publicIp), "1.1.1.1 must be recognized as public")
    }
}
