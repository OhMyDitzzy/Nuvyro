package com.ditzzy.nuvyro.core.transfer.discovery

import java.util.concurrent.atomic.AtomicBoolean

/** Slots represent platform operations, including cancelled requests awaiting a terminal callback. */
internal class TransferNsdBudget {
    internal class Lease(private val release: () -> Unit) {
        private val completed = AtomicBoolean(false)
        fun complete() {
            if (completed.compareAndSet(false, true)) release()
        }
    }

    private var discoveries = 0
    private var resolutions = 0

    @Synchronized
    fun discovery(): Lease? {
        if (discoveries == 1) return null
        discoveries++
        return Lease { synchronized(this) { discoveries-- } }
    }

    @Synchronized
    fun resolution(): Lease? {
        if (resolutions == MaxResolutions) return null
        resolutions++
        return Lease { synchronized(this) { resolutions-- } }
    }

    @Synchronized
    fun resolutionCapacityAvailable(): Boolean = resolutions < MaxResolutions

    companion object {
        const val MaxResolutions = 4
        val Shared = TransferNsdBudget()
    }
}
