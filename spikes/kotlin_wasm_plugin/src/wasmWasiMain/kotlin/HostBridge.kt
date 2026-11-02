@file:OptIn(UnsafeWasmMemoryApi::class, ExperimentalWasmInterop::class)

import kotlin.wasm.WasmImport
import kotlin.wasm.unsafe.UnsafeWasmMemoryApi
import kotlin.wasm.unsafe.withScopedMemoryAllocator

@WasmImport("host", "host_read_input")
external fun hostReadInput(destination: Int, capacity: Int): Int

@WasmImport("host", "host_write_output")
external fun hostWriteOutput(source: Int, length: Int)

@WasmImport("host", "host_http_get_document")
external fun hostHttpGetDocument(urlPtr: Int, urlLen: Int): Int

@WasmImport("host", "host_select")
external fun hostSelect(handle: Int, selectorPtr: Int, selectorLen: Int): Int

@WasmImport("host", "host_list_len")
external fun hostListLen(listHandle: Int): Int

@WasmImport("host", "host_list_item")
external fun hostListItem(listHandle: Int, index: Int): Int

@WasmImport("host", "host_text")
external fun hostText(handle: Int, destination: Int, capacity: Int): Int

@WasmImport("host", "host_attr")
external fun hostAttr(handle: Int, namePtr: Int, nameLen: Int, destination: Int, capacity: Int): Int

/** Copies [text] into plugin memory for the duration of [block], passing (pointer, length). */
internal inline fun <T> withUtf8(text: String, block: (Int, Int) -> T): T {
    val bytes = text.encodeToByteArray()
    return withScopedMemoryAllocator { allocator ->
        val start = allocator.allocate(maxOf(bytes.size, 1))
        bytes.forEachIndexed { index, byte -> (start + index).storeByte(byte) }
        block(start.address.toInt(), bytes.size)
    }
}

/** Lets the host fill a scratch buffer; a negative count means "no value". */
internal fun readUtf8(capacity: Int, fill: (Int, Int) -> Int): String? =
    withScopedMemoryAllocator { allocator ->
        val start = allocator.allocate(capacity)
        val count = fill(start.address.toInt(), capacity)
        if (count < 0) null else ByteArray(count) { (start + it).loadByte() }.decodeToString()
    }
