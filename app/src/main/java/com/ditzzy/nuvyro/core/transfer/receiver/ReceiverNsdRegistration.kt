package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.discovery.TransferNsdBudget
import java.io.IOException
import java.lang.ref.WeakReference
import kotlinx.coroutines.channels.SendChannel

internal interface ReceiverNsdCallbacks {
    fun registered()
    fun registrationFailed()
    fun unregistered()
    fun unregistrationFailed()
}

internal interface ReceiverNsdPlatform {
    fun register(binding: ReceiverAdvertisement, callback: ReceiverNsdCallbacks)
    fun unregister(callback: ReceiverNsdCallbacks)
}

/** Retains the platform slot until a terminal callback, even after the owning coroutine ends. */
internal class ReceiverNsdRegistration(
    private val platform: ReceiverNsdPlatform,
    private val binding: ReceiverAdvertisement,
    private val lease: TransferNsdBudget.Lease,
    sink: SendChannel<Unit>
) : ReceiverNsdCallbacks {
    private val sink = WeakReference(sink)
    private var submitted = false
    private var terminal = false
    private var registered = false
    private var stopped = false
    private var stopSent = false

    @Synchronized
    fun isRegistered(): Boolean = registered && !stopped && !terminal

    @Synchronized
    fun submit() {
        if (stopped || terminal) return
        check(!submitted)
        submitted = true
        try { platform.register(binding, this) }
        catch (_: RuntimeException) { fail(); stop() }
    }

    @Synchronized
    fun stop() {
        stopped = true
        if (!submitted) { terminal = true; lease.complete(); return }
        if (terminal || stopSent) return
        stopSent = true
        try { platform.unregister(this) }
        catch (_: RuntimeException) { fail() }
    }

    @Synchronized
    override fun registered() {
        if (terminal || registered) return
        registered = true
        if (stopped || sink.get() == null) {
            // A pending unregister may have raced registration; retry once on confirmed success.
            stopSent = false
            stop()
        } else if (sink.get()?.trySend(Unit)?.isSuccess != true) {
            fail()
            stop()
        }
    }

    @Synchronized
    override fun registrationFailed() {
        if (terminal) return
        terminal = true
        lease.complete()
        fail()
    }

    @Synchronized
    override fun unregistered() {
        if (terminal) return
        terminal = true
        lease.complete()
        fail()
    }

    @Synchronized
    override fun unregistrationFailed() {
        if (!terminal) fail()
    }

    private fun fail() { sink.get()?.close(IOException("Transfer advertisement stopped")) }
}
