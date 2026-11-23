package apiscanner

import com.lagradost.runtime.dex.DexConverter
import com.lagradost.runtime.loader.PluginBytecodeTransformer
import com.lagradost.runtime.security.PluginSecurityVerifier
import java.io.File
import java.nio.file.Files

/** Who a scanned file is, when the caller knows (from the repository listing). */
data class PluginIdentity(val internalName: String, val repositoryUrl: String = "")

/** One classified reference of one plugin. */
data class Finding(val reference: Reference, val status: Status, val occurrences: Int)

/** Everything found in one plugin file. */
data class PluginReport(
    val file: String,
    val identity: PluginIdentity,
    /** References to the runtime's classes, by status (RESOLVED ones are counted, not listed). */
    val findings: List<Finding>,
    val resolvedCount: Int,
    /** What the sandbox's verifier refuses (it would not load untrusted). */
    val sandboxRefusals: List<String>,
    /** A permission the verifier would ask for, if any. */
    val permissionNeeded: String?,
    /** Problems of the DEX conversion itself; empty when it went well. */
    val conversionErrors: List<String>,
    /** Set when the file could not be scanned at all. */
    val error: String? = null,
)

/**
 * Scans one `.cs3` the way the runtime prepares it: DEX to JVM with [DexConverter], references read
 * from that JAR (before the transformer, so neutralised calls are still visible), then the
 * transformer and the verifier as for an untrusted install. Works in a temporary folder.
 */
class PluginScan(private val classifier: Classifier) {

    fun scan(cs3: File, identity: PluginIdentity): PluginReport {
        val work = Files.createTempDirectory("api-scan").toFile()
        return try {
            scanIn(work, cs3, identity)
        } catch (error: Exception) {
            failed(cs3, identity, "${error::class.java.simpleName}: ${error.message}")
        } catch (error: LinkageError) {
            failed(cs3, identity, "${error::class.java.simpleName}: ${error.message}")
        } finally {
            work.deleteRecursively()
        }
    }

    private fun scanIn(work: File, cs3: File, identity: PluginIdentity): PluginReport {
        val converted = File(work, "plugin-jvm.jar")
        val conversion = DexConverter.convert(cs3, converted)
        val counts = ReferenceCollector.collect(converted)
        val findings = counts.map { (ref, n) -> Finding(ref, classifier.classify(ref), n) }
        val transformed = File(work, "plugin-verified.jar")
        converted.copyTo(transformed)
        PluginBytecodeTransformer.transform(transformed)
        val verdict = PluginSecurityVerifier.collectFindings(transformed, identity.internalName)
        return PluginReport(
            file = cs3.name,
            identity = identity,
            findings = findings.filter { it.status != Status.RESOLVED }.sortedBy { it.reference.display() },
            resolvedCount = findings.count { it.status == Status.RESOLVED },
            sandboxRefusals = verdict.violations,
            permissionNeeded = verdict.permissionNeeded?.permissionName,
            conversionErrors = conversion.errors.toList() + conversion.failedMethods.map { "untranslated method: $it" },
        )
    }

    private fun failed(cs3: File, identity: PluginIdentity, message: String) = PluginReport(
        file = cs3.name,
        identity = identity,
        findings = emptyList(),
        resolvedCount = 0,
        sandboxRefusals = emptyList(),
        permissionNeeded = null,
        conversionErrors = emptyList(),
        error = message,
    )
}
