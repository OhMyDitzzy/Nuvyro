package com.ditzzy.nuvyro.core.transfer.receiver

import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.LinkAddress
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withTimeoutOrNull

/** Each collection owns its callback. An old callback can only invalidate its own collection. */
internal class AndroidReceiverRouteMonitor(
    connectivity: ConnectivityManager?,
    hasPermission: () -> Boolean
) {
    private val connectivity = connectivity
    private val hasPermission = hasPermission

    fun wifi(network: Network): Flow<ReceiverRouteSnapshot?> = watch(network) { reader -> reader.wifi(network) }

    fun hotspot(ownsReservation: () -> Boolean): Flow<ReceiverRouteSnapshot?> =
        watch(null) { reader -> reader.hotspot(ownsReservation) }

    private fun watch(selected: Network?, read: (AndroidReceiverRoutes) -> ReceiverRouteSnapshot?) = flow {
        val manager = connectivity
        if (manager == null || !hasPermission()) { emit(null); return@flow }
        val reader = AndroidReceiverRoutes(manager)
        val wake = Channel<Unit>(Channel.CONFLATED)
        val invalid = AtomicBoolean(false)
        val closed = AtomicBoolean(false)
        val observed = AtomicReference<Map<Network, Pair<String?, Set<LinkAddress>>>?>(null)
        val callback = ReceiverNetworkCallback(selected, wake, invalid, closed, observed)
        var attempted = false
        try {
            // Observe every visible client/uplink for hotspot exclusions, including cellular and VPN.
            val builder = NetworkRequest.Builder()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) builder.clearCapabilities()
            else builder.removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_RESTRICTED)
                .removeCapability(NetworkCapabilities.NET_CAPABILITY_TRUSTED)
                .removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            val request = builder.build()
            attempted = true
            manager.registerNetworkCallback(request, callback)
            @Suppress("DEPRECATION")
            val networks = if (selected != null) arrayOf(selected) else manager.allNetworks
            if (networks.size > ReceiverRouteSnapshot.MaxClients) { emit(null); return@flow }
            val baseline = mutableMapOf<Network, Pair<String?, Set<LinkAddress>>>()
            for (network in networks) {
                val properties = manager.getLinkProperties(network)
                if (properties == null) { emit(null); return@flow }
                baseline[network] = properties.interfaceName to properties.linkAddresses.toSet()
            }
            observed.set(baseline.toMap())
            var initial: ReceiverRouteSnapshot? = null
            while (currentCoroutineContext().isActive) {
                val snapshot = if (hasPermission() && !invalid.get()) read(reader).takeUnless { invalid.get() } else null
                if (snapshot == null) { emit(null); break }
                if (initial == null) { initial = snapshot; emit(snapshot) }
                else if (!initial.sameRoutingAs(snapshot)) { emit(snapshot); break }
                withTimeoutOrNull(500) { wake.receive() }
            }
        } catch (_: SecurityException) {
            emit(null)
        } finally {
            closed.set(true)
            wake.close()
            if (attempted) runCatching { manager.unregisterNetworkCallback(callback) }
        }
    }.flowOn(Dispatchers.IO)
}

private class ReceiverNetworkCallback(
    selected: Network?, wake: SendChannel<Unit>, invalid: AtomicBoolean, closed: AtomicBoolean,
    observed: AtomicReference<Map<Network, Pair<String?, Set<LinkAddress>>>?>
) : ConnectivityManager.NetworkCallback() {
    private val selected = selected
    private val wake = wake
    private val invalid = invalid
    private val closed = closed
    private val observed = observed

    private fun relevant(network: Network) = selected == null || network == selected
    override fun onLost(network: Network) {
        if (!closed.get()) {
            if (relevant(network)) invalid.set(true)
            wake.trySend(Unit)
        }
    }
    override fun onAvailable(network: Network) {
        if (!closed.get()) {
            if (relevant(network) && observed.get()?.containsKey(network) == false) invalid.set(true)
            wake.trySend(Unit)
        }
    }
    override fun onLinkPropertiesChanged(network: Network, properties: LinkProperties) {
        if (!closed.get()) {
            val baseline = observed.get()
            if (baseline != null && relevant(network) &&
                baseline[network] != (properties.interfaceName to properties.linkAddresses.toSet())) invalid.set(true)
            wake.trySend(Unit)
        }
    }
    override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
        if (!closed.get()) {
            if (network == selected && (!capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) ||
                    !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN))) invalid.set(true)
            wake.trySend(Unit)
        }
    }
}
