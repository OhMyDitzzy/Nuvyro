package com.ditzzy.nuvyro.core.lan.profile

import android.net.LinkAddress
import android.net.Network
import android.net.nsd.NsdServiceInfo
import android.os.Build
import com.ditzzy.nuvyro.core.lan.isOnLan
import com.ditzzy.nuvyro.data.lan.NearbyReceiver
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Suppress("DEPRECATION")
internal class ProfileClient {
    suspend fun fetch(network: Network, info: NsdServiceInfo, links: List<LinkAddress>): NearbyReceiver =
        withContext(Dispatchers.IO) {
            val version = info.attributes["v"]?.toString(Charsets.UTF_8)
            val id = info.attributes["id"]?.toString(Charsets.UTF_8)
            if (version != ProfileProtocol.Version.toString() || id?.length != 36 || info.port !in 1..65535) {
                throw IOException("Unsupported receiver")
            }
            val addresses = if (Build.VERSION.SDK_INT >= 34) info.hostAddresses else listOfNotNull(info.host)
            val host = addresses.firstOrNull { isOnLan(it, links) } ?: throw IOException("Receiver is outside this LAN")
            val packet = request(network, host, info.port)
            if (packet.sessionId != id) throw IOException("Receiver session changed")
            NearbyReceiver(info.serviceName, packet.name, DiscoveryPhoto.decode(packet.photo))
        }

    suspend fun fetchHotspot(network: Network, gateway: InetAddress, links: List<LinkAddress>): NearbyReceiver =
        withContext(Dispatchers.IO) {
            if (!isOnLan(gateway, links)) throw IOException("Hotspot is outside this LAN")
            val packet = request(network, gateway, ProfileProtocol.HotspotPort)
            NearbyReceiver("Nuvyro-${packet.sessionId}", packet.name, DiscoveryPhoto.decode(packet.photo))
        }

    private suspend fun request(network: Network, host: InetAddress, port: Int): ProfilePacket = coroutineScope {
        network.socketFactory.createSocket().use { socket ->
            val cancellationCloser = launch(start = CoroutineStart.UNDISPATCHED) {
                try { awaitCancellation() } finally { runCatching { socket.close() } }
            }
            val deadline = launch { delay(4_000); runCatching { socket.close() } }
            try {
                socket.soTimeout = 2_000
                socket.connect(InetSocketAddress(host, port), 2_000)
                DataOutputStream(socket.getOutputStream()).apply { writeInt(ProfileProtocol.Magic); flush() }
                ProfileProtocol.read(DataInputStream(socket.getInputStream()))
            } finally {
                deadline.cancel()
                cancellationCloser.cancel()
            }
        }
    }
}
