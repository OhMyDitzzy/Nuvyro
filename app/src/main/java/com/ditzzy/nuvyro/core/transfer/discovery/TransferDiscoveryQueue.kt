package com.ditzzy.nuvyro.core.transfer.discovery

import com.ditzzy.nuvyro.core.transfer.endpoint.TransferEndpointContract

/** Cancelling a service invalidates its result but holds its worker slot until finally completes. */
internal class TransferDiscoveryQueue<S> {
    internal class Work<S> internal constructor(
        val name: String,
        val service: S,
        internal val identity: Any
    )

    private class Entry<S>(var service: S) {
        var work: Work<S>? = null
        var lastFinished: Long? = null
    }

    private val entries = linkedMapOf<String, Entry<S>>()
    private val running = mutableSetOf<Work<S>>()
    private var closed = false

    @Synchronized
    fun found(name: String, service: S): Boolean {
        if (closed || !TransferEndpointContract.acceptsName(name)) return false
        val existing = entries[name]
        if (existing != null) { existing.service = service; return true }
        if (entries.size >= TransferEndpointContract.MaxServices) return false
        entries[name] = Entry(service)
        return true
    }

    @Synchronized
    fun claim(now: Long): Work<S>? {
        if (closed || now < 0 || running.size >= TransferNsdBudget.MaxResolutions) return null
        val entry = entries.entries.firstOrNull { (_, entry) ->
            entry.work == null && entry.lastFinished?.let { now - it >= RetryMillis } != false
        } ?: return null
        val work = Work(entry.key, entry.value.service, entry.value)
        entry.value.work = work
        running += work
        return work
    }

    @Synchronized
    fun isCurrent(work: Work<S>): Boolean =
        !closed && work in running && entries[work.name]?.let {
            it === work.identity && it.work === work
        } == true

    @Synchronized
    fun finished(work: Work<S>, now: Long) {
        if (!running.remove(work)) return
        entries[work.name]?.takeIf { it === work.identity && it.work === work }?.let {
            it.work = null
            it.lastFinished = now
        }
    }

    @Synchronized
    fun lost(name: String): Work<S>? = entries.remove(name)?.work

    @Synchronized
    fun services(): List<S> = entries.values.map { it.service }

    @Synchronized
    fun size(): Int = entries.size

    @Synchronized
    fun close() { closed = true; entries.clear() }

    companion object {
        const val RetryMillis = 8_000L
        const val NetworkRefreshMillis = 2_000L
    }
}
