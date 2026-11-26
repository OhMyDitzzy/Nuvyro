package com.ditzzy.nuvyro.core.hotspot

import android.net.ConnectivityManager
import com.ditzzy.nuvyro.core.lan.isInSubnet
import java.net.NetworkInterface
import java.net.Socket

internal class HotspotPeerAccess(private val connectivity: ConnectivityManager?) {
    @Suppress("DEPRECATION")
    fun allows(socket: Socket): Boolean {
        val manager = connectivity ?: return false
        val address = socket.localAddress
        // A local-only hotspot has no client Network. Exclude uplinks, VPNs and other known networks.
        val clientAddresses = manager.allNetworks.flatMap {
            manager.getLinkProperties(it)?.linkAddresses.orEmpty().map { link -> link.address }
        }
        if (address in clientAddresses) return false
        val networkInterface = NetworkInterface.getByInetAddress(address) ?: return false
        if (!networkInterface.isUp || networkInterface.isLoopback) return false
        return networkInterface.interfaceAddresses.any {
            isInSubnet(socket.inetAddress, it.address, it.networkPrefixLength.toInt())
        }
    }
}
