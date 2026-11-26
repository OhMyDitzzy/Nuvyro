package com.ditzzy.nuvyro.core.lan

import android.net.LinkAddress
import java.net.InetAddress

internal fun isOnLan(address: InetAddress, links: List<LinkAddress>): Boolean =
    links.any { isInSubnet(address, it.address, it.prefixLength) }

internal fun isInSubnet(address: InetAddress, localAddress: InetAddress, prefixLength: Int): Boolean {
    if (address.isAnyLocalAddress || address.isLoopbackAddress || address.isMulticastAddress) return false
    val bytes = address.address
    val local = localAddress.address
    if (bytes.size != local.size || prefixLength !in 0..local.size * 8) return false
    val whole = prefixLength / 8
    val bits = prefixLength % 8
    return (0 until whole).all { bytes[it] == local[it] } &&
        (bits == 0 || ((bytes[whole].toInt() xor local[whole].toInt()) and
            (0xff shl (8 - bits))) == 0)
}
