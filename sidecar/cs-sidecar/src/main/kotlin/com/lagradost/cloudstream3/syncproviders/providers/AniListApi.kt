package com.lagradost.cloudstream3.syncproviders.providers

import com.lagradost.cloudstream3.syncproviders.SyncAPI

/** The AniList service stub for desktop extension loading. */
class AniListApi : SyncAPI() {
    override val name = "AniList"
    override val idPrefix = "anilist"
}
