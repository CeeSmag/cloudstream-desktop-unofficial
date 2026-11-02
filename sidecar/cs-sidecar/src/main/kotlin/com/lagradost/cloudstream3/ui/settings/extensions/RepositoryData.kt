package com.lagradost.cloudstream3.ui.settings.extensions

import com.fasterxml.jackson.annotation.JsonProperty

/** A repository the user subscribed to; same shape as CloudStream's class so plugins compile against it. */
data class RepositoryData(
    @JsonProperty("iconUrl") val iconUrl: String?,
    @JsonProperty("name") val name: String,
    @JsonProperty("url") val url: String,
) {
    constructor(name: String, url: String) : this(null, name, url)
}
