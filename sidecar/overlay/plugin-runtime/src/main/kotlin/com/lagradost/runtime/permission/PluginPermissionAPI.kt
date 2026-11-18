package com.lagradost.runtime.permission

import com.fasterxml.jackson.core.type.TypeReference
import com.fasterxml.jackson.module.kotlin.jacksonObjectMapper
import java.io.File
import java.util.concurrent.ConcurrentHashMap

/**
 * Clean management interface for inspecting and modifying plugin security permissions.
 */
interface PluginPermissionHandler {
    fun hasPermission(pluginId: String, permission: PluginPermission): Boolean
    fun hasPermission(pluginId: String, permissionName: String): Boolean
    fun grantPermission(pluginId: String, permission: PluginPermission)
    fun grantPermission(pluginId: String, permissionName: String)
    fun revokePermission(pluginId: String, permission: PluginPermission)
    fun getGrantedPermissions(pluginId: String): Set<PluginPermission>
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

    override fun grantPermission(pluginId: String, permission: PluginPermission) {
        grantPermissionInternal(pluginId, permission.id)
        println("[Security API] Granted permission '${permission.displayName}' to plugin $pluginId")
    }

    override fun grantPermission(pluginId: String, permissionName: String) {
        val perm = PluginPermission.fromIdOrName(permissionName)
        if (perm != null) {
            grantPermission(pluginId, perm)
        } else {
            grantPermissionInternal(pluginId, permissionName)
            println("[Security API] Granted legacy permission '$permissionName' to plugin $pluginId")
        }
    }

    override fun revokePermission(pluginId: String, permission: PluginPermission) {
        val grantedMap = getGrantedMap()
        val pluginPermissions = grantedMap[pluginId] ?: return
        if (pluginPermissions.remove(permission.id) || pluginPermissions.remove(permission.displayName)) {
            grantedMap[pluginId] = pluginPermissions
            saveGrantedMap(grantedMap)
            println("[Security API] Revoked permission '${permission.displayName}' from plugin $pluginId")
        }
    }

    override fun getGrantedPermissions(pluginId: String): Set<PluginPermission> {
        val grantedMap = getGrantedMap()
        val rawList = grantedMap[pluginId] ?: return emptySet()
        return rawList.mapNotNull { PluginPermission.fromIdOrName(it) }.toSet()
    }

    private fun grantPermissionInternal(pluginId: String, permString: String) {
        val grantedMap = getGrantedMap()
        val pluginPermissions = grantedMap[pluginId] ?: mutableListOf()
        if (!pluginPermissions.contains(permString)) {
            pluginPermissions.add(permString)
            grantedMap[pluginId] = pluginPermissions
            saveGrantedMap(grantedMap)
        }
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

    private fun saveGrantedMap(map: ConcurrentHashMap<String, MutableList<String>>) {
        synchronized(lock) {
            try {
                storeFile.parentFile?.mkdirs()
                storeFile.writeText(mapper.writeValueAsString(map))
            } catch (error: Exception) {
                error.printStackTrace()
            }
        }
    }
}
