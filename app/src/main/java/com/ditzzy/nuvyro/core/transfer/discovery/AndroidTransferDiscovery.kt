package com.ditzzy.nuvyro.core.transfer.discovery

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.SystemClock
import com.ditzzy.nuvyro.core.lan.LanPermission
import com.ditzzy.nuvyro.core.transfer.endpoint.AndroidTransferEndpoints
import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferEndpointCandidate
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferNetworkEpoch
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferResolution
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferTarget
import java.lang.ref.WeakReference
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** Single-use discovery owner. Collection starts routing discovery; close invalidates all hints. */
internal class AndroidTransferDiscovery(
    private val connectivity: ConnectivityManager?,
    private val manager: NsdManager?,
    private val hasPermission: () -> Boolean,
    private val clock: () -> Long = SystemClock::elapsedRealtime,
    private val budget: TransferNsdBudget = TransferNsdBudget.Shared
) : AutoCloseable {
    private val endpoints = AndroidTransferEndpoints(connectivity, clock)
    private val used = AtomicBoolean(false)
    private val closed = AtomicBoolean(false)
    private val run = AtomicReference<Job?>(null)
    private val epochRef = AtomicReference<TransferNetworkEpoch?>(null)

    fun browse(network: Network) = callbackFlow<TransferDiscoveryUpdate> {
        if (!used.compareAndSet(false, true)) {
            throw TransferDiscoveryException(TransferDiscoveryFailure.AlreadyUsed)
        }
        val ownerJob = currentCoroutineContext()[Job] ?: error("Missing discovery job")
        run.set(ownerJob)
        val queue = TransferDiscoveryQueue<NsdServiceInfo>()
        val workers = ConcurrentHashMap<TransferDiscoveryQueue.Work<NsdServiceInfo>, Job>()
        val children = mutableListOf<Job>()
        var callback: ConnectivityManager.NetworkCallback? = null
        var registrationAttempted = false
        try {
            if (closed.get()) throw TransferDiscoveryException(TransferDiscoveryFailure.Closed)
            if (!hasPermission()) throw TransferDiscoveryException(TransferDiscoveryFailure.PermissionRequired)
            val connectivity = connectivity
                ?: throw TransferDiscoveryException(TransferDiscoveryFailure.NetworkUnavailable)
            val epoch = endpoints.activate(network)
                ?: throw TransferDiscoveryException(TransferDiscoveryFailure.NetworkUnavailable)
            epochRef.set(epoch)
            val active = AtomicBoolean(false)
            val failure = AtomicReference<TransferDiscoveryFailure?>(null)
            val nsd = manager?.let { AndroidTransferNsd(it, budget) }

            fun publish() {
                trySend(TransferDiscoveryUpdate(
                    epoch, active.get(), failure.get(), queue.size(), budget.resolutionCapacityAvailable()
                ))
            }

            fun startDue() {
                if (!active.get() || closed.get() || !budget.resolutionCapacityAvailable()) return
                repeat(TransferNsdBudget.MaxResolutions) {
                    val work = queue.claim(clock()) ?: return
                    val ticket = AtomicReference<TransferResolution?>(null)
                    val worker = launch(start = CoroutineStart.LAZY) {
                        try {
                            if (!queue.isCurrent(work) || !hasPermission()) return@launch
                            val resolution = endpoints.beginResolution(epoch, work.service) ?: return@launch
                            ticket.set(resolution)
                            val result = nsd?.resolve(work.service) ?: TransferNsdResult.Failed
                            if (queue.isCurrent(work) && result is TransferNsdResult.Resolved) {
                                endpoints.resolved(resolution, result.service)
                            }
                        } catch (error: CancellationException) {
                            throw error
                        } catch (_: RuntimeException) {
                            // A failed hint does not authorize a fallback network or a profile connection.
                        }
                    }
                    workers[work] = worker
                    worker.invokeOnCompletion {
                        ticket.get()?.let(endpoints::resolutionFailed)
                        workers.remove(work)
                        queue.finished(work, clock())
                        publish()
                    }
                    worker.start()
                }
            }

            val watcher = SelectedNetworkCallback(network, WeakReference(channel), closed)
            callback = watcher
            registrationAttempted = true
            connectivity.registerNetworkCallback(
                NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build(),
                watcher
            )
            publish()
            children += launch {
                try {
                    if (nsd == null) throw TransferDiscoveryException(TransferDiscoveryFailure.NsdUnavailable)
                    nsd.discover(network).collect { event ->
                        when (event) {
                            TransferNsdEvent.Started -> active.set(true)
                            is TransferNsdEvent.Found -> {
                                endpoints.serviceKey(epoch, event.service)?.let {
                                    queue.found(it, event.service)
                                }
                            }
                            is TransferNsdEvent.Lost -> {
                                endpoints.serviceKey(epoch, event.service)?.let {
                                    queue.lost(it)?.let { work -> workers[work]?.cancel() }
                                    endpoints.serviceLost(epoch, event.service)
                                }
                            }
                        }
                        startDue()
                        publish()
                    }
                } catch (error: CancellationException) {
                    throw error
                } catch (error: TransferDiscoveryException) {
                    failure.set(error.reason)
                } catch (_: RuntimeException) {
                    failure.set(TransferDiscoveryFailure.NsdUnavailable)
                } finally {
                    active.set(false)
                    val services = queue.services()
                    queue.close()
                    workers.values.forEach { it.cancel() }
                    services.forEach { endpoints.serviceLost(epoch, it) }
                    publish()
                }
            }
            children += launch {
                try {
                    while (isActive) {
                        if (!hasPermission()) {
                            close(TransferDiscoveryException(TransferDiscoveryFailure.PermissionRequired))
                            break
                        }
                        if (endpoints.refresh(epoch) !== epoch) {
                            close(TransferDiscoveryException(TransferDiscoveryFailure.NetworkChanged))
                            break
                        }
                        startDue()
                        publish()
                        delay(TransferDiscoveryQueue.NetworkRefreshMillis)
                    }
                } catch (error: CancellationException) {
                    throw error
                } catch (_: RuntimeException) {
                    close(TransferDiscoveryException(TransferDiscoveryFailure.NetworkUnavailable))
                }
            }
            awaitClose { }
        } catch (error: CancellationException) {
            throw error
        } catch (_: SecurityException) {
            throw TransferDiscoveryException(TransferDiscoveryFailure.PermissionRequired)
        } catch (_: RuntimeException) {
            throw TransferDiscoveryException(TransferDiscoveryFailure.NetworkUnavailable)
        } finally {
            closed.set(true)
            queue.close()
            children.forEach { it.cancel() }
            workers.values.forEach { it.cancel() }
            endpoints.close()
            if (registrationAttempted) callback?.let { registered ->
                runCatching { connectivity?.unregisterNetworkCallback(registered) }
            }
            epochRef.set(null)
            run.compareAndSet(ownerJob, null)
        }
    }.buffer(Channel.CONFLATED).flowOn(Dispatchers.IO)

    fun select(sessionId: SecureSessionId): TransferTarget? {
        if (closed.get() || !hasPermission()) return null
        val epoch = epochRef.get() ?: return null
        if (endpoints.currentEpoch() !== epoch) return null
        return endpoints.select(epoch, sessionId)
    }

    fun clearSelection(target: TransferTarget): Boolean = endpoints.clearSelection(target)

    fun candidates(target: TransferTarget): List<TransferEndpointCandidate> =
        if (closed.get() || !hasPermission()) emptyList() else endpoints.candidates(target)

    fun networkFor(candidate: TransferEndpointCandidate): Network? =
        if (closed.get() || !hasPermission()) null else endpoints.networkFor(candidate)

    override fun close() {
        closed.set(true)
        endpoints.close()
        run.get()?.cancel()
    }

    private class SelectedNetworkCallback(
        private val selected: Network,
        private val sink: WeakReference<SendChannel<TransferDiscoveryUpdate>>,
        private val closed: AtomicBoolean
    ) : ConnectivityManager.NetworkCallback() {
        override fun onLost(network: Network) {
            if (network == selected) invalidate()
        }

        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            if (network == selected && (!capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) ||
                !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN))
            ) invalidate()
        }

        private fun invalidate() {
            closed.set(true)
            sink.get()?.close(TransferDiscoveryException(TransferDiscoveryFailure.NetworkUnavailable))
        }
    }

    companion object {
        fun create(context: Context): AndroidTransferDiscovery {
            val app = context.applicationContext
            return AndroidTransferDiscovery(
                app.getSystemService(ConnectivityManager::class.java),
                app.getSystemService(NsdManager::class.java),
                { LanPermission.isGranted(app) }
            )
        }
    }
}
