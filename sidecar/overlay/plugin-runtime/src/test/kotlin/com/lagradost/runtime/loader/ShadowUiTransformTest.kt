package com.lagradost.runtime.loader

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Cache acceptance in [ExtensionLoader] must track [ShadowUiTransform.VERSION], otherwise
 * StreamPlay/FlixProxy keeps a pre-remap JVM jar and fails verify with RequiresPermissionException.
 */
class ShadowUiTransformTest {
    @Test
    fun transformerVersionIsNonEmptyAndBumpable() {
        assertTrue(ShadowUiTransform.VERSION.isNotBlank())
        // Current sandbox remap pass (raw sockets + Thread stubs).
        assertEquals("4", ShadowUiTransform.VERSION)
    }
}
