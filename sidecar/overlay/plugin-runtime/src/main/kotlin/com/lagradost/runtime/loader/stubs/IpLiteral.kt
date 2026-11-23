package com.lagradost.runtime.loader.stubs

/**
 * Strict parser for textual IP addresses, used so plugin input never reaches the JDK's lenient
 * name parsing (which also accepts `127.1`, and falls back to DNS for anything else).
 *
 * Accepted: IPv4 dotted-quad (four decimal parts 0..255, no leading zeros, so `017` is not
 * octal-ambiguous) and IPv6 (RFC 4291 text forms: 1..4 hex digits per group, at most one `::`,
 * optional trailing dotted-quad), IPv6 optionally in brackets (as in a URL; the JDK refuses brackets
 * around IPv4, so do we). Zone ids (`%eth0`) are rejected.
 * Anything else returns `null` and is treated as a host name by the caller.
 *
 * Pure function, no state.
 */
object IpLiteral {

    fun parse(text: String): ByteArray? {
        val bracketed = text.length >= 2 && text.startsWith("[") && text.endsWith("]")
        val body = if (bracketed) text.substring(1, text.length - 1) else text
        if (body.isEmpty()) return null
        return when {
            body.contains(':') -> parseV6(body)
            bracketed -> null
            else -> parseV4(body)
        }
    }

    fun parseV4(text: String): ByteArray? {
        val parts = text.split('.')
        if (parts.size != 4) return null
        val out = ByteArray(4)
        for ((i, part) in parts.withIndex()) {
            out[i] = (decimalOctet(part) ?: return null).toByte()
        }
        return out
    }

    private fun decimalOctet(part: String): Int? {
        if (part.isEmpty() || part.length > 3 || !part.all { it in '0'..'9' }) return null
        if (part.length > 1 && part[0] == '0') return null
        val value = part.toInt()
        return if (value <= 255) value else null
    }

    private fun parseV6(text: String): ByteArray? {
        val halves = text.split("::")
        val words = when (halves.size) {
            1 -> words(halves[0], v4AllowedLast = true)?.takeIf { it.size == 8 }
            2 -> {
                // A dotted-quad may only end the whole address, so never before "::".
                val head = words(halves[0], v4AllowedLast = false) ?: return null
                val tail = words(halves[1], v4AllowedLast = true) ?: return null
                val explicit = head.size + tail.size
                if (explicit > 7) null else head + List(8 - explicit) { 0 } + tail
            }
            else -> null
        } ?: return null
        val out = ByteArray(16)
        for ((i, word) in words.withIndex()) {
            out[2 * i] = (word shr 8).toByte()
            out[2 * i + 1] = word.toByte()
        }
        return out
    }

    /**
     * 16-bit words of one side of `::`. `""` has none; an empty group (`1::2:` or `:1`) is invalid.
     * With [v4AllowedLast] the final group may be a dotted-quad, which counts as two words.
     */
    private fun words(side: String, v4AllowedLast: Boolean): List<Int>? {
        if (side.isEmpty()) return emptyList()
        val groups = side.split(':')
        val out = ArrayList<Int>(8)
        for ((i, group) in groups.withIndex()) {
            if (v4AllowedLast && i == groups.lastIndex && group.contains('.')) {
                val v4 = parseV4(group) ?: return null
                out += ((v4[0].toInt() and 0xFF) shl 8) or (v4[1].toInt() and 0xFF)
                out += ((v4[2].toInt() and 0xFF) shl 8) or (v4[3].toInt() and 0xFF)
            } else {
                out += hexWord(group) ?: return null
            }
        }
        return out
    }

    private fun hexWord(group: String): Int? {
        if (group.isEmpty() || group.length > 4) return null
        if (!group.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) return null
        return group.toInt(16)
    }
}
