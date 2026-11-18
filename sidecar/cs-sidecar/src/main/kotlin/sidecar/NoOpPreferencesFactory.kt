package sidecar

import java.util.prefs.AbstractPreferences
import java.util.prefs.Preferences
import java.util.prefs.PreferencesFactory

/**
 * In-memory prefs so the JVM never opens the Windows registry.
 * AppContainer denies `HKCU\Software\JavaSoft\Prefs` (error 5); without this,
 * `Preferences.userRoot()` aborts plugin load.
 */
class NoOpPreferencesFactory : PreferencesFactory {
    private val root = NoOpPreferences()

    override fun systemRoot(): Preferences = root

    override fun userRoot(): Preferences = root
}

private class NoOpPreferences : AbstractPreferences(null, "") {
    private val map = mutableMapOf<String, String>()

    override fun putSpi(key: String, value: String) {
        map[key] = value
    }

    override fun getSpi(key: String): String? = map[key]

    override fun removeSpi(key: String) {
        map.remove(key)
    }

    override fun removeNodeSpi() {}

    override fun keysSpi(): Array<String> = map.keys.toTypedArray()

    override fun childrenNamesSpi(): Array<String> = emptyArray()

    override fun childSpi(name: String): AbstractPreferences = this

    override fun syncSpi() {}

    override fun flushSpi() {}
}
