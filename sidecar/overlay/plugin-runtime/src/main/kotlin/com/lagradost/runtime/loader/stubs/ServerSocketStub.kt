package com.lagradost.runtime.loader.stubs

import java.net.InetAddress
import java.net.ServerSocket

/**
 * Replaces [java.net.ServerSocket] in plugin bytecode.
 *
 * Always binds to loopback with an ephemeral or requested port on 127.0.0.1 only —
 * never 0.0.0.0 / LAN. Enough for FlixProxy-style local header proxies; not enough
 * for a malicious LAN listener.
 */
class ServerSocketStub {
    private val server: ServerSocket

    constructor(port: Int) {
        server = ServerSocket(port, 50, InetAddress.getByName("127.0.0.1"))
    }

    constructor() : this(0)

    constructor(port: Int, backlog: Int) {
        server = ServerSocket(port, backlog, InetAddress.getByName("127.0.0.1"))
    }

    constructor(port: Int, backlog: Int, bindAddr: InetAddress?) {
        val loopback = InetAddress.getByName("127.0.0.1")
        if (bindAddr != null && !bindAddr.isLoopbackAddress) {
            throw SecurityException(
                "Plugin Security: Server sockets may only bind to loopback (127.0.0.1), not '${bindAddr.hostAddress}'.",
            )
        }
        server = ServerSocket(port, backlog, loopback)
    }

    fun getLocalPort(): Int = server.localPort

    fun accept(): SocketStub = SocketStub.wrapAccepted(server.accept())

    fun close() {
        server.close()
    }

    fun isClosed(): Boolean = server.isClosed
}
