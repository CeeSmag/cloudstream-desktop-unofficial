package com.lagradost.cloudstream3.plugins

import com.fasterxml.jackson.annotation.JsonProperty

/** The manifest (`repo.json`) of an extension repository; same shape as CloudStream's class. */
data class Repository(
    @JsonProperty("iconUrl") val iconUrl: String?,
    @JsonProperty("name") val name: String,
    @JsonProperty("description") val description: String?,
    @JsonProperty("manifestVersion") val manifestVersion: Int,
    @JsonProperty("pluginLists") val pluginLists: List<String>,
)
