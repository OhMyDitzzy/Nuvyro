package com.ditzzy.nuvyro.core.transfer.endpoint

import java.net.InetAddress

internal data class ResolvedTransferService(
    val name: String,
    val type: String,
    val version: String,
    val sessionId: String,
    val port: Int,
    val addresses: List<InetAddress>
)

/** Bounded, process-local hints. The owner supplies a monotonic millisecond clock. */
internal class TransferEndpointRegistry(private val clock: () -> Long) : AutoCloseable {
    private class ActiveNetwork(
        val epoch: TransferNetworkEpoch,
        val snapshot: TransferNetworkSnapshot,
        var observedAt: Long,
        val gateways: List<TransferRoute>
    )

    private class Service(
        var pending: TransferResolution? = null,
        var routes: List<TransferRoute> = emptyList(),
        val resolvedAt: Long = 0
    )

    private var network: ActiveNetwork? = null
    private var target: TransferTarget? = null
    private val services = linkedMapOf<String, Service>()
    private var generation = 0L
    private var lastNow = 0L
    private var closed = false

    @Synchronized
    fun activate(snapshot: TransferNetworkSnapshot): TransferNetworkEpoch? {
        val now = now() ?: return null
        clear()
        if (generation == Long.MAX_VALUE) { closed = true; return null }
        val epoch = TransferNetworkEpoch(++generation, snapshot.networkHandle)
        val gateways = snapshot.gatewayHosts().distinctBy { it.isIpv6 }.take(2).map {
            TransferRoute(it, TransferEndpointSource.Gateway, null)
        }
        network = ActiveNetwork(epoch, snapshot, now, gateways)
        return epoch
    }

    @Synchronized
    fun refresh(
        epoch: TransferNetworkEpoch,
        snapshot: TransferNetworkSnapshot
    ): TransferNetworkEpoch? {
        val now = now() ?: return null
        val active = network?.takeIf { it.epoch === epoch } ?: return null
        if (snapshot.networkHandle != epoch.networkHandle) { clear(); return null }
        if (!active.snapshot.sameRoutingAs(snapshot)) return activate(snapshot)
        active.observedAt = now
        return epoch
    }

    @Synchronized
    fun networkLost(epoch: TransferNetworkEpoch): Boolean {
        if (network?.epoch !== epoch) return false
        clear()
        return true
    }

    @Synchronized
    fun select(epoch: TransferNetworkEpoch, sessionId: SecureSessionId): TransferTarget? {
        val now = now() ?: return null
        if (!freshNetwork(epoch, now)) return null
        return TransferTarget(epoch, sessionId).also { target = it }
    }

    @Synchronized
    fun clearSelection(selection: TransferTarget): Boolean {
        if (target !== selection) return false
        target = null
        return true
    }

    @Synchronized
    fun beginResolution(
        epoch: TransferNetworkEpoch,
        serviceName: String,
        serviceType: String
    ): TransferResolution? {
        val now = now() ?: return null
        if (!freshNetwork(epoch, now) || !TransferEndpointContract.acceptsName(serviceName) ||
            !TransferEndpointContract.acceptsType(serviceType)
        ) return null
        val existing = services[serviceName]
        if (existing?.pending != null ||
            (existing == null && services.size >= TransferEndpointContract.MaxServices) ||
            services.values.count { it.pending != null } >= TransferEndpointContract.MaxPendingResolutions
        ) return null
        val ticket = TransferResolution(epoch, serviceName, now)
        val service = existing ?: Service().also { services[serviceName] = it }
        service.pending = ticket
        return ticket
    }

    @Synchronized
    fun resolved(ticket: TransferResolution, info: ResolvedTransferService): Boolean {
        val now = now() ?: return false
        val active = network ?: return false
        if (!freshNetwork(ticket.epoch, now) ||
            services[ticket.serviceName]?.pending !== ticket
        ) return false
        services.remove(ticket.serviceName)
        if (info.name != ticket.serviceName || !TransferEndpointContract.acceptsType(info.type) ||
            info.version != TransferEndpointContract.Version || info.port != TransferEndpointContract.Port ||
            info.addresses.isEmpty() || info.addresses.size > TransferEndpointContract.MaxResolvedAddresses
        ) return false
        val sessionId = SecureSessionId.parse(info.sessionId) ?: return false
        val routes = info.addresses.mapNotNull(active.snapshot::peer).distinct()
            .take(TransferEndpointContract.MaxAddressesPerService).map {
                TransferRoute(it, TransferEndpointSource.Nsd, sessionId)
            }
        if (routes.isEmpty()) return false
        services[info.name] = Service(routes = routes, resolvedAt = now)
        return true
    }

    @Synchronized
    fun resolutionFailed(ticket: TransferResolution): Boolean {
        if (network?.epoch !== ticket.epoch || services[ticket.serviceName]?.pending !== ticket) return false
        services.remove(ticket.serviceName)
        return true
    }

    @Synchronized
    fun serviceLost(epoch: TransferNetworkEpoch, name: String): Boolean {
        if (network?.epoch !== epoch) return false
        return services.remove(name) != null
    }

    @Synchronized
    fun candidates(selection: TransferTarget): List<TransferEndpointCandidate> {
        val now = now() ?: return emptyList()
        if (target !== selection || !freshNetwork(selection.epoch, now)) return emptyList()
        val active = network ?: return emptyList()
        val gateways = active.gateways
        val routes = services.values.flatMap { it.routes }
            .filter { route ->
                route.advertisedSessionId == selection.expectedSessionId &&
                    gateways.none { it.host == route.host }
            }.distinctBy { it.host }
            .take(TransferEndpointContract.MaxCandidates - gateways.size) + gateways
        return routes
            .map { TransferEndpointCandidate(selection, it, now) }
    }

    @Synchronized
    fun isCurrent(candidate: TransferEndpointCandidate): Boolean {
        val now = now() ?: return false
        if (target !== candidate.target || !freshNetwork(candidate.target.epoch, now) ||
            now - candidate.issuedAt !in 0L until TransferEndpointContract.FreshnessMillis
        ) return false
        val active = network ?: return false
        return active.gateways.any { it === candidate.route } ||
            services.values.any { service -> service.routes.any { it === candidate.route } }
    }

    @Synchronized
    override fun close() { clear(); closed = true }

    private fun freshNetwork(epoch: TransferNetworkEpoch, now: Long): Boolean {
        val active = network ?: return false
        return active.epoch === epoch &&
            now - active.observedAt in 0L until TransferEndpointContract.FreshnessMillis
    }

    private fun now(): Long? {
        if (closed) return null
        val value = clock()
        if (value < 0 || value < lastNow) { close(); return null }
        lastNow = value
        val iterator = services.values.iterator()
        while (iterator.hasNext()) {
            val service = iterator.next()
            val expired = service.pending?.let {
                value - it.startedAt >= TransferEndpointContract.ResolveTimeoutMillis
            } ?: false
            val staleRoutes = service.routes.isNotEmpty() &&
                value - service.resolvedAt >= TransferEndpointContract.FreshnessMillis
            if (expired || (staleRoutes && service.pending == null)) iterator.remove()
            else if (staleRoutes) service.routes = emptyList()
        }
        return value
    }

    private fun clear() { network = null; target = null; services.clear() }
}
