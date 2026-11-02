package com.lagradost.cloudstream3.syncproviders

import com.lagradost.cloudstream3.syncproviders.providers.SimklApi

/** Holder of the sync services; only the ones plugins are known to reach for are present. */
class AccountManager {
    companion object {
        @JvmStatic
        val simklApi = SimklApi()
    }
}
