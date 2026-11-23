package apiscanner

import com.fasterxml.jackson.module.kotlin.jacksonObjectMapper
import com.fasterxml.jackson.module.kotlin.readValue
import java.io.File
import java.util.concurrent.Callable
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlin.system.exitProcess

/**
 * `api-scanner --out <folder> [--workers N] [--identities <file.json>] <plugin.cs3 | folder>...`
 *
 * Scans each `.cs3` statically (nothing of the plugin runs) and writes `api_gaps.json` and
 * `api_gaps.md` into the output folder. `--identities` maps file names to
 * `{"internalName": ..., "repositoryUrl": ...}` (tools/extension_recorder writes it).
 * Exit code 0 when the report was written (gaps are findings, not errors), 2 on bad arguments.
 */
fun main(args: Array<String>) {
    val options = try {
        Options.parse(args.toList())
    } catch (error: IllegalArgumentException) {
        System.err.println("api-scanner: ${error.message}")
        exitProcess(2)
    }
    val report = ScanReport.of(scanAll(options))
    ReportWriter.write(report, options.out)
    println("api-scanner: ${report.scannedPlugins} plugins, ${report.gaps.size} gaps -> ${options.out}")
}

/** Command-line options. */
data class Options(val out: File, val workers: Int, val identities: Map<String, PluginIdentity>, val files: List<File>) {
    companion object {
        fun parse(args: List<String>): Options {
            var out: File? = null
            var workers = Runtime.getRuntime().availableProcessors().coerceIn(1, 8)
            var identities: Map<String, PluginIdentity> = emptyMap()
            val inputs = mutableListOf<File>()
            val rest = args.iterator()
            while (rest.hasNext()) {
                when (val arg = rest.next()) {
                    "--out" -> out = File(value(rest, arg))
                    "--workers" -> workers = value(rest, arg).toIntOrNull()?.takeIf { it > 0 }
                        ?: throw IllegalArgumentException("--workers needs a positive number")
                    "--identities" -> identities = readIdentities(File(value(rest, arg)))
                    else -> inputs += File(arg)
                }
            }
            val files = inputs.flatMap(::pluginFiles).distinct().sortedBy { it.name }
            require(out != null) { "--out <folder> is required" }
            require(files.isNotEmpty()) { "no .cs3 file given" }
            return Options(out, workers, identities, files)
        }

        private fun value(rest: Iterator<String>, flag: String): String =
            if (rest.hasNext()) rest.next() else throw IllegalArgumentException("$flag needs a value")

        private fun pluginFiles(input: File): List<File> = when {
            input.isDirectory -> input.walkTopDown().filter { it.isFile && it.extension == "cs3" }.toList()
            input.isFile -> listOf(input)
            else -> throw IllegalArgumentException("no such file: $input")
        }

        private fun readIdentities(file: File): Map<String, PluginIdentity> =
            jacksonObjectMapper().readValue(file)
    }
}

/**
 * Scans [Options.files] on a fixed pool. Each task makes its own [Classifier] and [PluginScan], and
 * results are gathered once at the end, so the tasks share no mutable state.
 */
fun scanAll(options: Options): List<PluginReport> {
    val pool = Executors.newFixedThreadPool(options.workers)
    try {
        val loader = Classifier::class.java.classLoader
        val tasks = options.files.map { file ->
            Callable {
                val identity = options.identities[file.name] ?: PluginIdentity(file.nameWithoutExtension)
                PluginScan(Classifier(loader)).scan(file, identity)
            }
        }
        return pool.invokeAll(tasks).map { it.get() }
    } finally {
        pool.shutdown()
        pool.awaitTermination(1, TimeUnit.MINUTES)
    }
}
