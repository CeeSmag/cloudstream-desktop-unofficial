package com.lagradost.cloudstream3.syncproviders.providers

import com.lagradost.cloudstream3.syncproviders.SyncAPI

/** The Simkl watchlist service; signing in is not supported on the desktop sidecar. */
class SimklApi : SyncAPI() {
    override val name = "Simkl"
    override val idPrefix = "simkl"
}
