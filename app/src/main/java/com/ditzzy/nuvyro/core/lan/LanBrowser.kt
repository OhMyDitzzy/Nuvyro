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
