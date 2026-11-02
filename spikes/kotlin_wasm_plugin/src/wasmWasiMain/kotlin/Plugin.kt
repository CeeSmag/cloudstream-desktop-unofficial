@file:OptIn(kotlin.wasm.ExperimentalWasmInterop::class)

import kotlin.wasm.WasmExport
import kotlin.wasm.WasmImport

/** Host function provided by the Rust side. */
@WasmImport("host", "host_double")
external fun hostDouble(value: Int): Int

/** Stand-in for a provider call: crosses the boundary in both directions. */
@WasmExport("plugin_run")
fun pluginRun(input: Int): Int = hostDouble(input) + 1

fun main() {}
