package com.ditzzy.nuvyro.core.lan

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.LinkProperties
import java.io.IOException
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.distinctUntilChanged

class LanNetworkMonitor(private val connectivity: ConnectivityManager?) {
    fun networks() = callbackFlow<Network?> {
        val connectivity = connectivity ?: throw IOException("Network monitoring is unavailable")
        val available = linkedSetOf<Network>()
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onLinkPropertiesChanged(network: Network, properties: LinkProperties) {
                if (properties.linkAddresses.isEmpty()) return
                available += network
                trySend(available.firstOrNull())
            }

            override fun onLost(network: Network) {
                available -= network
                trySend(available.firstOrNull())
            }
        }
        trySend(null)
        connectivity.registerNetworkCallback(
            NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build(),
            callback
        )
        awaitClose { runCatching { connectivity.unregisterNetworkCallback(callback) } }
    }.distinctUntilChanged()
}
