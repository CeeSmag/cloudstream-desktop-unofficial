package sidecar

import com.lagradost.cloudstream3.ui.settings.extensions.RepositoryData
import java.util.concurrent.ConcurrentLinkedQueue

/**
 * Repositories that plugins asked the app to add (`RepositoryManager.addRepository`).
 * The Rust side collects them with the `take_added_repositories` call and saves them.
 */
object AddedRepositories {
    private val pending = ConcurrentLinkedQueue<RepositoryData>()
    private val everSeen = java.util.concurrent.ConcurrentHashMap.newKeySet<String>()

    /** Queues a repository; one that was already queued earlier in this run is ignored. */
    fun add(repository: RepositoryData) {
        if (everSeen.add(repository.url)) pending.add(repository)
    }

    /** Everything queued since the last call; the queue is emptied. */
    fun take(): List<RepositoryData> = generateSequence { pending.poll() }.toList()

    /** Every repository a plugin added during this run, including those already handed over. */
    fun known(): List<String> = everSeen.toList()
}
