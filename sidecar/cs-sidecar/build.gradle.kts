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
    "com/lagradost/cloudstream3/utils/UiText.kt",
)

val copyVendorStubs by tasks.registering(Copy::class) {
    from("../desktop-app/src/main/kotlin") { include(vendorStubSources) }
    into(layout.buildDirectory.dir("generated/vendor-stubs/kotlin"))
}

kotlin.sourceSets.named("main") {
    kotlin.srcDir(layout.buildDirectory.dir("generated/vendor-stubs/kotlin"))
}

tasks.named("compileKotlin") { dependsOn(copyVendorStubs) }

dependencies {
    testImplementation(kotlin("test"))
    implementation(project(":library"))
    implementation(project(":plugin-runtime"))
    implementation(project(":android-stubs"))
    implementation(project(":common"))
    implementation(libs.nicehttp)
    implementation("com.fasterxml.jackson.module:jackson-module-kotlin:2.18.3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
}

application {
    mainClass.set("sidecar.SidecarMainKt")
}
