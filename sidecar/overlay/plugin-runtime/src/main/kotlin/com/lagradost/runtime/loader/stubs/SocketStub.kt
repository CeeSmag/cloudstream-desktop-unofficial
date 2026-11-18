package com.lagradost.runtime.loader.stubs

import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.Proxy
import java.net.Socket
import java.net.SocketAddress
import java.net.SocketException

/**
 * Replaces [java.net.Socket] in plugin bytecode.
 *
 * Accepted connections from [ServerSocketStub] are wrapped and usable.
 * Outbound constructors throw — plugins must use NiceHttp/OkHttp (SSRF-guarded).
 */
class SocketStub {
    private var delegate: Socket? = null

    /** Outbound connect — always refused. */
    constructor() {
        refuseOutbound()
    }

    constructor(host: String?, port: Int) {
        refuseOutbound()
    }

    constructor(address: InetAddress?, port: Int) {
        refuseOutbound()
    }

    constructor(host: String?, port: Int, localAddr: InetAddress?, localPort: Int) {
        refuseOutbound()
    }

    constructor(address: InetAddress?, port: Int, localAddr: InetAddress?, localPort: Int) {
        refuseOutbound()
    }

    constructor(proxy: Proxy?) {
        refuseOutbound()
    }

    private constructor(accepted: Socket) {
        delegate = accepted
    }

    companion object {
        @JvmStatic
        fun wrapAccepted(accepted: Socket): SocketStub = SocketStub(accepted)

        private fun refuseOutbound(): Nothing =
            throw SecurityException(
                "Plugin Security: Opening outbound sockets is blocked. Use the app HTTP client.",
            )
    }

    fun setSoTimeout(timeout: Int) {
        requireDelegate().soTimeout = timeout
    }

    fun getInputStream(): InputStream = requireDelegate().getInputStream()

    fun getOutputStream(): OutputStream = requireDelegate().getOutputStream()

    fun close() {
        delegate?.close()
    }

    fun isClosed(): Boolean = delegate?.isClosed ?: true

    fun isConnected(): Boolean = delegate?.isConnected ?: false

    fun getInetAddress(): InetAddress? = delegate?.inetAddress

    fun getPort(): Int = delegate?.port ?: 0

    fun getLocalPort(): Int = delegate?.localPort ?: 0

    fun getRemoteSocketAddress(): SocketAddress? = delegate?.remoteSocketAddress

    fun shutdownInput() {
        requireDelegate().shutdownInput()
    }

    fun shutdownOutput() {
        requireDelegate().shutdownOutput()
    }

    private fun requireDelegate(): Socket =
        delegate ?: throw SocketException("Plugin Security: outbound socket was never connected")
}
