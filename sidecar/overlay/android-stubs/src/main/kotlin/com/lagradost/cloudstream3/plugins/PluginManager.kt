package com.lagradost.cloudstream3.plugins

object PluginManager {
    fun getPluginsOnline(): Array<PluginData> = emptyArray()
    fun getPluginsLocal(): Array<PluginData> = emptyArray()
    fun unloadPlugin(absolutePath: String) {}
}

data class PluginData(
    val internalName: String,
    val url: String?,
    val isOnline: Boolean,
    val filePath: String,
    val version: Int,
)
