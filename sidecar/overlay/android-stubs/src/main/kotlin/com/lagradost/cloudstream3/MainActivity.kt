package com.lagradost.cloudstream3

import com.lagradost.cloudstream3.utils.Event

/**
 * Stub for CloudStream Android's MainActivity.
 * Required by plugins that reflectively access MainActivity.Companion.
 */
class MainActivity {
    companion object {
        @JvmStatic
        val afterPluginsLoadedEvent = Event<Boolean>()

        @JvmStatic
        val mainPluginsLoadedEvent = Event<Boolean>()

        @JvmStatic
        val afterRepositoryLoadedEvent = Event<Boolean>()

        @JvmStatic
        val bookmarksUpdatedEvent = Event<Boolean>()

        @JvmStatic
        val reloadHomeEvent = Event<Boolean>()

        @JvmStatic
        val reloadLibraryEvent = Event<Boolean>()

        @JvmStatic
        val reloadAccountEvent = Event<Boolean>()
    }
}
