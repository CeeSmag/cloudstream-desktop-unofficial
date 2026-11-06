package com.lagradost.cloudstream3

import android.annotation.Implemented
import android.content.Context

@Implemented
class AcraApplication {
    companion object {
        @JvmStatic
        var context: Context? = Context()

        inline fun <reified T> getKey(key: String): T? {
            return null
        }

        @JvmStatic
        fun setKey(key: String, value: Any?) {
        }
    }
}
