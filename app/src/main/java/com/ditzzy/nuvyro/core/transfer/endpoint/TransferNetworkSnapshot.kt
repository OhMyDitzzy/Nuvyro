package com.ditzzy.nuvyro.core.transfer.endpoint

import java.net.Inet6Address
import java.net.InetAddress

internal class TransferHost private constructor(
    private val bytes: ByteArray,
    val scopeId: Int
) {
    val isIpv6 get() = bytes.size == 16
    fun addressBytes(): ByteArray = bytes.copyOf()
    fun toInetAddress(): InetAddress = if (isIpv6) {
        Inet6Address.getByAddress(null, bytes.copyOf(), scopeId)
    } else {
        InetAddress.getByAddress(bytes.copyOf())
    }

    override fun equals(other: Any?): Boolean =
        other is TransferHost && scopeId == other.scopeId && bytes.contentEquals(other.bytes)
    override fun hashCode(): Int = 31 * bytes.contentHashCode() + scopeId
    override fun toString(): String = if (isIpv6) "TransferHost(IPv6)" else "TransferHost(IPv4)"

    companion object {
        internal fun from(address: InetAddress, interfaceIndex: Int): TransferHost? {
            if (interfaceIndex <= 0 || address.isAnyLocalAddress || address.isLoopbackAddress ||
                address.isMulticastAddress
            ) return null
            val bytes = address.address
            if (bytes.size == 4) {
                val first = bytes[0].toInt() and 0xff
                if (first == 0 || first >= 224) return null
                return TransferHost(bytes.copyOf(), 0)
            }
            if (address !is Inet6Address || bytes.size != 16) return null
            // Reject IPv4-compatible/mapped forms rather than changing address families.
            if (bytes.take(12).all { it == 0.toByte() } ||
                (bytes.take(10).all { it == 0.toByte() } &&
                    bytes[10] == (-1).toByte() && bytes[11] == (-1).toByte())
            ) return null
            if (address.scopeId != 0 && address.scopeId != interfaceIndex) return null
            return TransferHost(bytes.copyOf(), if (address.isLinkLocalAddress) interfaceIndex else 0)
        }
    }
}

internal data class TransferLocalAddress(val address: InetAddress, val prefixLength: Int)

internal class TransferNetworkSnapshot private constructor(
    val networkHandle: Long,
    val interfaceIndex: Int,
    private val prefixes: List<Prefix>,
    private val gateways: List<TransferHost>
) {
    private data class Prefix(val local: TransferHost, val bits: Int)

    fun sameRoutingAs(other: TransferNetworkSnapshot): Boolean =
        networkHandle == other.networkHandle && interfaceIndex == other.interfaceIndex &&
            prefixes.toSet() == other.prefixes.toSet() && gateways.toSet() == other.gateways.toSet()

    fun gatewayHosts(): List<TransferHost> = gateways.toList()

    fun peer(address: InetAddress): TransferHost? {
        val host = TransferHost.from(address, interfaceIndex) ?: return null
        if (prefixes.any { it.local == host }) return null
        val bytes = host.addressBytes()
        val prefix = prefixes.filter { prefix ->
            val local = prefix.local.addressBytes()
            bytes.size == local.size && matchesPrefix(bytes, local, prefix.bits)
        }.maxByOrNull { it.bits } ?: return null
        return host.takeUnless { isIpv4SubnetBoundary(bytes, prefix.bits) }
    }

    companion object {
        const val MaxLocalAddresses = 16
        const val MaxGateways = 8

        fun create(
            networkHandle: Long,
            interfaceIndex: Int,
            localAddresses: List<TransferLocalAddress>,
            gatewayAddresses: List<InetAddress>
        ): TransferNetworkSnapshot? {
            if (networkHandle == 0L || interfaceIndex <= 0 ||
                localAddresses.isEmpty() || localAddresses.size > MaxLocalAddresses ||
                gatewayAddresses.size > MaxGateways
            ) return null
            val prefixes = localAddresses.mapNotNull { local ->
                val host = TransferHost.from(local.address, interfaceIndex) ?: return@mapNotNull null
                if (local.prefixLength !in 1..local.address.address.size * 8) return@mapNotNull null
                Prefix(host, local.prefixLength)
            }.distinct()
            if (prefixes.isEmpty()) return null
            val base = TransferNetworkSnapshot(networkHandle, interfaceIndex, prefixes, emptyList())
            val gateways = gatewayAddresses.mapNotNull(base::peer).distinct()
            return TransferNetworkSnapshot(networkHandle, interfaceIndex, prefixes, gateways)
        }

        private fun matchesPrefix(bytes: ByteArray, local: ByteArray, bits: Int): Boolean {
            val whole = bits / 8
            val extra = bits % 8
            return (0 until whole).all { bytes[it] == local[it] } &&
                (extra == 0 || ((bytes[whole].toInt() xor local[whole].toInt()) and
                    (0xff shl (8 - extra))) == 0)
        }

        private fun isIpv4SubnetBoundary(bytes: ByteArray, bits: Int): Boolean {
            if (bytes.size != 4 || bits >= 31) return false
            val hostBits = (bits until 32).map { bit ->
                (bytes[bit / 8].toInt() ushr (7 - bit % 8)) and 1
            }
            return hostBits.all { it == 0 } || hostBits.all { it == 1 }
        }
    }
}
