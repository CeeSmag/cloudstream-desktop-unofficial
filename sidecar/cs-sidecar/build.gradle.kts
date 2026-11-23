plugins {
    kotlin("jvm")
    application
}

configurations.all {
    resolutionStrategy.force("com.fasterxml.jackson.module:jackson-module-kotlin:2.18.3")
    resolutionStrategy.force("com.fasterxml.jackson.core:jackson-databind:2.18.3")
    resolutionStrategy.force("com.fasterxml.jackson.core:jackson-core:2.18.3")
    resolutionStrategy.force("com.fasterxml.jackson.core:jackson-annotations:2.18.3")
}

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

tasks.test {
    useJUnitPlatform()
    // The race tests run many threads for a few seconds; a hang must fail the build, not stall it.
    systemProperty("junit.jupiter.execution.timeout.default", "60 s")
    testLogging { events("failed", "skipped", "passed") }
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile> {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_21) }
}

// CloudStream classes that plugins link against but the library module does not contain. The
// pinned desktop client already implements these three (app context, key/value store, UiText), so
// they are compiled from its checkout at build time rather than copied into this repository.
// The account/sync and Cloudflare classes are written in this module (src/main/kotlin).
val vendorStubSources = listOf(
    "com/lagradost/cloudstream3/CloudStreamApp.kt",
    "com/lagradost/cloudstream3/utils/DataStore.kt",
)

val copyVendorStubs by tasks.registering(Copy::class) {
    from("../desktop-app/src/main/kotlin") { include(vendorStubSources) }
    into(layout.buildDirectory.dir("generated/vendor-stubs/kotlin"))
}

kotlin.sourceSets.named("main") {
    kotlin.srcDir(layout.buildDirectory.dir("generated/vendor-stubs/kotlin"))
}

tasks.named("compileKotlin") { dependsOn(copyVendorStubs) }

val pitestRuntime: Configuration by configurations.creating

dependencies {
    testImplementation(kotlin("test"))
    implementation(project(":library"))
    implementation(project(":plugin-runtime"))
    implementation(project(":android-stubs"))
    implementation(project(":common"))
    implementation(libs.nicehttp)
    implementation("com.google.code.gson:gson:2.11.0")
    implementation("com.fasterxml.jackson.module:jackson-module-kotlin:2.18.3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-swing:1.10.2")

    pitestRuntime("org.pitest:pitest-command-line:1.30.2")
    pitestRuntime("org.pitest:pitest-junit5-plugin:1.2.3")
}

application {
    mainClass.set("sidecar.SidecarMainKt")
}

tasks.register<JavaExec>("pitest") {
    group = "verification"
    description = "Run PITest mutation testing on cs-sidecar and CloudflareSolver/BrowserFetch."
    dependsOn("testClasses", ":plugin-runtime:classes")
    mainClass.set("org.pitest.mutationtest.commandline.MutationCoverageReport")

    val sidecarMain = sourceSets["main"].output.classesDirs
    val runtimeProject = project(":plugin-runtime")
    val runtimeMain = runtimeProject.extensions.getByType<SourceSetContainer>()["main"].output.classesDirs
    val defaultTargets = listOf(
        "sidecar.*",
        "com.lagradost.cloudstream3.*",
        "com.lagradost.runtime.loader.stubs.CloudflareSolver*",
        "com.lagradost.runtime.loader.stubs.BrowserFetch*",
    ).joinToString(",")
    val targets = (findProperty("pitestTargets") as String?)?.takeIf { it.isNotBlank() } ?: defaultTargets
    val threads = Runtime.getRuntime().availableProcessors().coerceIn(1, 6).toString()

    classpath = pitestRuntime + sidecarMain + runtimeMain + sourceSets["test"].runtimeClasspath
    doFirst {
        val mutablePaths = (sidecarMain.files + runtimeMain.files)
            .filter { it.exists() }
            .joinToString(",") { it.absolutePath }
        args(
            "--reportDir", layout.buildDirectory.dir("reports/pitest").get().asFile.absolutePath,
            "--targetClasses", targets,
            "--targetTests", "sidecar.*,stubs.*",
            "--sourceDirs", listOf(
                "${projectDir}/src/main/kotlin",
                "${runtimeProject.projectDir}/src/main/kotlin",
            ).joinToString(","),
            "--mutableCodePaths", mutablePaths,
            "--avoidCallsTo", "kotlin.jvm.internal.Intrinsics,kotlin.io.CloseableKt,com.lagradost.common.logging.AppLogger,java.util.logging,org.slf4j",
            "--threads", threads,
            "--timeoutConst", "6000",
            "--outputFormats", "HTML,XML",
            "--timestampedReports", "false",
            "--failWhenNoMutations", "false",
        )
    }
}
