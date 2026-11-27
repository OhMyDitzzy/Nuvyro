package com.ditzzy.nuvyro.core.transfer.endpoint

import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.ip
import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.ipv6
import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.snapshot
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TransferNetworkSnapshotTest {
    @Test
    fun onlyUnicastPeersOnTheSelectedSubnetAreAccepted() {
        val network = snapshot()
        assertNotNull(network.peer(ip()))
        for (address in listOf(
            ip(0, 0, 0, 0), ip(127, 0, 0, 1), ip(224, 0, 0, 1), ip(255, 255, 255, 255),
            ip(d = 0), ip(d = 255), ip(d = 2), ip(c = 43)
        )) assertNull(address.hostAddress, network.peer(address))
    }

    @Test
    fun prefixesDoNotTurnIntoDefaultRoutesAndPartialBytesAreMatched() {
        assertNull(TransferNetworkSnapshot.create(
            101, 7, listOf(TransferLocalAddress(ip(d = 2), 0)), emptyList()
        ))
        val network = snapshot(locals = listOf(TransferLocalAddress(ip(10, 1, 128, 1), 17)))
        assertNotNull(network.peer(ip(10, 1, 255, 3)))
        assertNull(network.peer(ip(10, 1, 127, 3)))
        val pointToPoint = snapshot(locals = listOf(TransferLocalAddress(ip(d = 2), 31)))
        assertNotNull(pointToPoint.peer(ip(d = 3)))
    }

    @Test
    fun aBroaderPrefixCannotReclassifyTheSpecificSubnetBroadcastAsUnicast() {
        val network = snapshot(locals = listOf(
            TransferLocalAddress(ip(10, 0, 0, 1), 8),
            TransferLocalAddress(ip(10, 1, 2, 1), 24)
        ))
        assertNull(network.peer(ip(10, 1, 2, 255)))
        assertNull(network.peer(ip(10, 1, 2, 0)))
        assertNotNull(network.peer(ip(10, 1, 2, 3)))
    }

    @Test
    fun ipv6LinkLocalScopeIsAssignedToThisInterfaceAndNeverAnotherOne() {
        val network = snapshot(locals = listOf(
            TransferLocalAddress(ipv6("fe800000000000000000000000000002"), 64)
        ))
        val host = requireNotNull(network.peer(ipv6()))
        assertEquals(7, host.scopeId)
        assertEquals(7, (host.toInetAddress() as java.net.Inet6Address).scopeId)
        assertEquals(host, network.peer(ipv6(scope = 7)))
        assertNull(network.peer(ipv6(scope = 8)))
        assertNull(network.peer(ipv6("fe800000000000000000000000000002")))
        assertNull(network.peer(ipv6("ff020000000000000000000000000001")))
        assertNull(network.peer(ipv6("00000000000000000000000000000000")))
        assertNull(network.peer(ipv6("00000000000000000000000000000001")))
    }

    @Test
    fun globalIpv6UsesTheCurrentPrefixWithoutInventingALinkScope() {
        val network = snapshot(locals = listOf(
            TransferLocalAddress(ipv6("20010db8000100000000000000000002"), 64)
        ))
        val host = requireNotNull(network.peer(ipv6("20010db8000100000000000000000003")))
        assertEquals(0, host.scopeId)
        assertNull(network.peer(ipv6("20010db8000200000000000000000003")))
        assertNull(network.peer(ipv6("20010db8000100000000000000000003", scope = 8)))
    }

    @Test
    fun ipv4MappedAndCompatibleIpv6DoNotChangeFamilies() {
        assertNull(TransferHost.from(ipv6("00000000000000000000ffffc0a82a03"), 7))
        assertNull(TransferHost.from(ipv6("000000000000000000000000c0a82a03"), 7))
    }

    @Test
    fun snapshotAndHostBytesCannotBeChangedThroughCallerCollections() {
        val gateways = mutableListOf(ip(d = 1))
        val locals = mutableListOf(TransferLocalAddress(ip(d = 2), 24))
        val network = snapshot(gateways = gateways, locals = locals)
        gateways.clear()
        locals.clear()
        val host = network.gatewayHosts().single()
        val original = host.addressBytes()
        host.addressBytes().fill(0)
        assertArrayEquals(original, host.addressBytes())
        assertEquals(ip(d = 1), host.toInetAddress())
        assertEquals(listOf(host), network.gatewayHosts())
        assertNotNull(network.peer(ip()))
    }

    @Test
    fun gatewayHintsMustBeOnTheCurrentLanAndDuplicatesAreRemoved() {
        val network = snapshot(gateways = listOf(
            ip(d = 1), ip(d = 1), ip(c = 43), ip(d = 2), ip(d = 255)
        ))
        assertEquals(listOf(ip(d = 1)), network.gatewayHosts().map { it.toInetAddress() })
    }

    @Test
    fun opaqueNetworkHandleIsNotTruncatedOrRequiredToBeSignedPositive() {
        val handle = Long.MIN_VALUE + 123
        assertEquals(handle, snapshot(handle = handle).networkHandle)
        assertNull(TransferNetworkSnapshot.create(
            0, 7, listOf(TransferLocalAddress(ip(d = 2), 24)), emptyList()
        ))
        assertNull(TransferNetworkSnapshot.create(
            101, 0, listOf(TransferLocalAddress(ip(d = 2), 24)), emptyList()
        ))
    }

    @Test
    fun inputLimitsAndInvalidPrefixesFailWithoutWideningTheLan() {
        val local = TransferLocalAddress(ip(d = 2), 24)
        assertNull(TransferNetworkSnapshot.create(101, 7, List(17) { local }, emptyList()))
        assertNull(TransferNetworkSnapshot.create(101, 7, listOf(local), List(9) { ip(d = 1) }))
        for (bits in listOf(-1, 0, 33)) {
            assertNull(TransferNetworkSnapshot.create(
                101, 7, listOf(TransferLocalAddress(ip(d = 2), bits)), emptyList()
            ))
        }
    }

    @Test
    fun routingEqualityIgnoresOrderingButIncludesNetworkAndInterface() {
        val local = listOf(
            TransferLocalAddress(ip(d = 2), 24),
            TransferLocalAddress(ipv6("fe800000000000000000000000000002"), 64)
        )
        val first = snapshot(locals = local, gateways = listOf(ip(d = 1), ipv6()))
        val reordered = snapshot(locals = local.reversed(), gateways = listOf(ipv6(), ip(d = 1)))
        assertTrue(first.sameRoutingAs(reordered))
        assertFalse(first.sameRoutingAs(snapshot(handle = 102, locals = local)))
        assertFalse(first.sameRoutingAs(snapshot(index = 8, locals = local)))
    }
}
