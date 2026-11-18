package com.lagradost.runtime.loader

import org.objectweb.asm.ClassReader
import org.objectweb.asm.ClassWriter
import org.objectweb.asm.commons.ClassRemapper
import org.objectweb.asm.commons.Remapper

/**
 * Rewrites dangerous JDK types in plugin bytecode to sandboxed stubs:
 * - [java.net.ServerSocket] → loopback-only [com.lagradost.runtime.loader.stubs.ServerSocketStub]
 * - [java.net.Socket] → [com.lagradost.runtime.loader.stubs.SocketStub] (outbound connect refused)
 * - [java.net.DatagramSocket] → throwing stub
 * - [java.lang.Thread] → capped daemon [com.lagradost.runtime.loader.stubs.ThreadStub]
 *
 * Runs before static security verification so StreamPlay/FlixProxy can load without
 * granting unrestricted NETWORK_SOCKETS or Trust.
 */
object RawSocketRemapper {
    private val MAP = mapOf(
        "java/net/ServerSocket" to "com/lagradost/runtime/loader/stubs/ServerSocketStub",
        "java/net/Socket" to "com/lagradost/runtime/loader/stubs/SocketStub",
        "java/net/DatagramSocket" to "com/lagradost/runtime/loader/stubs/DatagramSocketStub",
        "java/lang/Thread" to "com/lagradost/runtime/loader/stubs/ThreadStub",
    )

    private val remapper = object : Remapper() {
        override fun map(internalName: String): String = MAP[internalName] ?: internalName
    }

    fun remap(bytes: ByteArray): ByteArray {
        val reader = ClassReader(bytes)
        val writer = ClassWriter(reader, 0)
        reader.accept(ClassRemapper(writer, remapper), 0)
        return writer.toByteArray()
    }
}
