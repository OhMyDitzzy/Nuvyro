package com.ditzzy.nuvyro.core.transfer.receiver

import android.net.ConnectivityManager
import android.net.Network
import com.ditzzy.nuvyro.core.transfer.endpoint.AndroidTransferNetworkReader
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferLocalAddress
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferNetworkSnapshot
import java.net.NetworkInterface
import java.net.SocketException

/** Reads on the owner's IO context. Construction starts no listener, native engine or discovery. */
internal class AndroidReceiverRoutes(
    private val connectivity: ConnectivityManager?,
    private val interfaceIndex: (String) -> Int? = { name ->
        NetworkInterface.getByName(name)?.takeIf { it.isUp && !it.isLoopback }?.index
    },
    private val interfaces: () -> List<ReceiverInterfaceSnapshot>? = ::readInterfaces
) {
    private val wifiReader = AndroidTransferNetworkReader(connectivity, interfaceIndex)

    fun wifi(network: Network): ReceiverRouteSnapshot? = wifiReader.read(network)?.let(ReceiverRouteSnapshot::wifi)

    /** The callback must reflect reservation ownership, never a UI toggle or a cached SSID. */
    @Suppress("DEPRECATION")
    fun hotspot(ownsReservation: () -> Boolean): ReceiverRouteSnapshot? = try {
        readHotspot(ownsReservation)
    } catch (_: SecurityException) {
        null
    } catch (_: SocketException) {
        null
    } catch (_: IllegalArgumentException) {
        null
    }

    @Suppress("DEPRECATION")
    private fun readHotspot(ownsReservation: () -> Boolean): ReceiverRouteSnapshot? {
        val manager = connectivity ?: return null
        if (!ownsReservation()) return null
        val networks = manager.allNetworks
        if (networks.size > ReceiverRouteSnapshot.MaxClients) return null
        val clients = networks.map { network ->
            val properties = manager.getLinkProperties(network) ?: return null
            val name = properties.interfaceName ?: return null
            val index = interfaceIndex(name)?.takeIf { it > 0 } ?: return null
            val addresses = properties.linkAddresses
            if (addresses.size > TransferNetworkSnapshot.MaxLocalAddresses) return null
            ReceiverClientSnapshot(index, addresses.map { it.address })
        }
        val snapshot = ReceiverRouteSnapshot.hotspot(interfaces() ?: return null, clients)
        return snapshot.takeIf { ownsReservation() }
    }
}

private fun readInterfaces(): List<ReceiverInterfaceSnapshot>? {
    val all = NetworkInterface.getNetworkInterfaces()?.asSequence()
        ?.take(ReceiverRouteSnapshot.MaxInterfaces + 1)?.toList() ?: return null
    if (all.size > ReceiverRouteSnapshot.MaxInterfaces) return null
    return all.map { network ->
        val addresses = network.interfaceAddresses
        if (addresses.size > TransferNetworkSnapshot.MaxLocalAddresses) return null
        ReceiverInterfaceSnapshot(network.index, network.isUp, network.isLoopback,
            addresses.map { TransferLocalAddress(it.address, it.networkPrefixLength.toInt()) })
    }
}
