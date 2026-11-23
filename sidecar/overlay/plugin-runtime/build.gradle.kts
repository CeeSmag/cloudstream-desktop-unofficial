plugins {
    kotlin("jvm")
}

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_21)
    }
}

configurations.all {
    resolutionStrategy.force("com.fasterxml.jackson.module:jackson-module-kotlin:2.18.3")
    resolutionStrategy.force("com.fasterxml.jackson.core:jackson-databind:2.18.3")
    resolutionStrategy.force("com.fasterxml.jackson.core:jackson-core:2.18.3")
    resolutionStrategy.force("com.fasterxml.jackson.core:jackson-annotations:2.18.3")
}

val pitestRuntime: Configuration by configurations.creating

dependencies {
    // Needs access to stubs to pass to plugins
    implementation(project(":android-stubs"))

    // Needs access to base CloudstreamPlugin and Extractors
    implementation(project(":library"))
    implementation(libs.nicehttp)
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    // Logging and common utils
    implementation(project(":common"))

    // Coroutines for plugin dispatcher & invoker isolation
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")

    // Dalvik-to-JVM transcompiler
    implementation("de.femtopedia.dex2jar:dex-tools:2.4.38")

    // JSON for manifest parsing
    implementation("com.fasterxml.jackson.module:jackson-module-kotlin:2.18.3")

    // ASM Bytecode Manipulation for Static Verification + raw-socket remapping
    implementation("org.ow2.asm:asm:9.6")
    implementation("org.ow2.asm:asm-tree:9.6")
    implementation("org.ow2.asm:asm-commons:9.6")

    // Rhino JS Engine Sandbox
    implementation(libs.rhino)

    // AXML Parser for plugin settings discovery
    implementation("net.dongliu:apk-parser:2.6.10")

    testImplementation(kotlin("test"))
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.0")

    pitestRuntime("org.pitest:pitest-command-line:1.30.2")
    pitestRuntime("org.pitest:pitest-junit5-plugin:1.2.3")
}

tasks.withType<Test> {
    useJUnitPlatform()
}

tasks.register<JavaExec>("pitest") {
    group = "verification"
    description = "Run PITest mutation testing on plugin-runtime and android-stubs overlays."
    dependsOn("testClasses", ":android-stubs:classes")
    mainClass.set("org.pitest.mutationtest.commandline.MutationCoverageReport")

    val runtimeMain = sourceSets["main"].output.classesDirs
    val stubsProject = project(":android-stubs")
    val stubsMain = stubsProject.extensions.getByType<SourceSetContainer>()["main"].output.classesDirs
    val defaultTargets = listOf(
        "com.lagradost.runtime.assets.*",
        "com.lagradost.runtime.loader.ApiResolver*",
        "com.lagradost.runtime.loader.CompatPluginClassLoader*",
        "com.lagradost.runtime.loader.GuardedCallRewriter*",
        "com.lagradost.runtime.loader.PluginBytecodeTransformer*",
        "com.lagradost.runtime.loader.RawSocketRemapper*",
        "com.lagradost.runtime.loader.SafePluginClassLoader*",
        "com.lagradost.runtime.loader.ShadowUiTransform*",
        "com.lagradost.runtime.loader.stubs.*",
        "com.lagradost.runtime.security.*",
        "com.lagradost.runtime.permission.*",
        "android.content.res.AssetManager*",
        "android.content.res.Resources*",
        "android.view.MotionEvent*",
        "android.view.View*",
        "android.widget.AbsoluteLayout*",
        "android.webkit.*",
        "androidx.fragment.app.Fragment*",
        "androidx.fragment.app.DialogFragment*",
        "com.google.android.material.bottomsheet.*",
    ).joinToString(",")
    val targets = (findProperty("pitestTargets") as String?)?.takeIf { it.isNotBlank() } ?: defaultTargets
    val excluded = (findProperty("pitestExcluded") as String?)
        ?: "com.lagradost.runtime.loader.stubs.CloudflareSolver*,com.lagradost.runtime.loader.stubs.BrowserFetch*"
    val threads = Runtime.getRuntime().availableProcessors().coerceIn(1, 6).toString()

    classpath = pitestRuntime + runtimeMain + stubsMain + sourceSets["test"].runtimeClasspath
    doFirst {
        val mutablePaths = (runtimeMain.files + stubsMain.files)
            .filter { it.exists() }
            .joinToString(",") { it.absolutePath }
        args(
            "--reportDir", layout.buildDirectory.dir("reports/pitest").get().asFile.absolutePath,
            "--targetClasses", targets,
            "--excludedClasses", excluded,
            "--targetTests", "com.lagradost.runtime.*",
            "--sourceDirs", listOf(
                "${projectDir}/src/main/kotlin",
                "${projectDir}/src/main/java",
                "${stubsProject.projectDir}/src/main/java",
                "${stubsProject.projectDir}/src/main/kotlin",
            ).joinToString(","),
            "--mutableCodePaths", mutablePaths,
            "--threads", threads,
            "--timeoutConst", "6000",
            "--outputFormats", "HTML,XML",
            "--timestampedReports", "false",
            "--failWhenNoMutations", "false",
        )
    }
}
