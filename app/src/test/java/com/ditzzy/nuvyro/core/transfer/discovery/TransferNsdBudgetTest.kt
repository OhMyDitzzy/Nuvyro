package com.ditzzy.nuvyro.core.transfer.discovery

import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TransferNsdBudgetTest {
    @Test
    fun onlyOneTransferDiscoveryCanBeRegisteredAtATime() {
        val budget = TransferNsdBudget()
        val lease = requireNotNull(budget.discovery())
        assertNull(budget.discovery())
        lease.complete()
        assertNotNull(budget.discovery())
    }

    @Test
    fun fourOutstandingPlatformResolutionsExhaustTheSharedCapacity() {
        val budget = TransferNsdBudget()
        val active = List(4) { requireNotNull(budget.resolution()) }
        assertFalse(budget.resolutionCapacityAvailable())
        assertNull(budget.resolution())
        active[0].complete()
        assertTrue(budget.resolutionCapacityAvailable())
        assertNotNull(budget.resolution())
        assertNull(budget.resolution())
    }

    @Test
    fun duplicateTerminalCallbacksCannotReleaseAnotherOperationsSlot() {
        val budget = TransferNsdBudget()
        val first = requireNotNull(budget.resolution())
        repeat(3) { assertNotNull(budget.resolution()) }
        first.complete()
        val replacement = requireNotNull(budget.resolution())
        first.complete()
        first.complete()
        assertNull(budget.resolution())
        replacement.complete()
        assertNotNull(budget.resolution())
    }

    @Test
    fun resolutionAndDiscoveryAccountingAreIndependent() {
        val budget = TransferNsdBudget()
        val discovery = requireNotNull(budget.discovery())
        val resolutions = List(4) { requireNotNull(budget.resolution()) }
        discovery.complete()
        assertNotNull(budget.discovery())
        assertNull(budget.resolution())
        resolutions.forEach { it.complete() }
        assertNull(budget.discovery())
        assertTrue(budget.resolutionCapacityAvailable())
    }

    @Test
    fun concurrentReservationCannotExceedFourPlatformRequests() {
        val budget = TransferNsdBudget()
        val start = CountDownLatch(1)
        val executor = Executors.newFixedThreadPool(8)
        try {
            val futures = (1..8).map {
                executor.submit<TransferNsdBudget.Lease?> { start.await(); budget.resolution() }
            }
            start.countDown()
            val leases = futures.map { it.get(5, TimeUnit.SECONDS) }.filterNotNull()
            assertEquals(4, leases.size)
            assertNull(budget.resolution())
            leases.forEach { it.complete() }
            assertTrue(budget.resolutionCapacityAvailable())
        } finally {
            start.countDown()
            executor.shutdownNow()
        }
    }
}
