package com.ditzzy.nuvyro.core.lan.profile

import android.net.LinkAddress
import com.ditzzy.nuvyro.core.lan.isOnLan
import java.io.Closeable
import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketTimeoutException
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.withContext

internal class ProfileServer(
    private val links: List<LinkAddress>,
    private val packet: ProfilePacket
) : Closeable {
    private val server = ServerSocket(0, 8).apply { soTimeout = 500 }
    private val sockets = ConcurrentHashMap.newKeySet<Socket>()
    val port: Int get() = server.localPort

    suspend fun serve() = withContext(Dispatchers.IO) {
        coroutineScope {
            val clients = Semaphore(4)
            try {
                while (isActive) {
                    val socket = try { server.accept() } catch (_: SocketTimeoutException) { continue }
                    val wifiInterface = links.any { it.address == socket.localAddress }
                    if (!wifiInterface || !isOnLan(socket.inetAddress, links) || !clients.tryAcquire()) {
                        runCatching { socket.close() }
                        continue
                    }
                    sockets += socket
                    launch {
                        val deadline = launch { delay(3_000); runCatching { socket.close() } }
                        try {
                            socket.use {
                                it.soTimeout = 2_000
                                if (DataInputStream(it.getInputStream()).readInt() == ProfileProtocol.Magic) {
                                    ProfileProtocol.write(DataOutputStream(it.getOutputStream()), packet)
                                }
                            }
                        } catch (_: java.io.IOException) {
                            // A failed profile request must not stop availability for other devices.
                        } finally {
                            deadline.cancel()
                            sockets -= socket
                            runCatching { socket.close() }
                            clients.release()
                        }
                    }
                }
            } finally {
                sockets.forEach { runCatching { it.close() } }
            }
        }
    }

    override fun close() { server.close() }
}
