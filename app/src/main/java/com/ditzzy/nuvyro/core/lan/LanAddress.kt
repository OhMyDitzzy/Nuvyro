package com.ditzzy.nuvyro.core.lan

import android.net.LinkAddress
import java.net.InetAddress

internal fun isOnLan(address: InetAddress, links: List<LinkAddress>): Boolean {
    if (address.isAnyLocalAddress || address.isLoopbackAddress || address.isMulticastAddress) return false
    val bytes = address.address
    return links.any { link ->
        val local = link.address.address
        if (bytes.size != local.size) return@any false
        val whole = link.prefixLength / 8
        val bits = link.prefixLength % 8
        (0 until whole).all { bytes[it] == local[it] } &&
            (bits == 0 || ((bytes[whole].toInt() xor local[whole].toInt()) and
                (0xff shl (8 - bits))) == 0)
    }
}
