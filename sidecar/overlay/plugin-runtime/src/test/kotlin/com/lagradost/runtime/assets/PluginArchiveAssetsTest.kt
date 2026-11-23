package com.lagradost.runtime.assets

import android.content.Context
import android.content.res.AssetManager
import android.content.res.Resources
import com.lagradost.cloudstream3.plugins.Plugin
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.util.concurrent.Callable
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame
import kotlin.test.assertTrue

/**
 * A plugin's bundled files through [AssetManager] over [PluginArchiveAssets], the way Eclipsia reads
 * `plugin.resources.assets.open("default-providers.json")` and its `scrapers/` scripts.
 */
class PluginArchiveAssetsTest {

    @TempDir
    lateinit var tempDir: File

    private val providers = """{"providers":["a","b"]}""".toByteArray()
    private val script = "export const name = 'x';".toByteArray()

    /** Shaped like Eclipsia.cs3: code, manifest, assets with a subfolder, plus crafted names. */
    private fun archive(extra: Map<String, ByteArray> = emptyMap()): File {
        val file = File(tempDir, "Plugin${extra.size}.cs3")
        ZipOutputStream(file.outputStream()).use { zip ->
            val entries = linkedMapOf(
                "manifest.json" to """{"requiresResources":true}""".toByteArray(),
                "classes.dex" to byteArrayOf(1, 2, 3),
                "assets/" to ByteArray(0),
                "assets/default-providers.json" to providers,
                "assets/scrapers/" to ByteArray(0),
                "assets/scrapers/alpha.js" to script,
                "assets/scrapers/beta.js" to script,
            ) + extra
            for ((name, bytes) in entries) {
                zip.putNextEntry(ZipEntry(name))
                zip.write(bytes)
                zip.closeEntry()
            }
        }
        return file
    }

    private fun assetsOf(file: File) = AssetManager(PluginArchiveAssets(file))

    @Test
    fun `open reads the exact bytes of top-level and nested assets`() {
        val assets = assetsOf(archive())
        assertContentEquals(providers, assets.open("default-providers.json").readBytes())
        assertContentEquals(script, assets.open("scrapers/alpha.js").readBytes())
        assertContentEquals(script, assets.open("scrapers/beta.js", AssetManager.ACCESS_BUFFER).readBytes())
    }

    @Test
    fun `list gives direct children, root included, and nothing for unknown folders`() {
        val assets = assetsOf(archive())
        assertEquals(listOf("default-providers.json", "scrapers"), assets.list("").toList())
        assertEquals(listOf("alpha.js", "beta.js"), assets.list("scrapers").toList())
        assertEquals(listOf("alpha.js", "beta.js"), assets.list("scrapers/").toList())
        assertEquals(emptyList(), assets.list("nothing").toList())
        assertEquals(emptyList(), assets.list("default-providers.json").toList())
    }

    @Test
    fun `missing files and folders are FileNotFoundException, as on Android`() {
        val assets = assetsOf(archive())
        assertFailsWith<FileNotFoundException> { assets.open("absent.json") }
        assertFailsWith<FileNotFoundException> { assets.open("scrapers") }
        assertFailsWith<FileNotFoundException> { assets.open("scrapers/") }
    }

    @Test
    fun `names that could leave assets are refused before the archive is read`() {
        val assets = assetsOf(archive())
        val escapes = listOf(
            "../classes.dex", "../manifest.json", "scrapers/../default-providers.json", "./default-providers.json",
            "/default-providers.json", "scrapers//alpha.js", "scrapers\\alpha.js", "default-providers.json\u0000",
            "", ".", "..",
        )
        for (name in escapes) {
            assertFailsWith<FileNotFoundException>("open('$name')") { assets.open(name) }
        }
        for (name in listOf("..", "../", "/", "scrapers/..", "./scrapers", "a\\b")) {
            assertFailsWith<FileNotFoundException>("list('$name')") { assets.list(name) }
        }
    }

    @Test
    fun `crafted entry names in the archive are not offered by list`() {
        val crafted = archive(mapOf("assets/../evil.js" to script, "assets/..\\evil.js" to script))
        assertEquals(listOf("default-providers.json", "scrapers"), assetsOf(crafted).list("").toList())
    }

    @Test
    fun `an asset larger than the cap is refused while inflating`() {
        val big = archive(mapOf("assets/big.bin" to ByteArray(64 * 1024)))
        val capped = AssetManager(PluginArchiveAssets(big, maxBytes = 32 * 1024))
        assertFailsWith<IOException> { capped.open("big.bin") }
        assertContentEquals(providers, capped.open("default-providers.json").readBytes())
    }

    @Test
    fun `a missing or broken archive has no assets`() {
        val missing = assetsOf(File(tempDir, "gone.cs3"))
        assertFailsWith<FileNotFoundException> { missing.open("default-providers.json") }
        assertEquals(emptyList(), missing.list("").toList())
        val broken = File(tempDir, "broken.cs3").apply { writeText("not a zip") }
        assertFailsWith<FileNotFoundException> { assetsOf(broken).open("default-providers.json") }
        assertEquals(emptyList(), assetsOf(broken).list("").toList())
    }

    @Test
    fun `the archive is not kept open, so it can be replaced after reads`() {
        val file = archive()
        val assets = assetsOf(file)
        assets.open("default-providers.json").readBytes()
        // A held handle would keep Windows from deleting it; on every OS the next read must see the new file.
        assertTrue(file.delete())
        assertFailsWith<FileNotFoundException> { assets.open("default-providers.json") }
        assertEquals(file, archive(), "rebuilt at the same path")
        assertContentEquals(providers, assets.open("default-providers.json").readBytes())
    }

    @Test
    fun `concurrent readers each get whole, correct assets`() {
        // No shared state by design; this checks it under load (one AssetManager, many threads).
        val assets = assetsOf(archive())
        val pool = Executors.newFixedThreadPool(16)
        try {
            val tasks = (0 until 400).map { i ->
                Callable {
                    if (i % 2 == 0) {
                        assets.open("default-providers.json").readBytes().contentEquals(providers)
                    } else {
                        assets.list("scrapers").toList() == listOf("alpha.js", "beta.js")
                    }
                }
            }
            val results = pool.invokeAll(tasks, 60, TimeUnit.SECONDS).map { it.get() }
            assertEquals(400, results.count { it })
        } finally {
            pool.shutdownNow()
        }
    }

    @Test
    fun `stub resources and contexts without an archive have empty assets`() {
        val plain = Resources()
        assertEquals(emptyList(), plain.assets.list("").toList())
        assertFailsWith<FileNotFoundException> { plain.assets.open("default-providers.json") }
        assertEquals(emptyList(), Context(File(tempDir, "ctx")).assets.list("").toList())
        assertEquals(emptyList(), AssetManager().list("").toList())
    }

    private class TestPlugin : Plugin()

    @Test
    fun `a plugin gets resources over its own archive, and the context's without one`() {
        val context = Context(File(tempDir, "ctx"))
        val plugin = TestPlugin().apply { filename = archive().absolutePath }
        val resources = PluginAssets.resourcesFor(plugin, context)
        assertContentEquals(providers, resources.assets.open("default-providers.json").readBytes())

        val noFile = TestPlugin()
        assertSame(context.resources, PluginAssets.resourcesFor(noFile, context))
        val gone = TestPlugin().apply { filename = File(tempDir, "gone.cs3").absolutePath }
        assertSame(context.resources, PluginAssets.resourcesFor(gone, context))
    }

    /**
     * The real plugin, when at hand: `CS_TEST_ECLIPSIA_CS3=<path to Eclipsia.cs3>`. Reads what
     * Eclipsia reads first when it looks for streams.
     */
    @Test
    fun `real eclipsia archive serves its scraper list and scripts`() {
        val file = System.getenv("CS_TEST_ECLIPSIA_CS3")?.let(::File)?.takeIf { it.isFile } ?: return
        val assets = assetsOf(file)
        assertTrue(assets.open("default-providers.json").readBytes().isNotEmpty())
        val scrapers = assets.list("scrapers")
        assertTrue(scrapers.isNotEmpty(), "scrapers/ listed")
        assertTrue(assets.open("scrapers/${scrapers.first()}").readBytes().isNotEmpty())
    }
}
