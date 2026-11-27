package com.ditzzy.nuvyro.core.transfer.endpoint

import android.net.ConnectivityManager
import android.net.Network
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.SystemClock

/**
 * Owns one selected client Network. Its discovery owner forwards scoped callbacks;
 * constructing this object starts no discovery, socket, or native engine.
 * Forward callbacks with their original epoch. A changed refresh result requires
 * cancelling old discovery/connection work and selecting the QR session again.
 * Platform snapshots are read synchronously; use the transfer owner's IO context.
 */
internal class AndroidTransferEndpoints(
    connectivity: ConnectivityManager?,
    clock: () -> Long = SystemClock::elapsedRealtime
) : AutoCloseable {
    private class Binding(val network: Network, val epoch: TransferNetworkEpoch)
    private val reader = AndroidTransferNetworkReader(connectivity)
    private val registry = TransferEndpointRegistry(clock)
    private var binding: Binding? = null

    @Synchronized
    fun activate(network: Network): TransferNetworkEpoch? {
        binding?.let { registry.networkLost(it.epoch) }
        binding = null
        val snapshot = reader.read(network) ?: return null
        val epoch = registry.activate(snapshot) ?: return null
        binding = Binding(network, epoch)
        return epoch
    }

    @Synchronized
    fun currentEpoch(): TransferNetworkEpoch? = binding?.epoch

    @Synchronized
    fun refresh(epoch: TransferNetworkEpoch): TransferNetworkEpoch? {
        val active = binding?.takeIf { it.epoch === epoch } ?: return null
        val snapshot = reader.read(active.network)
        if (snapshot == null) { networkLost(epoch); return null }
        val refreshed = registry.refresh(epoch, snapshot)
        if (refreshed == null) { networkLost(epoch); return null }
        binding = Binding(active.network, refreshed)
        return refreshed
    }

    @Synchronized
    fun select(epoch: TransferNetworkEpoch, sessionId: SecureSessionId): TransferTarget? {
        if (refresh(epoch) !== epoch) return null
        return registry.select(epoch, sessionId)
    }

    @Synchronized
    fun clearSelection(target: TransferTarget): Boolean = registry.clearSelection(target)

    @Synchronized
    fun serviceKey(epoch: TransferNetworkEpoch, service: NsdServiceInfo): String? {
        if (binding?.epoch !== epoch || !matchesNetwork(service)) return null
        val name = service.serviceName ?: return null
        val type = service.serviceType ?: return null
        return name.takeIf {
            TransferEndpointContract.acceptsName(it) && TransferEndpointContract.acceptsType(type)
        }
    }

    @Synchronized
    fun beginResolution(epoch: TransferNetworkEpoch, service: NsdServiceInfo): TransferResolution? {
        if (refresh(epoch) !== epoch) return null
        val name = serviceKey(epoch, service) ?: return null
        return registry.beginResolution(
            epoch, name, TransferEndpointContract.ServiceType
        )
    }

    @Synchronized
    @Suppress("DEPRECATION")
    fun resolved(ticket: TransferResolution, service: NsdServiceInfo): Boolean {
        if (refresh(ticket.epoch) !== ticket.epoch) return false
        if (!matchesNetwork(service)) return failed(ticket)
        val attributes = service.attributes
        val version = ascii(attributes["v"], 3) ?: return failed(ticket)
        val sessionId = ascii(attributes["sid"], 32) ?: return failed(ticket)
        val addresses = if (Build.VERSION.SDK_INT >= 34) service.hostAddresses
            else listOfNotNull(service.host)
        if (addresses.size > TransferEndpointContract.MaxResolvedAddresses) return failed(ticket)
        return registry.resolved(
            ticket,
            ResolvedTransferService(
                service.serviceName ?: return failed(ticket),
                service.serviceType ?: return failed(ticket),
                version, sessionId, service.port, addresses.toList()
            )
        )
    }

    @Synchronized
    fun resolutionFailed(ticket: TransferResolution): Boolean = registry.resolutionFailed(ticket)

    @Synchronized
    fun serviceLost(epoch: TransferNetworkEpoch, service: NsdServiceInfo): Boolean {
        val name = serviceKey(epoch, service) ?: return false
        return registry.serviceLost(epoch, name)
    }

    @Synchronized
    fun candidates(target: TransferTarget): List<TransferEndpointCandidate> {
        if (refresh(target.epoch) !== target.epoch) return emptyList()
        return registry.candidates(target)
    }

    /**
     * Recheck before using native transport. The transport must still bind this Network
     * before connect and fail on loss, with no default-network retry.
     */
    @Synchronized
    fun networkFor(candidate: TransferEndpointCandidate): Network? {
        if (refresh(candidate.target.epoch) !== candidate.target.epoch ||
            !registry.isCurrent(candidate)
        ) return null
        return binding?.network
    }

    @Synchronized
    fun networkLost(epoch: TransferNetworkEpoch): Boolean {
        if (binding?.epoch !== epoch) return false
        binding = null
        return registry.networkLost(epoch)
    }

    @Synchronized
    override fun close() { binding = null; registry.close() }

    private fun matchesNetwork(service: NsdServiceInfo): Boolean {
        val active = binding ?: return false
        // Older NSD APIs have no Network provenance; subnet checks and pinned TLS are still required.
        return Build.VERSION.SDK_INT < 33 || service.network == active.network
    }

    private fun failed(ticket: TransferResolution): Boolean {
        registry.resolutionFailed(ticket)
        return false
    }

    private fun ascii(bytes: ByteArray?, length: Int): String? {
        if (bytes == null || bytes.size != length || bytes.any { it < 0 }) return null
        return bytes.copyOf().toString(Charsets.US_ASCII)
    }
}
