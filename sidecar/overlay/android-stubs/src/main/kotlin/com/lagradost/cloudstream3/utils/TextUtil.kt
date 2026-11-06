package com.lagradost.cloudstream3.utils

import android.content.Context

sealed class UiText {
    companion object {
        const val TAG = "UiText"
    }

    data class DynamicString(val value: String) : UiText() {
        override fun toString(): String = value
        override fun equals(other: Any?): Boolean {
            if (other !is DynamicString) return false
            return this.value == other.value
        }
        override fun hashCode(): Int = value.hashCode()
    }

    class StringResource(
        val resId: Int,
        val args: List<Any>
    ) : UiText() {
        override fun toString(): String = "resId = $resId"
        override fun equals(other: Any?): Boolean {
            if (other !is StringResource) return false
            return this.resId == other.resId && this.args == other.args
        }
        override fun hashCode(): Int = resId
    }

    fun asStringNull(context: Context?): String? {
        return context?.let { asString(it) }
    }

    fun asString(context: Context): String {
        return when (this) {
            is DynamicString -> value
            is StringResource -> {
                val str = context.getString(resId)
                if (args.isEmpty()) str else String.format(str, *args.toTypedArray())
            }
        }
    }
}

fun txt(value: String): UiText = UiText.DynamicString(value)

@JvmName("txtNull")
fun txt(value: String?): UiText? = value?.let { UiText.DynamicString(it) }

fun txt(resId: Int, vararg args: Any): UiText =
    UiText.StringResource(resId, args.toList())

@JvmName("txtNull")
fun txt(resId: Int?, vararg args: Any?): UiText? {
    if (resId == null || args.any { it == null }) return null
    return UiText.StringResource(resId, args.filterNotNull())
}
