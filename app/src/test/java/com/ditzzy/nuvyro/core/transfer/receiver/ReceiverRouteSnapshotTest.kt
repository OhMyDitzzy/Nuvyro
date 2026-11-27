package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.endpoint.TransferLocalAddress
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferNetworkSnapshot
import java.net.Inet6Address
import java.net.InetAddress
import org.junit.Assert.*
import org.junit.Test

class ReceiverRouteSnapshotTest {
    private fun ip(last: Int = 1) = InetAddress.getByAddress(byteArrayOf(192.toByte(),168.toByte(),42,last.toByte()))
    private fun local(last: Int = 1) = TransferLocalAddress(ip(last),24)
    private fun iface(index: Int = 7, up: Boolean = true, loopback: Boolean = false,
                      addresses: List<TransferLocalAddress> = listOf(local())) =
        ReceiverInterfaceSnapshot(index,up,loopback,addresses)

    @Test fun wifiPreservesUnsignedNetworkBitsAndAllLocalAddresses() {
        val network=requireNotNull(TransferNetworkSnapshot.create(-1,7,listOf(local(),local(2)),emptyList()))
        val snapshot=ReceiverRouteSnapshot.wifi(network)
        assertEquals(-1L,snapshot.networkHandle);assertEquals(7,snapshot.interfaceIndex)
        assertEquals(2,snapshot.localAddresses().size);assertTrue(snapshot.clientAddresses().isEmpty())
        assertEquals("ReceiverRouteSnapshot(WiFi)",snapshot.toString())
    }

    @Test fun hotspotDoesNotInventAClientNetworkHandle() {
        val snapshot=requireNotNull(ReceiverRouteSnapshot.hotspot(listOf(iface()),emptyList()))
        assertNull(snapshot.networkHandle);assertEquals(7,snapshot.interfaceIndex)
        assertEquals(listOf(local()),snapshot.localAddresses());assertEquals("ReceiverRouteSnapshot(Hotspot)",snapshot.toString())
    }

    @Test fun everyClientInterfaceAndLocalAddressIsExcludedIncludingVpnAndUplinks() {
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface()),listOf(ReceiverClientSnapshot(7,emptyList()))))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface()),listOf(ReceiverClientSnapshot(8,listOf(ip())))))
        val clients=listOf(ReceiverClientSnapshot(8,listOf(ip(8))),ReceiverClientSnapshot(9,listOf(ip(9))))
        val snapshot=requireNotNull(ReceiverRouteSnapshot.hotspot(listOf(iface(),iface(8,addresses=listOf(local(8))),iface(9,addresses=listOf(local(9)))),clients))
        assertEquals(setOf(8,9),snapshot.clientInterfaces().toSet());assertEquals(setOf(ip(8),ip(9)),snapshot.clientAddresses().toSet())
    }

    @Test fun ambiguousUnmanagedInterfacesAreRejectedInsteadOfGuessingTheHotspot() {
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface(),iface(8,addresses=listOf(local(2)))),emptyList()))
        assertNull(ReceiverRouteSnapshot.hotspot(emptyList(),emptyList()))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface(up=false)),emptyList()))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface(loopback=true)),emptyList()))
    }

    @Test fun unusablePrefixesAndNonUnicastAddressesCannotProduceABinding() {
        for ((address,prefix) in listOf(InetAddress.getByAddress(byteArrayOf(0,0,0,0)) to 24,
            InetAddress.getByAddress(byteArrayOf(127,0,0,1)) to 8,
            InetAddress.getByAddress(byteArrayOf(224.toByte(),0,0,1)) to 24,ip() to 0,ip() to 33)) {
            assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface(addresses=listOf(TransferLocalAddress(address,prefix)))),emptyList()))
        }
    }

    @Test fun linkLocalIpv6IsScopedToTheActualInterface() {
        val bytes=ByteArray(16).apply { this[0]=0xfe.toByte();this[1]=0x80.toByte();this[15]=1 }
        val address=Inet6Address.getByAddress(null,bytes,0)
        val snapshot=requireNotNull(ReceiverRouteSnapshot.hotspot(listOf(iface(addresses=listOf(TransferLocalAddress(address,64)))),emptyList()))
        assertEquals(7,(snapshot.localAddresses().single().address as Inet6Address).scopeId)
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface(addresses=listOf(TransferLocalAddress(Inet6Address.getByAddress(null,bytes,8),64)))),emptyList()))
    }

    @Test fun incompleteOrOversizedEvidenceFailsClosed() {
        assertNull(ReceiverRouteSnapshot.hotspot(List(65){iface(it+1)},emptyList()))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface()),List(33){ReceiverClientSnapshot(it+20,emptyList())}))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface()),listOf(ReceiverClientSnapshot(8,List(129){ip(2)}))))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface(0)),emptyList()))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface(),iface()),emptyList()))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface(addresses=List(17){local()})),emptyList()))
        assertNull(ReceiverRouteSnapshot.hotspot(listOf(iface()),listOf(ReceiverClientSnapshot(0,emptyList()))))
    }

    @Test fun snapshotsCopyListsAndCompareActualRoutingWithoutDependingOnInputOrder() {
        val addresses=mutableListOf(local(),local(2));val clients=mutableListOf(ReceiverClientSnapshot(8,listOf(ip(8))))
        val a=requireNotNull(ReceiverRouteSnapshot.hotspot(listOf(iface(addresses=addresses)),clients))
        val b=requireNotNull(ReceiverRouteSnapshot.hotspot(listOf(iface(addresses=addresses.reversed())),clients))
        assertTrue(a.sameRoutingAs(b));addresses.clear();clients.clear();assertEquals(2,a.localAddresses().size);assertEquals(listOf(8),a.clientInterfaces())
        val changed=requireNotNull(ReceiverRouteSnapshot.hotspot(listOf(iface(addresses=listOf(local(),local(2)))),listOf(ReceiverClientSnapshot(9,listOf(ip(9))))))
        assertFalse(a.sameRoutingAs(changed))
        val wifi=ReceiverRouteSnapshot.wifi(requireNotNull(TransferNetworkSnapshot.create(1,7,a.localAddresses(),emptyList())))
        assertFalse(a.sameRoutingAs(wifi))
    }

    @Test fun absentConnectivityCannotBeReplacedByDefaultInterfacesOrAnActiveUiFlag() {
        val reader=AndroidReceiverRoutes(null,interfaces={error("must not enumerate without connectivity")})
        assertNull(reader.hotspot {true});assertNull(reader.hotspot {false})
    }
}
