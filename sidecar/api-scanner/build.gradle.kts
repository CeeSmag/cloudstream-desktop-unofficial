// Development tool, not shipped: scans plugin files for Android / CloudStream API members the
// desktop runtime does not provide. Uses the runtime's own converter, resolver and verifier, so the
// report matches what the sidecar does. Run through tools/extension_recorder (`--mode scan`).
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
    systemProperty("junit.jupiter.execution.timeout.default", "60 s")
    testLogging { events("failed", "skipped") }
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile> {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_21) }
}

dependencies {
    // The whole runtime a plugin sees: the sidecar's own CloudStream classes (RepositoryManager,
    // account stubs, ...) as well as the plugin runtime, the Android stubs and the library.
    implementation(project(":cs-sidecar"))
    implementation(project(":plugin-runtime"))
    implementation(project(":android-stubs"))
    implementation(project(":library"))
    implementation(project(":common"))
    implementation("org.ow2.asm:asm:9.6")
    implementation("org.ow2.asm:asm-tree:9.6")
    implementation("com.fasterxml.jackson.module:jackson-module-kotlin:2.18.3")
    testImplementation(kotlin("test"))
}

application {
    mainClass.set("apiscanner.ApiScannerMainKt")
}
