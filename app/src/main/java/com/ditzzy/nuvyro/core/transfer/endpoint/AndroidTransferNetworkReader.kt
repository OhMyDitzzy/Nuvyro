package com.ditzzy.nuvyro.core.transfer.endpoint

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import java.net.NetworkInterface
import java.net.SocketException

/** Reads only the explicitly selected sender Wi-Fi Network, including local-only Wi-Fi. */
internal class AndroidTransferNetworkReader(
    private val connectivity: ConnectivityManager?,
    private val interfaceIndex: (String) -> Int? = { name ->
        NetworkInterface.getByName(name)?.takeIf { it.isUp && !it.isLoopback }?.index
    }
) {
    fun read(network: Network): TransferNetworkSnapshot? = try {
        snapshot(network)
    } catch (_: SecurityException) {
        null
    } catch (_: SocketException) {
        null
    } catch (_: IllegalArgumentException) {
        null
    }

    private fun snapshot(network: Network): TransferNetworkSnapshot? {
        val manager = connectivity ?: return null
        val capabilities = manager.getNetworkCapabilities(network) ?: return null
        if (!capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) ||
            !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
        ) return null
        val properties = manager.getLinkProperties(network) ?: return null
        val name = properties.interfaceName ?: return null
        val index = interfaceIndex(name)?.takeIf { it > 0 } ?: return null
        val links = properties.linkAddresses
        if (links.size > TransferNetworkSnapshot.MaxLocalAddresses) return null
        val gateways = properties.routes.asSequence().filter {
            it.isDefaultRoute && it.getInterface() == name
        }.mapNotNull { it.gateway }.take(TransferNetworkSnapshot.MaxGateways + 1).toList()
        return TransferNetworkSnapshot.create(
            network.networkHandle,
            index,
            links.map { TransferLocalAddress(it.address, it.prefixLength) },
            gateways
        )
    }
}
