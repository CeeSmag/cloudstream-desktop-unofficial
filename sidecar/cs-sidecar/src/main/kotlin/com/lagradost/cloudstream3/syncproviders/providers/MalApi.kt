package com.lagradost.cloudstream3.syncproviders.providers

import com.lagradost.cloudstream3.syncproviders.SyncAPI

/** The MyAnimeList service stub for desktop extension loading. */
class MalApi : SyncAPI() {
    override val name = "MyAnimeList"
    override val idPrefix = "mal"
}
