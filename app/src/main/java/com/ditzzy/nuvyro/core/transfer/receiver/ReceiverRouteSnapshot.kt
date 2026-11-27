package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.endpoint.TransferHost
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferLocalAddress
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferNetworkSnapshot
import java.net.InetAddress

internal data class ReceiverInterfaceSnapshot(
    val index: Int,
    val isUp: Boolean,
    val isLoopback: Boolean,
    val addresses: List<TransferLocalAddress>
)

internal data class ReceiverClientSnapshot(val interfaceIndex: Int, val addresses: List<InetAddress>)

/** Immutable input to the native route policy; it proves no peer identity or transfer consent. */
internal class ReceiverRouteSnapshot private constructor(
    val networkHandle: Long?,
    val interfaceIndex: Int,
    locals: List<TransferLocalAddress>,
    clients: List<InetAddress>,
    interfaces: List<Int>
) {
    private val locals = locals.toList()
    private val clients = clients.toList()
    private val interfaces = interfaces.toList()

    fun localAddresses(): List<TransferLocalAddress> = locals.toList()
    fun clientAddresses(): List<InetAddress> = clients.toList()
    fun clientInterfaces(): List<Int> = interfaces.toList()

    fun sameRoutingAs(other: ReceiverRouteSnapshot): Boolean =
        networkHandle == other.networkHandle && interfaceIndex == other.interfaceIndex &&
            locals.toSet() == other.locals.toSet() && clients.toSet() == other.clients.toSet() &&
            interfaces.toSet() == other.interfaces.toSet()

    override fun toString(): String = "ReceiverRouteSnapshot(${if (networkHandle == null) "Hotspot" else "WiFi"})"

    companion object {
        const val MaxInterfaces = 64
        const val MaxClients = 32
        const val MaxClientAddresses = 128

        fun wifi(network: TransferNetworkSnapshot): ReceiverRouteSnapshot = ReceiverRouteSnapshot(
            network.networkHandle, network.interfaceIndex, network.localAddresses(), emptyList(), emptyList()
        )

        /** Called only while the coordinator still owns an active local-only hotspot reservation. */
        fun hotspot(
            interfaces: List<ReceiverInterfaceSnapshot>,
            clients: List<ReceiverClientSnapshot>
        ): ReceiverRouteSnapshot? {
            if (interfaces.size > MaxInterfaces || clients.size > MaxClients ||
                clients.any { it.interfaceIndex <= 0 } ||
                clients.sumOf { it.addresses.size.toLong() } > MaxClientAddresses ||
                interfaces.any { it.index <= 0 || it.addresses.size > TransferNetworkSnapshot.MaxLocalAddresses } ||
                interfaces.map { it.index }.distinct().size != interfaces.size
            ) return null
            val clientInterfaces = clients.map { it.interfaceIndex }.distinct()
            val clientAddresses = clients.flatMap { it.addresses }.distinct()
            val candidates = interfaces.filter {
                it.isUp && !it.isLoopback && it.index !in clientInterfaces &&
                    it.addresses.none { local -> local.address in clientAddresses }
            }.mapNotNull { candidate ->
                val locals = candidate.addresses.mapNotNull { local ->
                    val host = TransferHost.from(local.address, candidate.index) ?: return@mapNotNull null
                    if (local.prefixLength !in 1..local.address.address.size * 8) return@mapNotNull null
                    TransferLocalAddress(host.toInetAddress(), local.prefixLength)
                }.distinct()
                locals.takeIf { it.isNotEmpty() }?.let { candidate.index to it }
            }
            // Guessing among unmanaged interfaces could expose the listener on an uplink or VPN.
            val (index, locals) = candidates.singleOrNull() ?: return null
            return ReceiverRouteSnapshot(null, index, locals, clientAddresses, clientInterfaces)
        }
    }
}
