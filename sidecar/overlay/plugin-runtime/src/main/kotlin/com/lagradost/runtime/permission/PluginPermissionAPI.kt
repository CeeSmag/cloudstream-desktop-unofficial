package com.lagradost.runtime.permission

import com.fasterxml.jackson.core.type.TypeReference
import com.fasterxml.jackson.module.kotlin.jacksonObjectMapper
import java.io.File
import java.util.concurrent.ConcurrentHashMap

/**
 * Clean management interface for inspecting plugin security permissions.
 */
interface PluginPermissionHandler {
    fun hasPermission(pluginId: String, permission: PluginPermission): Boolean
    fun hasPermission(pluginId: String, permissionName: String): Boolean
}

/**
 * File-backed permission store under `cloudstream.data.dir`.
 *
 * The upstream used `java.util.prefs` (Windows registry). AppContainer cannot open
 * `HKCU\Software\JavaSoft\Prefs`, which crashed plugin load with ExceptionInInitializerError.
 */
object PluginPermissionAPI : PluginPermissionHandler {
    private val mapper = jacksonObjectMapper()
    private val lock = Any()

    private val storeFile: File by lazy {
        val dataDir = System.getProperty("cloudstream.data.dir")
            ?: System.getProperty("java.io.tmpdir")
            ?: "."
        val dir = File(dataDir, "plugin_permissions")
        dir.mkdirs()
        File(dir, "granted.json")
    }

    override fun hasPermission(pluginId: String, permission: PluginPermission): Boolean {
        val grantedMap = getGrantedMap()
        val pluginPermissions = grantedMap[pluginId] ?: return false
        return pluginPermissions.contains(permission.id) || pluginPermissions.contains(permission.displayName)
    }

    override fun hasPermission(pluginId: String, permissionName: String): Boolean {
        val perm = PluginPermission.fromIdOrName(permissionName)
        if (perm != null) {
            return hasPermission(pluginId, perm)
        }
        val grantedMap = getGrantedMap()
        val pluginPermissions = grantedMap[pluginId] ?: return false
        return pluginPermissions.contains(permissionName) || pluginPermissions.contains("Network Sockets")
    }

    private fun getGrantedMap(): ConcurrentHashMap<String, MutableList<String>> {
        synchronized(lock) {
            if (!storeFile.isFile) {
                return ConcurrentHashMap()
            }
            return try {
                mapper.readValue(
                    storeFile.readText(),
                    object : TypeReference<ConcurrentHashMap<String, MutableList<String>>>() {},
                )
            } catch (_: Exception) {
                ConcurrentHashMap()
            }
        }
    }
}
