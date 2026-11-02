plugins {
    kotlin("multiplatform") version "2.4.0"
    kotlin("plugin.serialization") version "2.4.0"
}

kotlin {
    @OptIn(org.jetbrains.kotlin.gradle.ExperimentalWasmDsl::class)
    wasmWasi {
        nodejs()
        binaries.executable()
    }
    sourceSets {
        commonMain.dependencies {
            implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
            implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.11.0")
        }
    }
}
