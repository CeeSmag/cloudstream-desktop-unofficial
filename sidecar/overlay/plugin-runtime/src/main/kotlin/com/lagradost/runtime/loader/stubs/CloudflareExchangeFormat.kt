package com.lagradost.runtime.loader.stubs

/**
 * The text of the verification files exchanged with the app (one `key=value` per line, no JSON so no library
 * is needed). Pure: [CloudflareSolver] does the file work.
 *
 * Request (`request-<id>.txt`, written here): `id`, `host`, `url`, `method`, and optionally
 * `rejected=<pass id>` (the site refused the saved pass the app handed out: the app retires it and checks
 * the site again) and `need=browser` (this origin is reached only through the browser window, so a cookie
 * pass would not help).
 *
 * Result (`result-<id>.txt`, written by the app): `ok=true|false`, `userAgent`, `reason`, `retry=true`,
 * `transport=browser`, one `cookie=name=value` per cookie, and optionally `source=saved` with `pass=<id>`
 * (answered from a pass the app kept from an earlier check) and `validUntil=<epoch ms>` (when the pass's
 * clearance ends).
 */
object CloudflareExchangeFormat {
    /** Where a clearance came from. */
    enum class Source { BROWSER, SAVED }

    /** Why a verification is asked for. */
    data class Ask(val rejectedPass: String? = null, val needsBrowser: Boolean = false)

    /** Named in `rejected=` when the refused saved pass came without a readable id. */
    const val UNNAMED_PASS = "unnamed"

    private val SAFE_ID = Regex("[A-Za-z0-9-]{1,64}")

    /** True when [id] can be written to and read from the exchange (the app checks the same rule). */
    fun isSafeId(id: String): Boolean = SAFE_ID.matches(id)

    /** The request file for one challenged request. */
    fun requestText(id: String, host: String, url: String, method: String, ask: Ask): String = buildString {
        append("id=$id\nhost=$host\nurl=$url\nmethod=$method\n")
        ask.rejectedPass?.let { append("rejected=${if (isSafeId(it)) it else UNNAMED_PASS}\n") }
        if (ask.needsBrowser) append("need=browser\n")
    }

    private fun value(lines: List<String>, key: String): String? =
        lines.firstOrNull { it.startsWith("$key=") }?.removePrefix("$key=")?.trim()

    /**
     * Reads what the app wrote; null for `ok=false` or an answer without an agent. Cookies are needed unless
     * the app says `transport=browser` (the browser was not challenged, so it has none to give).
     */
    fun parseClearance(text: String, obtainedAt: Long): CloudflareSolver.Clearance? {
        val lines = text.lines().filter { it.contains("=") }
        if (lines.none { it.trim() == "ok=true" }) return null
        val cookies = lines.filter { it.startsWith("cookie=") }
            .map { it.removePrefix("cookie=") }
            .filter { it.contains("=") }
            .associate { it.substringBefore("=").trim() to it.substringAfter("=").trim() }
        val agent = value(lines, "userAgent").orEmpty()
        val browserOnly = lines.any { it.trim() == "transport=browser" }
        if (agent.isEmpty() || (cookies.isEmpty() && !browserOnly)) return null
        val saved = value(lines, "source") == "saved"
        return CloudflareSolver.Clearance(
            cookies = if (browserOnly) emptyMap() else cookies,
            userAgent = agent,
            obtainedAt = obtainedAt,
            browserOnly = browserOnly,
            source = if (saved) Source.SAVED else Source.BROWSER,
            passId = value(lines, "pass")?.takeIf { saved && isSafeId(it) },
            validUntil = value(lines, "validUntil")?.toLongOrNull(),
        )
    }

    /** The app's reason for not verifying, or a default. */
    fun reason(text: String): String =
        value(text.lines(), "reason") ?: "the app's answer had no usable cookies"

    /** `retry=true`: not the viewer's choice (the app only wants a go-ahead first), so ask again next time. */
    fun isRetry(text: String): Boolean = text.lines().any { it.trim() == "retry=true" }
}
