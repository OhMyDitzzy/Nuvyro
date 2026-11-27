package com.ditzzy.nuvyro.core.transfer.endpoint

import java.net.Inet6Address
import java.net.InetAddress

internal class EndpointClock(var millis: Long = 1_000L) {
    fun read(): Long = millis
}

internal object EndpointTestData {
    val Session = requireNotNull(SecureSessionId.parse("1".repeat(32)))
    val OtherSession = requireNotNull(SecureSessionId.parse("2".repeat(32)))

    fun ip(a: Int = 192, b: Int = 168, c: Int = 42, d: Int = 3): InetAddress =
        InetAddress.getByAddress(byteArrayOf(a.toByte(), b.toByte(), c.toByte(), d.toByte()))

    fun ipv6(hex: String = "fe800000000000000000000000000003", scope: Int = 0): Inet6Address =
        Inet6Address.getByAddress(null, hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray(), scope)

    fun snapshot(
        handle: Long = 101L,
        index: Int = 7,
        gateways: List<InetAddress> = emptyList(),
        locals: List<TransferLocalAddress> = listOf(TransferLocalAddress(ip(d = 2), 24))
    ): TransferNetworkSnapshot = requireNotNull(
        TransferNetworkSnapshot.create(handle, index, locals, gateways)
    )

    fun service(
        name: String = "Receiver",
        session: SecureSessionId = Session,
        addresses: List<InetAddress> = listOf(ip())
    ) = ResolvedTransferService(
        name, TransferEndpointContract.ServiceType, "1.0", session.hex,
        TransferEndpointContract.Port, addresses
    )

    fun resolve(
        registry: TransferEndpointRegistry,
        epoch: TransferNetworkEpoch,
        info: ResolvedTransferService = service()
    ): Boolean {
        val ticket = registry.beginResolution(epoch, info.name, info.type) ?: return false
        return registry.resolved(ticket, info)
    }
}
