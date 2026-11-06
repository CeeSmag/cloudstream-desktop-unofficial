package com.lagradost.cloudstream3.actions

open class VideoClickAction @JvmOverloads constructor(
    open val name: String = "",
    open val iconId: Int = 0,
    open val requiresAuthentication: Boolean = false,
    open val callback: () -> Unit = {},
) {
    open val oneSource: Boolean = false
    open val isPlayer: Boolean = false
    var sourcePlugin: String? = null
}
