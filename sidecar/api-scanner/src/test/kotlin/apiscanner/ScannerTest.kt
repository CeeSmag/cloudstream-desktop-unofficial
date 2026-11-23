package apiscanner

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import org.objectweb.asm.ClassWriter
import org.objectweb.asm.Handle
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import org.objectweb.asm.Type
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Builds plugin-like JVM classes (as the DEX converter outputs them) and packs them in a JAR. */
private class FakePlugin(private val dir: File) {
    private val classes = LinkedHashMap<String, ByteArray>()

    fun add(name: String, superName: String = "java/lang/Object", body: ClassWriter.() -> Unit): FakePlugin {
        val writer = ClassWriter(ClassWriter.COMPUTE_MAXS)
        writer.visit(Opcodes.V11, Opcodes.ACC_PUBLIC, name, null, superName, null)
        writer.body()
        writer.visitEnd()
        classes[name] = writer.toByteArray()
        return this
    }

    fun jar(): File {
        val file = File(dir, "plugin-${classes.keys.hashCode()}.jar")
        ZipOutputStream(file.outputStream()).use { zip ->
            for ((name, bytes) in classes) {
                zip.putNextEntry(ZipEntry("$name.class"))
                zip.write(bytes)
                zip.closeEntry()
            }
        }
        return file
    }
}

private fun ClassWriter.method(name: String, descriptor: String, body: MethodVisitor.() -> Unit) {
    val mv = visitMethod(Opcodes.ACC_PUBLIC, name, descriptor, null, null)
    mv.visitCode()
    mv.body()
    mv.visitInsn(Opcodes.RETURN)
    mv.visitMaxs(0, 0)
    mv.visitEnd()
}

private const val RESOURCES = "android/content/res/Resources"
private val GET_ASSETS = Reference(RefKind.METHOD, RESOURCES, "getAssets", "()Landroid/content/res/AssetManager;")

class ReferenceCollectorTest {

    @TempDir
    lateinit var dir: File

    @Test
    fun `references to the runtime's classes are collected and counted`() {
        val jar = FakePlugin(dir).add("p/Provider") {
            method("run", "()V") {
                repeat(2) {
                    visitInsn(Opcodes.ACONST_NULL)
                    visitMethodInsn(Opcodes.INVOKEVIRTUAL, RESOURCES, "getAssets", "()Landroid/content/res/AssetManager;", false)
                    visitInsn(Opcodes.POP)
                }
                visitFieldInsn(Opcodes.GETSTATIC, "android/content/res/AssetManager", "ACCESS_STREAMING", "I")
                visitInsn(Opcodes.POP)
                visitLdcInsn(Type.getObjectType("android/net/wifi/WifiManager"))
                visitInsn(Opcodes.POP)
                visitLdcInsn(Handle(Opcodes.H_INVOKEVIRTUAL, "android/content/Context", "getResources", "()L$RESOURCES;", false))
                visitInsn(Opcodes.POP)
                visitInsn(Opcodes.ICONST_1)
                visitTypeInsn(Opcodes.ANEWARRAY, "android/view/View")
                visitInsn(Opcodes.POP)
                // Not scanned: the JDK and the plugin's own code.
                visitLdcInsn("x")
                visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/String", "length", "()I", false)
                visitInsn(Opcodes.POP)
            }
        }.jar()
        val found = ReferenceCollector.collect(jar)
        assertEquals(2, found[GET_ASSETS])
        assertEquals(1, found[Reference(RefKind.FIELD, "android/content/res/AssetManager", "ACCESS_STREAMING", "I")])
        assertEquals(1, found[Reference(RefKind.CLASS, "android/net/wifi/WifiManager", "", "")])
        assertEquals(1, found[Reference(RefKind.METHOD, "android/content/Context", "getResources", "()L$RESOURCES;")])
        assertEquals(1, found[Reference(RefKind.CLASS, "android/view/View", "", "")])
        assertTrue(found.keys.none { it.owner.startsWith("java/") }, "JDK not scanned: ${found.keys}")
    }

    @Test
    fun `members inherited from an Android class are reported on that class, own members are not`() {
        val jar = FakePlugin(dir)
            .add("p/Base", superName = "android/app/Activity") {
                method("own", "()V") {}
            }
            .add("p/Screen", superName = "p/Base") {
                method("run", "()V") {
                    visitVarInsn(Opcodes.ALOAD, 0)
                    visitInsn(Opcodes.ICONST_1)
                    visitMethodInsn(Opcodes.INVOKEVIRTUAL, "p/Screen", "getString", "(I)Ljava/lang/String;", false)
                    visitInsn(Opcodes.POP)
                    visitVarInsn(Opcodes.ALOAD, 0)
                    visitMethodInsn(Opcodes.INVOKEVIRTUAL, "p/Screen", "own", "()V", false)
                }
            }.jar()
        val found = ReferenceCollector.collect(jar).keys
        assertTrue(Reference(RefKind.METHOD, "android/app/Activity", "getString", "(I)Ljava/lang/String;") in found, "$found")
        assertTrue(found.none { it.name == "own" }, "declared by the plugin itself")
        assertTrue(found.none { it.owner.startsWith("p/") }, "own classes are not gaps")
    }
}

class ClassifierTest {

    private val classifier = Classifier(ClassifierTest::class.java.classLoader)

    private fun status(kind: RefKind, owner: String, name: String = "", desc: String = "") =
        classifier.classify(Reference(kind, owner, name, desc))

    @Test
    fun `every status is told apart`() {
        assertEquals(Status.RESOLVED, classifier.classify(GET_ASSETS))
        assertEquals(Status.MISSING_MEMBER, status(RefKind.METHOD, RESOURCES, "getNothing", "()V"))
        assertEquals(Status.MISSING_MEMBER, status(RefKind.METHOD, RESOURCES, "getAssets", "()Ljava/lang/Object;"), "return type counts")
        assertEquals(Status.GHOST_CLASS, status(RefKind.METHOD, "android/net/wifi/WifiNotAStub", "scan", "()V"))
        assertEquals(Status.GHOST_CLASS, status(RefKind.CLASS, "android/net/wifi/WifiNotAStub"))
        assertEquals(Status.RESOLVED, status(RefKind.CLASS, RESOURCES))
        assertEquals(Status.NEUTRALISED, status(RefKind.METHOD, "android/widget/NotAWidget", "show", "()V"))
        assertEquals(Status.RESOLVED, status(RefKind.FIELD, "android/content/res/AssetManager", "ACCESS_STREAMING", "I"))
        assertEquals(Status.MISSING_MEMBER, status(RefKind.FIELD, "android/content/res/AssetManager", "ACCESS_NOTHING", "I"))
    }

    @Test
    fun `the sidecar's own CloudStream classes count as provided`() {
        // cs-sidecar implements these for MegaProvider; without it on the classpath they looked missing.
        assertEquals(Status.RESOLVED, status(RefKind.CLASS, "com/lagradost/cloudstream3/plugins/RepositoryManager"))
        assertEquals(Status.RESOLVED, status(RefKind.CLASS, "com/lagradost/cloudstream3/ui/settings/extensions/RepositoryData"))
    }

    @Test
    fun `a stub whose members cannot be listed is reported as broken`() {
        // A class with a method returning a type nobody has: reflection cannot list its methods.
        val writer = ClassWriter(0)
        writer.visit(Opcodes.V11, Opcodes.ACC_PUBLIC, "brk/Broken", null, "java/lang/Object", null)
        writer.visitMethod(Opcodes.ACC_PUBLIC or Opcodes.ACC_ABSTRACT, "x", "()Lmissing/Type;", null, null).visitEnd()
        writer.visitEnd()
        val bytes = writer.toByteArray()
        val loader = object : ClassLoader(ClassifierTest::class.java.classLoader) {
            override fun findClass(name: String): Class<*> =
                if (name == "brk.Broken") defineClass(name, bytes, 0, bytes.size) else throw ClassNotFoundException(name)
        }
        val broken = Classifier(loader).classify(Reference(RefKind.METHOD, "brk/Broken", "y", "()V"))
        assertEquals(Status.BROKEN_STUB, broken)
    }
}

class ReportTest {

    private fun report(name: String, vararg findings: Finding) = PluginReport(
        file = "$name.cs3",
        identity = PluginIdentity(name),
        findings = findings.toList(),
        resolvedCount = 3,
        sandboxRefusals = emptyList(),
        permissionNeeded = null,
        conversionErrors = emptyList(),
    )

    private val missing = Finding(Reference(RefKind.METHOD, RESOURCES, "getFoo", "()V"), Status.MISSING_MEMBER, 1)
    private val ghost = Finding(Reference(RefKind.CLASS, "android/net/wifi/WifiManager", "", ""), Status.GHOST_CLASS, 4)

    @Test
    fun `gaps are ranked by how many plugins need them`() {
        val scan = ScanReport.of(listOf(report("B", missing, ghost), report("A", missing), report("C")))
        assertEquals(ScanReport.VERSION, scan.version)
        assertEquals(3, scan.scannedPlugins)
        val ranked = scan.gaps.map { it.reference to it.pluginCount }
        assertEquals(listOf("android.content.res.Resources.getFoo()V" to 2, "android.net.wifi.WifiManager" to 1), ranked)
        assertEquals(listOf("A", "B"), scan.gaps.first().plugins, "sorted, each plugin once")
    }

    @Test
    fun `the summary lists each status and the plugins that cannot load`() {
        val refused = report("Bad").copy(sandboxRefusals = listOf("Disallowed class 'java.lang.ProcessBuilder'"))
        val failed = report("Junk").copy(error = "ZipException: not a zip")
        val text = ReportWriter.markdown(ScanReport.of(listOf(report("A", missing), refused, failed)))
        assertTrue("## Missing members (fail at run time) (1)" in text, text)
        assertTrue("| 1 | `android.content.res.Resources.getFoo()V` | A |" in text, text)
        assertTrue("## Missing classes (ghost stubs) (0)" in text, text)
        assertTrue("- **Bad**: Disallowed class 'java.lang.ProcessBuilder'" in text, text)
        assertTrue("- **Junk**: ZipException: not a zip" in text, text)
    }

    @Test
    fun `the JSON report is written next to the summary`(@TempDir dir: File) {
        ReportWriter.write(ScanReport.of(listOf(report("A", missing))), dir)
        val json = File(dir, "api_gaps.json").readText()
        assertTrue("\"version\" : 1" in json, json)
        assertTrue("getFoo" in json)
        assertTrue(File(dir, "api_gaps.md").isFile)
    }
}

class PluginScanTest {

    @TempDir
    lateinit var dir: File

    private val scan = PluginScan(Classifier(PluginScanTest::class.java.classLoader))

    @Test
    fun `a file that is not a plugin is reported, not thrown`() {
        val junk = File(dir, "Junk.cs3").apply { writeText("not a plugin") }
        val report = scan.scan(junk, PluginIdentity("Junk"))
        assertNotNull(report.error)
        assertTrue(report.findings.isEmpty())
    }

    /** Real plugins, when at hand: `CS_TEST_ECLIPSIA_CS3=<Eclipsia.cs3>`. */
    @Test
    fun `eclipsia scans clean of the assets gap it had`() {
        val file = System.getenv("CS_TEST_ECLIPSIA_CS3")?.let(::File)?.takeIf { it.isFile } ?: return
        val report = scan.scan(file, PluginIdentity("Eclipsia"))
        assertNull(report.error, report.error)
        assertTrue(report.resolvedCount > 50, "it uses the runtime a lot: ${report.resolvedCount}")
        assertTrue(report.findings.none { it.reference.name == "getAssets" }, "getAssets resolves now")
        assertTrue(report.sandboxRefusals.isEmpty(), "loads untrusted: ${report.sandboxRefusals}")
    }
}

class OptionsTest {

    @Test
    fun `arguments are read and checked`(@TempDir dir: File) {
        val a = File(dir, "a.cs3").apply { writeText("x") }
        File(dir, "nested").mkdirs()
        val b = File(dir, "nested/b.cs3").apply { writeText("x") }
        File(dir, "notes.txt").writeText("ignored")
        val ids = File(dir, "ids.json").apply { writeText("""{"a.cs3":{"internalName":"A","repositoryUrl":"r"}}""") }
        val options = Options.parse(listOf("--out", "o", "--workers", "3", "--identities", ids.path, dir.path))
        assertEquals(listOf(a, b), options.files)
        assertEquals(3, options.workers)
        assertEquals(PluginIdentity("A", "r"), options.identities["a.cs3"])
        for (bad in listOf(listOf(dir.path), listOf("--out", "o"), listOf("--out", "o", "--workers", "0", a.path))) {
            kotlin.test.assertFailsWith<IllegalArgumentException> { Options.parse(bad) }
        }
    }
}
