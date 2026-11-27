package com.ditzzy.nuvyro.core.transfer.discovery

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class TransferDiscoveryQueueTest {
    @Test
    fun foundServicesAreBoundedAndDuplicateFoundDoesNotGrowTheQueue() {
        val queue = TransferDiscoveryQueue<String>()
        repeat(32) { assertTrue(queue.found("Receiver-$it", "value-$it")) }
        assertFalse(queue.found("Overflow", "value"))
        assertTrue(queue.found("Receiver-0", "updated"))
        assertEquals(32, queue.size())
        assertEquals("updated", queue.claim(0)?.service)
    }

    @Test
    fun invalidInstanceNamesAreRejectedBeforeTheyAreRetained() {
        val queue = TransferDiscoveryQueue<String>()
        for (name in listOf("", "x".repeat(64), "bad\nname", "bad\uD800")) {
            assertFalse(queue.found(name, "ignored"))
        }
        assertEquals(0, queue.size())
    }

    @Test
    fun atMostFourLogicalWorkersCanBeClaimed() {
        val queue = TransferDiscoveryQueue<String>()
        repeat(5) { queue.found("Receiver-$it", "value") }
        val running = List(4) { requireNotNull(queue.claim(0)) }
        assertNull(queue.claim(0))
        queue.finished(running[0], 1)
        assertEquals("Receiver-4", queue.claim(1)?.name)
        assertNull(queue.claim(1))
    }

    @Test
    fun lostWorkerKeepsItsSlotUntilItsCompletionRuns() {
        val queue = TransferDiscoveryQueue<String>()
        repeat(5) { queue.found("Receiver-$it", "value") }
        val running = List(4) { requireNotNull(queue.claim(0)) }
        assertSame(running[0], queue.lost(running[0].name))
        assertFalse(queue.isCurrent(running[0]))
        assertNull(queue.claim(10))
        queue.finished(running[0], 10)
        assertNotNull(queue.claim(10))
    }

    @Test
    fun lostAndReappearingInstanceRejectsTheOldWorkEvenWithTheSameName() {
        val queue = TransferDiscoveryQueue<String>()
        queue.found("Receiver", "old")
        val old = requireNotNull(queue.claim(0))
        queue.lost("Receiver")
        queue.found("Receiver", "new")
        val current = requireNotNull(queue.claim(1))
        assertFalse(queue.isCurrent(old))
        assertTrue(queue.isCurrent(current))
        queue.finished(old, 2)
        assertTrue(queue.isCurrent(current))
        assertNull(queue.claim(20_000))
        queue.finished(current, 3)
        assertEquals("new", queue.claim(8_003)?.service)
    }

    @Test
    fun duplicateCompletionCannotReleaseAReplacementWorker() {
        val queue = TransferDiscoveryQueue<String>()
        queue.found("Receiver", "value")
        val old = requireNotNull(queue.claim(0))
        queue.finished(old, 1)
        val current = requireNotNull(queue.claim(8_001))
        queue.finished(old, 8_002)
        assertTrue(queue.isCurrent(current))
        assertNull(queue.claim(30_000))
    }

    @Test
    fun refreshDelayStartsAtCompletionAndOtherServicesCanProceed() {
        val queue = TransferDiscoveryQueue<String>()
        queue.found("First", "one")
        queue.found("Second", "two")
        val first = requireNotNull(queue.claim(0))
        queue.finished(first, 10_000)
        assertEquals("Second", queue.claim(10_001)?.name)
        assertNull(queue.claim(17_999))
        assertEquals("First", queue.claim(18_000)?.name)
    }

    @Test
    fun duplicateFoundUpdatesFutureInputsWithoutMutatingTheRunningWork() {
        val queue = TransferDiscoveryQueue<String>()
        queue.found("Receiver", "old")
        val work = requireNotNull(queue.claim(0))
        queue.found("Receiver", "new")
        assertEquals("old", work.service)
        assertTrue(queue.isCurrent(work))
        queue.finished(work, 1)
        assertEquals("new", queue.claim(8_001)?.service)
    }

    @Test
    fun closingRejectsLateFoundClaimAndResultsButAcceptsWorkerCleanup() {
        val queue = TransferDiscoveryQueue<String>()
        queue.found("Receiver", "value")
        val work = requireNotNull(queue.claim(0))
        queue.close()
        queue.close()
        assertFalse(queue.isCurrent(work))
        assertFalse(queue.found("Late", "ignored"))
        assertNull(queue.claim(30_000))
        assertTrue(queue.services().isEmpty())
        queue.finished(work, 30_000)
        assertFalse(queue.isCurrent(work))
    }

    @Test
    fun logicalTimeoutAndRestartDoNotFreeUnfinishedPlatformOperations() {
        val budget = TransferNsdBudget()
        val first = TransferDiscoveryQueue<String>()
        repeat(4) { first.found("Receiver-$it", "value") }
        val work = List(4) { requireNotNull(first.claim(0)) }
        val physical = List(4) { requireNotNull(budget.resolution()) }
        work.forEach { first.finished(it, 8_000) }
        first.close()
        val restarted = TransferDiscoveryQueue<String>()
        restarted.found("New receiver", "new")
        assertNotNull(restarted.claim(10_000))
        assertNull(budget.resolution())
        physical[0].complete()
        assertNotNull(budget.resolution())
        assertNull(budget.resolution())
    }
}
