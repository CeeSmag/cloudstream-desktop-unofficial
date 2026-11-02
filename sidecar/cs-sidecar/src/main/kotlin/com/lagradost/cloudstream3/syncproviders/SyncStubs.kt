package com.lagradost.cloudstream3.syncproviders

import com.lagradost.cloudstream3.utils.UiText

/*
 * Account and list-sync classes that plugins link against (for example to import a Simkl
 * watchlist). The desktop sidecar has no accounts, so every lookup reports "not signed in";
 * the shapes match CloudStream's so plugins load and treat the answer as an ordinary failure.
 */

/** The signed-in user of a sync service. */
data class AuthUser(val name: String?, val id: Int, val profilePicture: String? = null)

/** Base of every account-based service. */
abstract class AuthAPI {
    abstract val name: String
    abstract val idPrefix: String
    open val requiresLogin: Boolean = true
}

/** A service that keeps the user's lists (watching, completed, ...). */
abstract class SyncAPI : AuthAPI() {
    /** One entry of a list. */
    data class LibraryItem(val name: String, val syncId: String, val url: String, val posterUrl: String? = null)

    /** A named list of entries. */
    data class LibraryList(val name: UiText, val items: List<LibraryItem>)

    /** Everything the service holds for the user. */
    data class LibraryMetadata(val allLibraryLists: List<LibraryList>)
}

/** Answers for a signed-in user; there never is one on the desktop sidecar. */
open class AuthRepo {
    open fun authUser(): AuthUser? = null
}

/** Reads the lists of one [SyncAPI]. */
class SyncRepo(val api: SyncAPI) : AuthRepo() {
    suspend fun library(): Result<SyncAPI.LibraryMetadata> =
        Result.failure(IllegalStateException("${api.name}: no account is signed in"))
}
