package com.ditzzy.nuvyro.core.transfer.endpoint

import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.OtherSession
import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.Session
import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.ip
import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.ipv6
import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.resolve
import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.service
import com.ditzzy.nuvyro.core.transfer.endpoint.EndpointTestData.snapshot
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class TransferEndpointRegistryTest {
    @Test
    fun secureSessionIdsRequireCanonicalNonzeroHexWithoutProfileUuidAliasing() {
        for (invalid in listOf("", "0".repeat(32), "a".repeat(31), "A".repeat(32),
            "g".repeat(32), "11111111-1111-1111-1111-111111111111"
        )) assertNull(SecureSessionId.parse(invalid))
        assertEquals(Session, SecureSessionId.parse("1".repeat(32)))
    }

    @Test
    fun onlyTheSelectedSecureSessionIsReturnedAndItsPortIsFixed() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        assertTrue(resolve(registry, epoch, service("One")))
        assertTrue(resolve(registry, epoch, service("Two", OtherSession, listOf(ip(d = 4)))))
        val selection = requireNotNull(registry.select(epoch, Session))
        val candidate = registry.candidates(selection).single()
        assertEquals(Session, candidate.expectedSessionId)
        assertEquals(47739, candidate.port)
        assertEquals(101L, candidate.networkHandle)
        assertEquals(ip(), candidate.host.toInetAddress())
        assertEquals(TransferEndpointSource.Nsd, candidate.source)
        assertTrue(registry.isCurrent(candidate))
    }

    @Test
    fun profileServicesWrongPortsAndVersionsCannotBecomeTransferEndpoints() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        assertNull(registry.beginResolution(epoch, "Profile", "_nuvyro._tcp."))
        for (info in listOf(
            service().copy(port = 47738), service().copy(version = "1"),
            service().copy(version = "1.0 "), service().copy(sessionId = "AB".repeat(16)),
            service().copy(type = "_nuvyro._tcp."), service().copy(name = "Wrong")
        )) {
            val ticket = requireNotNull(registry.beginResolution(
                epoch, "Receiver", TransferEndpointContract.ServiceType
            ))
            assertFalse(registry.resolved(ticket, info))
        }
        assertTrue(registry.candidates(requireNotNull(registry.select(epoch, Session))).isEmpty())
    }

    @Test
    fun nsdTypeIsCaseInsensitiveButInstanceNamesAndSessionBytesAreExact() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        val ticket = requireNotNull(registry.beginResolution(epoch, "One", "_NUVYRO-XFER._TCP"))
        assertTrue(registry.resolved(ticket, service("One").copy(type = "_NUVYRO-XFER._TCP.")))
        assertNull(registry.beginResolution(epoch, "x".repeat(64), TransferEndpointContract.ServiceType))
        assertNull(registry.beginResolution(epoch, "😀".repeat(20), TransferEndpointContract.ServiceType))
        assertNull(registry.beginResolution(epoch, "bad\nname", TransferEndpointContract.ServiceType))
        assertTrue(TransferEndpointContract.acceptsName("Receiver 😀"))
        assertFalse(TransferEndpointContract.acceptsName("bad\uD800"))
    }

    @Test
    fun lostAndReappearingServiceRejectsAnEarlierResolutionAndFailureCallback() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        val old = requireNotNull(registry.beginResolution(epoch, "Receiver", TransferEndpointContract.ServiceType))
        assertTrue(registry.serviceLost(epoch, "Receiver"))
        val current = requireNotNull(registry.beginResolution(epoch, "Receiver", TransferEndpointContract.ServiceType))
        assertFalse(registry.resolved(old, service()))
        assertFalse(registry.resolutionFailed(old))
        assertTrue(registry.resolved(current, service()))
        assertFalse(registry.resolved(current, service()))
    }

    @Test
    fun losingOrReplacingAResolvedServiceInvalidatesAlreadyReturnedCandidates() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        val target = requireNotNull(registry.select(epoch, Session))
        assertTrue(resolve(registry, epoch))
        val lost = registry.candidates(target).single()
        assertTrue(registry.serviceLost(epoch, "Receiver"))
        assertFalse(registry.isCurrent(lost))
        assertTrue(resolve(registry, epoch))
        val replaced = registry.candidates(target).single()
        assertTrue(resolve(registry, epoch, service(addresses = listOf(ip(d = 4)))))
        assertFalse(registry.isCurrent(replaced))
        assertEquals(ip(d = 4), registry.candidates(target).single().host.toInetAddress())
    }

    @Test
    fun reselectingEvenTheSameSessionInvalidatesThePreviousAttempt() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        assertTrue(resolve(registry, epoch))
        val old = requireNotNull(registry.select(epoch, Session))
        val endpoint = registry.candidates(old).single()
        val current = requireNotNull(registry.select(epoch, Session))
        assertFalse(registry.clearSelection(old))
        assertTrue(registry.candidates(old).isEmpty())
        assertFalse(registry.isCurrent(endpoint))
        assertEquals(1, registry.candidates(current).size)
        assertTrue(registry.clearSelection(current))
        assertTrue(registry.candidates(current).isEmpty())
    }

    @Test
    fun networkReplacementRejectsOldCallbacksWithoutClearingTheNewNetwork() {
        val registry = TransferEndpointRegistry { 1_000 }
        val old = requireNotNull(registry.activate(snapshot()))
        val pending = requireNotNull(registry.beginResolution(old, "Receiver", TransferEndpointContract.ServiceType))
        val current = requireNotNull(registry.activate(snapshot(handle = 102)))
        assertTrue(current.generation > old.generation)
        assertFalse(registry.networkLost(old))
        assertNull(registry.refresh(old, snapshot()))
        assertFalse(registry.resolved(pending, service()))
        assertNull(registry.select(old, Session))
        assertNotNull(registry.select(current, Session))
        val forged = TransferNetworkEpoch(current.generation, current.networkHandle)
        assertNull(registry.select(forged, Session))
        assertFalse(registry.networkLost(forged))
    }

    @Test
    fun lateGatewayAndGatewayChangesRotateTheEpochAndInvalidateSelections() {
        val registry = TransferEndpointRegistry { 1_000 }
        val initial = requireNotNull(registry.activate(snapshot()))
        val target = requireNotNull(registry.select(initial, Session))
        assertTrue(registry.candidates(target).isEmpty())
        val available = snapshot(gateways = listOf(ip(d = 1)))
        val ready = requireNotNull(registry.refresh(initial, available))
        assertNotSame(initial, ready)
        assertTrue(registry.candidates(target).isEmpty())
        val currentTarget = requireNotNull(registry.select(ready, Session))
        val endpoint = registry.candidates(currentTarget).single()
        assertEquals(TransferEndpointSource.Gateway, endpoint.source)
        assertEquals(Session, endpoint.expectedSessionId)
        val changed = requireNotNull(registry.refresh(ready, snapshot(gateways = listOf(ip(d = 4)))))
        assertFalse(registry.isCurrent(endpoint))
        assertFalse(registry.networkLost(ready))
        val changedTarget = requireNotNull(registry.select(changed, Session))
        assertEquals(ip(d = 4), registry.candidates(changedTarget).single().host.toInetAddress())
    }

    @Test
    fun identicalRefreshKeepsTheEpochButCannotMigrateItToAnotherNetworkHandle() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        assertSame(epoch, registry.refresh(epoch, snapshot()))
        assertNull(registry.refresh(epoch, snapshot(handle = 102)))
        assertNull(registry.select(epoch, Session))
    }

    @Test
    fun networkLossAndCloseAreTerminalForAllExistingTokens() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        val target = requireNotNull(registry.select(epoch, Session))
        assertTrue(resolve(registry, epoch))
        val endpoint = registry.candidates(target).single()
        assertTrue(registry.networkLost(epoch))
        assertFalse(registry.isCurrent(endpoint))
        assertTrue(registry.candidates(target).isEmpty())
        val current = requireNotNull(registry.activate(snapshot()))
        assertNotSame(epoch, current)
        registry.close()
        registry.close()
        assertNull(registry.activate(snapshot()))
        assertNull(registry.select(current, Session))
    }

    @Test
    fun cachedRoutesAndReturnedCandidatesExpireWithoutResurrectionByRefresh() {
        val clock = EndpointClock()
        val registry = TransferEndpointRegistry(clock::read)
        val snapshot = snapshot(gateways = listOf(ip(d = 1)))
        val epoch = requireNotNull(registry.activate(snapshot))
        val target = requireNotNull(registry.select(epoch, Session))
        assertTrue(resolve(registry, epoch))
        val endpoints = registry.candidates(target)
        clock.millis += TransferEndpointContract.FreshnessMillis - 1
        assertSame(epoch, registry.refresh(epoch, snapshot))
        assertTrue(endpoints.all(registry::isCurrent))
        clock.millis++
        assertSame(epoch, registry.refresh(epoch, snapshot))
        assertTrue(endpoints.none(registry::isCurrent))
        val refreshed = registry.candidates(target).single()
        assertEquals(TransferEndpointSource.Gateway, refreshed.source)
        assertTrue(registry.isCurrent(refreshed))
    }

    @Test
    fun staleNetworkSnapshotProducesNoCandidatesUntilFreshlyRead() {
        val clock = EndpointClock()
        val registry = TransferEndpointRegistry(clock::read)
        val snapshot = snapshot(gateways = listOf(ip(d = 1)))
        val epoch = requireNotNull(registry.activate(snapshot))
        val target = requireNotNull(registry.select(epoch, Session))
        clock.millis += TransferEndpointContract.FreshnessMillis
        assertTrue(registry.candidates(target).isEmpty())
        assertNull(registry.beginResolution(epoch, "Receiver", TransferEndpointContract.ServiceType))
        assertSame(epoch, registry.refresh(epoch, snapshot))
        assertEquals(1, registry.candidates(target).size)
    }

    @Test
    fun resolutionTimeoutIsAbsoluteAndAnOldResultCannotPopulateANewAttempt() {
        val clock = EndpointClock()
        val registry = TransferEndpointRegistry(clock::read)
        val epoch = requireNotNull(registry.activate(snapshot()))
        val old = requireNotNull(registry.beginResolution(epoch, "Receiver", TransferEndpointContract.ServiceType))
        clock.millis += TransferEndpointContract.ResolveTimeoutMillis - 1
        assertNull(registry.beginResolution(epoch, "Receiver", TransferEndpointContract.ServiceType))
        clock.millis++
        assertFalse(registry.resolved(old, service()))
        val current = requireNotNull(registry.beginResolution(epoch, "Receiver", TransferEndpointContract.ServiceType))
        assertFalse(registry.resolved(old, service()))
        assertTrue(registry.resolved(current, service()))
    }

    @Test
    fun expiringAnOldRouteDoesNotDiscardAFreshPendingResolution() {
        val clock = EndpointClock()
        val registry = TransferEndpointRegistry(clock::read)
        val snapshot = snapshot()
        val epoch = requireNotNull(registry.activate(snapshot))
        assertTrue(resolve(registry, epoch))
        clock.millis += TransferEndpointContract.FreshnessMillis - 1
        registry.refresh(epoch, snapshot)
        val pending = requireNotNull(registry.beginResolution(epoch, "Receiver", TransferEndpointContract.ServiceType))
        clock.millis += 2
        assertTrue(registry.resolved(pending, service()))
    }

    @Test
    fun backwardOrNegativeMonotonicClockClosesTheRegistry() {
        for (bad in listOf(-1L, 999L)) {
            val clock = EndpointClock()
            val registry = TransferEndpointRegistry(clock::read)
            val epoch = requireNotNull(registry.activate(snapshot()))
            clock.millis = bad
            assertNull(registry.select(epoch, Session))
            clock.millis = 10_000
            assertNull(registry.activate(snapshot()))
        }
    }

    @Test
    fun pendingResolutionsAndServiceRecordsHaveIndependentBounds() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        val tickets = (1..4).map { requireNotNull(registry.beginResolution(
            epoch, "Pending-$it", TransferEndpointContract.ServiceType
        )) }
        assertNull(registry.beginResolution(epoch, "Extra", TransferEndpointContract.ServiceType))
        assertTrue(registry.resolutionFailed(tickets.first()))
        assertNotNull(registry.beginResolution(epoch, "Extra", TransferEndpointContract.ServiceType))

        val full = TransferEndpointRegistry { 1_000 }
        val active = requireNotNull(full.activate(snapshot()))
        repeat(32) { assertTrue(resolve(full, active, service("Receiver-$it", addresses = listOf(ip(d = it + 3))))) }
        assertNull(full.beginResolution(active, "Overflow", TransferEndpointContract.ServiceType))
        assertTrue(full.serviceLost(active, "Receiver-0"))
        assertNotNull(full.beginResolution(active, "Replacement", TransferEndpointContract.ServiceType))
    }

    @Test
    fun concurrentResolutionRequestsCannotExceedThePendingLimit() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        val executor = Executors.newFixedThreadPool(8)
        val start = CountDownLatch(1)
        try {
            val requests = (1..8).map { index ->
                executor.submit<TransferResolution?> {
                    start.await()
                    registry.beginResolution(epoch, "Concurrent-$index", TransferEndpointContract.ServiceType)
                }
            }
            start.countDown()
            val accepted = requests.map { it.get(5, TimeUnit.SECONDS) }.filterNotNull()
            assertEquals(4, accepted.size)
            accepted.forEach { assertTrue(registry.resolved(it, service(it.serviceName))) }
        } finally {
            start.countDown()
            executor.shutdownNow()
        }
    }

    @Test
    fun addressesAreFilteredDeduplicatedAndBoundedPerServiceAndSelection() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot()))
        val target = requireNotNull(registry.select(epoch, Session))
        assertTrue(resolve(registry, epoch, service(addresses = listOf(
            ip(), ip(), ip(c = 43), ip(d = 2), ip(d = 255)
        ))))
        assertEquals(1, registry.candidates(target).size)
        assertFalse(resolve(registry, epoch, service(addresses = List(9) { ip() })))
        assertTrue(registry.candidates(target).isEmpty())
        for (batch in 0..2) {
            assertTrue(resolve(registry, epoch, service(
                "Batch-$batch", addresses = (0..7).map { ip(d = 10 + batch * 8 + it) }
            )))
        }
        assertEquals(8, registry.candidates(target).size)
    }

    @Test
    fun gatewayHintsUseAtMostOnePerFamilyAndRetainSenderIpv6Scope() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot(
            gateways = listOf(ip(d = 1), ip(d = 4), ipv6(), ipv6("fe800000000000000000000000000004")),
            locals = listOf(
                TransferLocalAddress(ip(d = 2), 24),
                TransferLocalAddress(ipv6("fe800000000000000000000000000002"), 64)
            )
        )))
        val target = requireNotNull(registry.select(epoch, Session))
        val endpoints = registry.candidates(target)
        assertEquals(2, endpoints.size)
        assertTrue(endpoints.all { it.port == 47739 && it.source == TransferEndpointSource.Gateway })
        assertEquals(7, endpoints.single { it.host.isIpv6 }.host.scopeId)
    }

    @Test
    fun matchingNsdHintsCannotCrowdTheCurrentGatewayOutOfTheCandidateList() {
        val registry = TransferEndpointRegistry { 1_000 }
        val epoch = requireNotNull(registry.activate(snapshot(gateways = listOf(ip(d = 1)))))
        repeat(10) { assertTrue(resolve(registry, epoch, service(
            "Receiver-$it", addresses = listOf(ip(d = it + 10))
        ))) }
        val target = requireNotNull(registry.select(epoch, Session))
        val endpoints = registry.candidates(target)
        assertEquals(8, endpoints.size)
        assertEquals(ip(d = 1), endpoints.single {
            it.source == TransferEndpointSource.Gateway
        }.host.toInetAddress())
    }
}
