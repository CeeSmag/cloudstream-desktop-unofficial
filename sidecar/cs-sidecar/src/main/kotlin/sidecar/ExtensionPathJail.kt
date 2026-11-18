package sidecar

import java.io.File

/**
 * Ensures a plugin path the host asks us to load stays under the installed-plugins folder.
 *
 * The JVM sandbox data dir is `<appdata>/sidecar` (`cloudstream.data.dir`). The host stores
 * `.cs3` files next to that at `<appdata>/extensions` ([AppPaths::extensions_dir]).
 */
object ExtensionPathJail {
    fun requireInsideExtensions(path: String): File {
        val file = File(path)
        require(file.isFile) { "extension file not found: $path" }
        val dataDir = System.getProperty("cloudstream.data.dir")
            ?: throw SecurityException("cloudstream.data.dir is not set; refusing to load plugins")
        val extensionsDir = hostExtensionsDir(File(dataDir))
        val canonical = file.canonicalFile
        val prefix = extensionsDir.path.trimEnd(File.separatorChar) + File.separatorChar
        require(canonical.path.startsWith(prefix) || canonical.path == extensionsDir.path) {
            "plugin path escapes the extensions folder: $path"
        }
        require(canonical.name.matches(SAFE_FILE_NAME)) {
            "unsafe plugin file name: ${canonical.name}"
        }
        return canonical
    }

    /** `<appdata>/extensions` when data dir is `<appdata>/sidecar`. */
    internal fun hostExtensionsDir(dataDir: File): File {
        val appData = dataDir.canonicalFile.parentFile
            ?: throw SecurityException("cloudstream.data.dir has no parent; refusing to load plugins")
        return File(appData, "extensions").canonicalFile
    }

    private val SAFE_FILE_NAME = Regex("^[A-Za-z0-9_-]+\\.cs3$")
}
