package sidecar

import com.lagradost.runtime.loader.PluginSecurityDeniedException
import com.lagradost.runtime.security.PluginSecurityBlockedException
import com.lagradost.runtime.security.PluginSecurityVerifier
import com.lagradost.runtime.security.RequiresPermissionException
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse

class SidecarErrorEncodingTest {

    @Test
    fun `encodeError attaches capped violations for typed security exceptions only`() {
        val many = (1..25).map { "Disallowed class 'p.C$it'" }
        val blocked = encodeError(PluginSecurityBlockedException(many, "summary"))
        assertEquals("PluginSecurityBlockedException", blocked["type"])
        assertEquals("summary", blocked["message"])
        val encodedViolations = blocked["violations"] as List<*>
        assertEquals(PluginSecurityVerifier.MAX_REPORTED_VIOLATIONS, encodedViolations.size)
        assertEquals("Disallowed class 'p.C1'", encodedViolations.first())

        val perm = encodeError(RequiresPermissionException("Raw network sockets", "Raw socket access"))
        assertEquals("RequiresPermissionException", perm["type"])
        assertEquals(listOf("Raw socket access"), perm["violations"])

        val denied = encodeError(PluginSecurityDeniedException("java.lang.ProcessBuilder", "denied"))
        assertEquals("PluginSecurityDeniedException", denied["type"])
        assertEquals(listOf("Disallowed class 'java.lang.ProcessBuilder'"), denied["violations"])

        // Plain SecurityException (e.g. ExtensionPathJail) must not include structured violations.
        val jail = encodeError(SecurityException("cloudstream.data.dir is not set"))
        assertEquals("SecurityException", jail["type"])
        assertFalse("violations" in jail)
    }
}
