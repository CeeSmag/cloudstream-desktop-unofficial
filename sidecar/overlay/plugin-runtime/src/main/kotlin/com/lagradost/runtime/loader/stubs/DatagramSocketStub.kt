package com.lagradost.runtime.loader.stubs

/**
 * Replaces [java.net.DatagramSocket] in plugin bytecode. UDP is not offered to plugins.
 */
class DatagramSocketStub {
    constructor() {
        refuse()
    }

    constructor(port: Int) {
        refuse()
    }

    constructor(port: Int, bindAddr: java.net.InetAddress?) {
        refuse()
    }

    constructor(bindAddr: java.net.SocketAddress?) {
        refuse()
    }

    private fun refuse(): Nothing =
        throw SecurityException("Plugin Security: Datagram / UDP sockets are blocked.")
}
