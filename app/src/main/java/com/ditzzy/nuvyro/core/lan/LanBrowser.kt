package com.ditzzy.nuvyro.core.lan

import android.net.ConnectivityManager
import android.net.Network
import android.net.nsd.NsdManager
import com.ditzzy.nuvyro.core.lan.profile.ProfileClient
import com.ditzzy.nuvyro.data.lan.NearbyReceiver
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit

sealed interface LanBrowserEvent {
    data object Started : LanBrowserEvent
    data class Updated(val receiver: NearbyReceiver) : LanBrowserEvent
    data class Lost(val id: String) : LanBrowserEvent
}

class LanBrowser(private val connectivity: ConnectivityManager?, manager: NsdManager?) {
    private val nsd = LanNsd(manager)
    private val client = ProfileClient()
    private val resolutions = Mutex()

    fun browse(network: Network) = channelFlow<LanBrowserEvent> {
        launch { browseHotspot(network).collect { send(it) } }
        val nsdReceivers = mutableSetOf<String>()
        while (isActive) {
            try {
                browseNsd(network).collect { event ->
                    when (event) {
                        is LanBrowserEvent.Updated -> nsdReceivers += event.receiver.id
                        is LanBrowserEvent.Lost -> nsdReceivers -= event.id
                        LanBrowserEvent.Started -> Unit
                    }
                    send(event)
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: SecurityException) {
                throw error
            } catch (_: Exception) {
                // Hotspot discovery can continue while the system NSD service recovers.
            }
            nsdReceivers.forEach { send(LanBrowserEvent.Lost(it)) }
            nsdReceivers.clear()
            delay(8_000)
        }
    }

    private fun browseHotspot(network: Network) = channelFlow<LanBrowserEvent> {
        send(LanBrowserEvent.Started)
        var currentId: String? = null
        var failures = 0
        while (isActive) {
            // DHCP routes may arrive after the first link address, or change on the same Network.
            val properties = connectivity?.getLinkProperties(network)
            val links = properties?.linkAddresses.orEmpty()
            val gateways = properties?.routes.orEmpty().filter { it.isDefaultRoute }.mapNotNull { it.gateway }
                .filter { isOnLan(it, links) }.distinctBy { it.address.size }.take(2)
            var found: NearbyReceiver? = null
            for (gateway in gateways) {
                try {
                    found = client.fetchHotspot(network, gateway, links)
                    break
                } catch (error: CancellationException) {
                    throw error
                } catch (error: SecurityException) {
                    throw error
                } catch (_: Exception) {
                    // Try the other address family before treating the host as unavailable.
                }
            }
            val receiver = found
            if (receiver != null) {
                currentId?.takeIf { it != receiver.id }?.let { send(LanBrowserEvent.Lost(it)) }
                currentId = receiver.id
                failures = 0
                send(LanBrowserEvent.Updated(receiver))
            } else {
                failures++
                if (failures >= 2) {
                    currentId?.let { send(LanBrowserEvent.Lost(it)) }
                    currentId = null
                }
            }
            delay(8_000)
        }
    }

    private fun browseNsd(network: Network) = channelFlow<LanBrowserEvent> {
        val links = connectivity?.getLinkProperties(network)?.linkAddresses
            ?: throw IOException("Wi-Fi is unavailable")
        val workers = mutableMapOf<String, Job>()
        val connections = Semaphore(4)
        nsd.discover(network).collect { event ->
            when (event) {
                NsdEvent.Started -> send(LanBrowserEvent.Started)
                is NsdEvent.Lost -> {
                    workers.remove(event.name)?.cancel()
                    send(LanBrowserEvent.Lost(event.name))
                }
                is NsdEvent.Found -> {
                    val name = event.service.serviceName
                    if (name !in workers && workers.size < 32) {
                        workers[name] = launch {
                            var failures = 0
                            while (isActive) {
                                try {
                                    val receiver = connections.withPermit {
                                        val info = resolutions.withLock { nsd.resolve(event.service) }
                                            ?: throw IOException("Receiver could not be resolved")
                                        client.fetch(network, info, links)
                                    }
                                    send(LanBrowserEvent.Updated(receiver))
                                    failures = 0
                                } catch (error: CancellationException) {
                                    throw error
                                } catch (_: Exception) {
                                    failures++
                                    if (failures >= 2) send(LanBrowserEvent.Lost(name))
                                }
                                delay(8_000)
                            }
                        }
                    }
                }
            }
        }
    }
}
