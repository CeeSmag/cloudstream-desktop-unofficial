package com.lagradost.cloudstream3.syncproviders

import com.lagradost.cloudstream3.syncproviders.providers.AniListApi
import com.lagradost.cloudstream3.syncproviders.providers.MalApi
import com.lagradost.cloudstream3.syncproviders.providers.SimklApi

/** Holder of the sync services for extension compatibility. */
class AccountManager {
    companion object {
        @JvmStatic
        val simklApi = SimklApi()

        @JvmStatic
        val aniListApi = AniListApi()

        @JvmStatic
        val malApi = MalApi()
    }
}
