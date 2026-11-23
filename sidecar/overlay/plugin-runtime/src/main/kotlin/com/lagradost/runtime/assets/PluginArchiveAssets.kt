package com.lagradost.runtime.assets

import android.content.Context
import android.content.res.AssetManager
import android.content.res.DesktopAssetSource
import android.content.res.Resources
import com.lagradost.cloudstream3.plugins.BasePlugin
import java.io.File
import java.io.IOException
import java.util.zip.ZipException
import java.util.zip.ZipFile

/**
 * The `assets/` folder of one plugin archive (`.cs3`), read-only.
 *
 * Lives in `com.lagradost.runtime.*`, which plugin bytecode may not reference or reflect on, so a
 * plugin cannot point it at another file; it only ever gets the instance bound to its own archive.
 * Paths are already checked by [AssetManager] (relative, no `..`); this only adds the `assets/` prefix.
 *
 * Every call opens and closes the archive: no handle stays open (on Windows one would keep the
 * plugin from being uninstalled), and there is no shared state, so any thread may call it.
 * Each asset is capped at [maxBytes], counted while inflating, so a forged size cannot get past it.
 */
class PluginArchiveAssets(
    private val archive: File,
    private val maxBytes: Int = MAX_ASSET_BYTES,
) : DesktopAssetSource {

    override fun read(path: String): ByteArray? = withArchive(missing = null) { zip ->
        val entry = zip.getEntry(PREFIX + path)
        if (entry == null || entry.isDirectory) return@withArchive null
        zip.getInputStream(entry).use { input -> readCapped(input, path) }
    }

    override fun list(dir: String): Array<String> = withArchive(missing = emptyArray()) { zip ->
        val prefix = if (dir.isEmpty()) PREFIX else "$PREFIX$dir/"
        zip.entries().asSequence()
            .map { it.name }
            .filter { it.startsWith(prefix) && it.length > prefix.length }
            .map { it.substring(prefix.length).substringBefore('/') }
            // Names a crafted archive may hold (`assets/../x`) that open() would refuse anyway.
            .filter { it.isNotEmpty() && it != "." && it != ".." && '\\' !in it }
            .distinct()
            .sorted()
            .toList()
            .toTypedArray()
    }

    private fun readCapped(input: java.io.InputStream, path: String): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val buffer = ByteArray(BUFFER_BYTES)
        var total = 0
        while (true) {
            val n = input.read(buffer)
            if (n < 0) break
            total += n
            if (total > maxBytes) {
                throw IOException("Plugin asset '$path' is larger than $maxBytes bytes")
            }
            out.write(buffer, 0, n)
        }
        return out.toByteArray()
    }

    /** Runs [block] on the open archive; [missing] when there is no readable archive at all. */
    private fun <T> withArchive(missing: T, block: (ZipFile) -> T): T {
        if (!archive.isFile) return missing
        val zip = try {
            ZipFile(archive)
        } catch (_: ZipException) {
            return missing
        }
        return zip.use(block)
    }

    companion object {
        const val PREFIX = "assets/"

        /** Largest single asset handed to a plugin (Eclipsia's biggest is a few hundred KB). */
        const val MAX_ASSET_BYTES = 32 * 1024 * 1024
        private const val BUFFER_BYTES = 16 * 1024
    }
}

/** What [com.lagradost.runtime.loader.ExtensionLoader.initializePlugin] gives a plugin as `resources`. */
object PluginAssets {

    /**
     * Resources whose assets are the plugin's own archive, like CloudStream's PluginManager does for
     * `requiresResources` (here for every plugin: reading one's own archive costs nothing until used).
     * Falls back to the context's resources when the plugin has no archive file.
     */
    @JvmStatic
    fun resourcesFor(plugin: BasePlugin, context: Context): Resources {
        val archive = plugin.filename?.let(::File)?.takeIf { it.isFile } ?: return context.resources
        return Resources(AssetManager(PluginArchiveAssets(archive)))
    }
}
