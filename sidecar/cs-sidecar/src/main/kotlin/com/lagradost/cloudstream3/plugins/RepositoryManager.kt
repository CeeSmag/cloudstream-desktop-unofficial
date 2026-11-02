package com.lagradost.cloudstream3.plugins

import com.lagradost.cloudstream3.app
import com.lagradost.cloudstream3.ui.settings.extensions.RepositoryData
import sidecar.AddedRepositories

/**
 * Desktop stand-in for CloudStream's `RepositoryManager`: aggregator plugins call it to read and
 * register repositories. Registered repositories are queued for the host app instead of being
 * written to Android preferences.
 */
object RepositoryManager {
    private const val SHORTCODE_REDIRECT = "https://cutt.ly/"

    /** Downloads and parses a repository manifest; `null` when it cannot be read. */
    suspend fun parseRepository(url: String): Repository? {
        return try {
            val manifestUrl = if (url.startsWith("http")) url else SHORTCODE_REDIRECT + url
            val repository = app.get(manifestUrl).parsedSafe<Repository>()
            if (repository == null) System.err.println("[sidecar] repository manifest not readable: $url")
            repository
        } catch (failure: Throwable) {
            System.err.println("[sidecar] cannot read repository $url: $failure")
            null
        }
    }

    /** Queues a repository for the host app. */
    suspend fun addRepository(repository: RepositoryData) {
        val name = repository.name.ifBlank { repository.url }
        AddedRepositories.add(RepositoryData(repository.iconUrl?.trim()?.ifEmpty { null }, name, repository.url))
    }

    /** Repositories plugins added during this run; the app keeps the authoritative list. */
    fun getRepositories(): Array<RepositoryData> =
        AddedRepositories.known().map { RepositoryData(null, it, it) }.toTypedArray()
}
