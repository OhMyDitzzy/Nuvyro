package com.ditzzy.nuvyro.core.lan

import android.net.Network
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import com.ditzzy.nuvyro.core.lan.profile.ProfileProtocol
import java.io.IOException
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.coroutines.resume

internal sealed interface NsdEvent {
    data object Started : NsdEvent
    data class Found(val service: NsdServiceInfo) : NsdEvent
    data class Lost(val name: String) : NsdEvent
}

@Suppress("DEPRECATION")
internal class LanNsd(private val manager: NsdManager?) {
    fun register(network: Network, port: Int, sessionId: String) = callbackFlow<Unit> {
        val manager = manager ?: throw IOException("Service registration is unavailable")
        val startup = launch { delay(10_000); close(IOException("Service registration timed out")) }
        val info = NsdServiceInfo().apply {
            serviceName = "Nuvyro-$sessionId"
            serviceType = ProfileProtocol.ServiceType
            setPort(port)
            setAttribute("v", ProfileProtocol.Version.toString())
            setAttribute("id", sessionId)
            if (Build.VERSION.SDK_INT >= 33) setNetwork(network)
        }
        val listener = object : NsdManager.RegistrationListener {
            override fun onServiceRegistered(info: NsdServiceInfo) {
                startup.cancel()
                if (!trySend(Unit).isSuccess) runCatching { manager.unregisterService(this) }
            }
            override fun onRegistrationFailed(info: NsdServiceInfo, error: Int) {
                close(IOException("Service registration failed: $error"))
            }
            override fun onServiceUnregistered(info: NsdServiceInfo) {
                close(IOException("Service registration stopped"))
            }
            override fun onUnregistrationFailed(info: NsdServiceInfo, error: Int) {
                close(IOException("Service unregistration failed: $error"))
            }
        }
        manager.registerService(info, NsdManager.PROTOCOL_DNS_SD, listener)
        awaitClose {
            startup.cancel()
            runCatching { manager.unregisterService(listener) }
        }
    }

    fun discover(network: Network) = callbackFlow<NsdEvent> {
        val manager = manager ?: throw IOException("Service discovery is unavailable")
        val startup = launch { delay(10_000); close(IOException("Starting discovery timed out")) }
        val listener = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(type: String) {
                startup.cancel()
                if (!trySend(NsdEvent.Started).isSuccess) runCatching { manager.stopServiceDiscovery(this) }
            }
            override fun onServiceFound(service: NsdServiceInfo) {
                if (service.serviceType.trimEnd('.') == ProfileProtocol.ServiceType.trimEnd('.')) {
                    trySend(NsdEvent.Found(service))
                }
            }
            override fun onServiceLost(service: NsdServiceInfo) {
                trySend(NsdEvent.Lost(service.serviceName))
            }
            override fun onDiscoveryStopped(type: String) { close(IOException("Discovery stopped")) }
            override fun onStartDiscoveryFailed(type: String, error: Int) {
                close(IOException("Discovery failed: $error"))
            }
            override fun onStopDiscoveryFailed(type: String, error: Int) {
                close(IOException("Stopping discovery failed: $error"))
            }
        }
        if (Build.VERSION.SDK_INT >= 33) {
            manager.discoverServices(
                ProfileProtocol.ServiceType, NsdManager.PROTOCOL_DNS_SD, network,
                java.util.concurrent.Executor { it.run() }, listener
            )
        } else {
            manager.discoverServices(ProfileProtocol.ServiceType, NsdManager.PROTOCOL_DNS_SD, listener)
        }
        awaitClose {
            startup.cancel()
            runCatching { manager.stopServiceDiscovery(listener) }
        }
    }

    suspend fun resolve(service: NsdServiceInfo): NsdServiceInfo? = withTimeoutOrNull(8_000) {
        val manager = manager ?: throw IOException("Service resolution is unavailable")
        suspendCancellableCoroutine { continuation ->
            val listener = object : NsdManager.ResolveListener {
                override fun onServiceResolved(info: NsdServiceInfo) {
                    if (continuation.isActive) continuation.resume(info)
                }
                override fun onResolveFailed(info: NsdServiceInfo, error: Int) {
                    if (continuation.isActive) continuation.resume(null)
                }
            }
            continuation.invokeOnCancellation {
                if (Build.VERSION.SDK_INT >= 34) runCatching { manager.stopServiceResolution(listener) }
            }
            try {
                manager.resolveService(service, listener)
            } catch (_: IllegalArgumentException) {
                if (continuation.isActive) continuation.resume(null)
            }
        }
    }
}
