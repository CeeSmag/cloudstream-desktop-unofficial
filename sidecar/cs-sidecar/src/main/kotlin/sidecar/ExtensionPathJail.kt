package sidecar

import java.io.File

/**
 * Ensures a plugin path the host asks us to load stays under `<data>/extensions`.
 * Defense in depth: the Rust side also jails names before RPC.
 */
object ExtensionPathJail {
    fun requireInsideExtensions(path: String): File {
        val file = File(path)
        require(file.isFile) { "extension file not found: $path" }
        val dataDir = System.getProperty("cloudstream.data.dir")
            ?: throw SecurityException("cloudstream.data.dir is not set; refusing to load plugins")
        val extensionsDir = File(dataDir, "extensions").canonicalFile
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

    private val SAFE_FILE_NAME = Regex("^[A-Za-z0-9_-]+\\.cs3$")
}
