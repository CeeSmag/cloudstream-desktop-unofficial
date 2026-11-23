package apiscanner

import com.fasterxml.jackson.databind.SerializationFeature
import com.fasterxml.jackson.module.kotlin.jacksonObjectMapper
import java.io.File

/** One gap across all plugins: a reference with a status other than RESOLVED, and who needs it. */
data class Gap(val reference: String, val kind: RefKind, val status: Status, val pluginCount: Int, val plugins: List<String>)

/** The whole run. [version] changes when the JSON layout does. */
data class ScanReport(
    val version: Int,
    val scannedPlugins: Int,
    val failedPlugins: Int,
    val gaps: List<Gap>,
    val plugins: List<PluginReport>,
) {
    companion object {
        const val VERSION = 1

        /** Folds per-plugin results (in any order) into one report, gaps ranked by how many plugins need them. */
        fun of(reports: List<PluginReport>): ScanReport {
            val sorted = reports.sortedBy { it.identity.internalName.ifEmpty { it.file } }
            val byGap = LinkedHashMap<GapKey, MutableSet<String>>()
            for (report in sorted) {
                val name = report.identity.internalName.ifEmpty { report.file }
                for (finding in report.findings) {
                    val key = GapKey(finding.reference.display(), finding.reference.kind, finding.status)
                    byGap.getOrPut(key) { sortedSetOf() }.add(name)
                }
            }
            val gaps = byGap.map { (key, plugins) ->
                Gap(key.reference, key.kind, key.status, plugins.size, plugins.toList())
            }.sortedWith(compareByDescending<Gap> { it.pluginCount }.thenBy { it.status }.thenBy { it.reference })
            return ScanReport(VERSION, sorted.size, sorted.count { it.error != null }, gaps, sorted)
        }
    }
}

private data class GapKey(val reference: String, val kind: RefKind, val status: Status)

/** Writes `api_gaps.json` and `api_gaps.md` into a folder. */
object ReportWriter {

    private val mapper = jacksonObjectMapper().enable(SerializationFeature.INDENT_OUTPUT)

    fun write(report: ScanReport, folder: File) {
        folder.mkdirs()
        File(folder, "api_gaps.json").writeText(mapper.writeValueAsString(report))
        File(folder, "api_gaps.md").writeText(markdown(report))
    }

    /** What to implement first: per status, the gaps most plugins hit, then the sandbox refusals. */
    fun markdown(report: ScanReport, perStatus: Int = 40): String = buildString {
        appendLine("# Android / CloudStream API gaps")
        appendLine()
        appendLine("Scanned ${report.scannedPlugins} video plugins (${report.failedPlugins} could not be scanned).")
        appendLine()
        for (status in Status.entries.filter { it != Status.RESOLVED }) {
            val rows = report.gaps.filter { it.status == status }
            appendLine("## ${title(status)} (${rows.size})")
            appendLine()
            if (rows.isEmpty()) {
                appendLine("None.")
            } else {
                appendLine("| Plugins | Reference | Used by |")
                appendLine("|---:|---|---|")
                for (gap in rows.take(perStatus)) {
                    appendLine("| ${gap.pluginCount} | `${gap.reference}` | ${sample(gap.plugins)} |")
                }
                if (rows.size > perStatus) appendLine("\n…and ${rows.size - perStatus} more in api_gaps.json.")
            }
            appendLine()
        }
        appendRefusals(report)
    }

    private fun StringBuilder.appendRefusals(report: ScanReport) {
        val refused = report.plugins.filter { it.sandboxRefusals.isNotEmpty() || it.error != null }
        appendLine("## Not loadable untrusted, or not scanned (${refused.size})")
        appendLine()
        if (refused.isEmpty()) appendLine("None.")
        for (plugin in refused) {
            val name = plugin.identity.internalName.ifEmpty { plugin.file }
            val why = plugin.error ?: plugin.sandboxRefusals.first()
            appendLine("- **$name**: ${why.replace("\n", " ")}")
        }
    }

    private fun title(status: Status) = when (status) {
        Status.MISSING_MEMBER -> "Missing members (fail at run time)"
        Status.GHOST_CLASS -> "Missing classes (ghost stubs)"
        Status.NEUTRALISED -> "Neutralised UI calls (silently do nothing)"
        Status.BROKEN_STUB -> "Stub classes whose members cannot be listed"
        Status.RESOLVED -> "Resolved"
    }

    private fun sample(plugins: List<String>, limit: Int = 5): String {
        val shown = plugins.take(limit).joinToString(", ")
        return if (plugins.size > limit) "$shown, +${plugins.size - limit}" else shown
    }
}
