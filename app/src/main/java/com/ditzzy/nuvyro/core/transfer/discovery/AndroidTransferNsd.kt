package com.ditzzy.nuvyro.core.transfer.discovery

import android.net.Network
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferEndpointContract
import java.lang.ref.WeakReference
import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull

internal sealed interface TransferNsdEvent {
    data object Started : TransferNsdEvent
    class Found(val service: NsdServiceInfo) : TransferNsdEvent
    class Lost(val service: NsdServiceInfo) : TransferNsdEvent
}

internal sealed interface TransferNsdResult {
    class Resolved(val service: NsdServiceInfo) : TransferNsdResult
    data object Failed : TransferNsdResult
    data object CapacityUnavailable : TransferNsdResult
}

@Suppress("DEPRECATION")
internal class AndroidTransferNsd(
    private val manager: NsdManager,
    private val budget: TransferNsdBudget = TransferNsdBudget.Shared
) {
    fun discover(network: Network) = callbackFlow<TransferNsdEvent> {
        val lease = budget.discovery()
            ?: throw TransferDiscoveryException(TransferDiscoveryFailure.NsdBusy)
        val listener = DiscoveryListener(manager, WeakReference(channel), lease)
        val startup = launch {
            delay(10_000)
            if (!listener.started.get()) close(
                TransferDiscoveryException(TransferDiscoveryFailure.NsdStartFailed)
            )
        }
        try {
            listener.submit(network)
            awaitClose { }
        } finally {
            startup.cancel()
            listener.stop()
        }
    }.buffer(64)

    suspend fun resolve(service: NsdServiceInfo): TransferNsdResult {
        val lease = budget.resolution() ?: return TransferNsdResult.CapacityUnavailable
        val request = AtomicReference<ResolutionListener?>(null)
        return try {
            withTimeoutOrNull(TransferEndpointContract.ResolveTimeoutMillis) {
                suspendCancellableCoroutine<TransferNsdResult> { continuation ->
                    val listener = ResolutionListener(manager, WeakReference(continuation), lease)
                    request.set(listener)
                    continuation.invokeOnCancellation { listener.stop() }
                    listener.submit(service)
                }
            } ?: TransferNsdResult.Failed
        } finally {
            val listener = request.get()
            if (listener == null) lease.complete() else listener.stop()
        }
    }

    private class DiscoveryListener(
        private val manager: NsdManager,
        private val sink: WeakReference<SendChannel<TransferNsdEvent>>,
        private val lease: TransferNsdBudget.Lease
    ) : NsdManager.DiscoveryListener {
        val started = AtomicBoolean(false)
        private var submitted = false
        private var terminal = false
        private var stopRequested = false
        private var stopSent = false

        @Synchronized
        fun submit(network: Network) {
            if (stopRequested) { terminal = true; lease.complete(); return }
            submitted = true
            try {
                if (Build.VERSION.SDK_INT >= 33) {
                    manager.discoverServices(
                        TransferEndpointContract.ServiceType, NsdManager.PROTOCOL_DNS_SD,
                        network, Executor { it.run() }, this
                    )
                } else {
                    manager.discoverServices(
                        TransferEndpointContract.ServiceType, NsdManager.PROTOCOL_DNS_SD, this
                    )
                }
            } catch (_: RuntimeException) {
                fail(TransferDiscoveryFailure.NsdStartFailed)
                stop()
            }
        }

        @Synchronized
        fun stop() {
            stopRequested = true
            if (!submitted) { terminal = true; lease.complete(); return }
            if (terminal || stopSent) return
            stopSent = true
            try { manager.stopServiceDiscovery(this) }
            catch (_: RuntimeException) { fail(TransferDiscoveryFailure.NsdStopFailed) }
            // Only a terminal callback releases the slot after submission.
        }

        @Synchronized
        override fun onDiscoveryStarted(type: String) {
            started.set(true)
            if (terminal) return
            if (stopRequested || sink.get() == null) {
                stopSent = false
                stop()
            } else emit(TransferNsdEvent.Started)
        }

        @Synchronized
        override fun onServiceFound(service: NsdServiceInfo) {
            if (!terminal && !stopRequested) emit(TransferNsdEvent.Found(service))
        }

        @Synchronized
        override fun onServiceLost(service: NsdServiceInfo) {
            if (!terminal && !stopRequested) emit(TransferNsdEvent.Lost(service))
        }

        @Synchronized
        override fun onDiscoveryStopped(type: String) {
            terminal = true
            lease.complete()
            fail(TransferDiscoveryFailure.NsdStopped)
        }

        @Synchronized
        override fun onStartDiscoveryFailed(type: String, error: Int) {
            terminal = true
            lease.complete()
            fail(TransferDiscoveryFailure.NsdStartFailed)
        }

        @Synchronized
        override fun onStopDiscoveryFailed(type: String, error: Int) {
            fail(TransferDiscoveryFailure.NsdStopFailed)
        }

        private fun emit(event: TransferNsdEvent) {
            val channel = sink.get()
            if (channel == null || !channel.trySend(event).isSuccess) {
                fail(TransferDiscoveryFailure.EventOverflow)
                stop()
            }
        }

        private fun fail(reason: TransferDiscoveryFailure) {
            sink.get()?.close(TransferDiscoveryException(reason))
        }
    }

    private class ResolutionListener(
        private val manager: NsdManager,
        private val waiter: WeakReference<CancellableContinuation<TransferNsdResult>>,
        private val lease: TransferNsdBudget.Lease
    ) : NsdManager.ResolveListener {
        private var submitted = false
        private var stopped = false
        private var terminal = false
        private val delivered = AtomicBoolean(false)

        @Synchronized
        fun submit(service: NsdServiceInfo) {
            if (stopped || waiter.get()?.isActive != true) {
                terminal = true
                lease.complete()
                return
            }
            submitted = true
            try {
                if (Build.VERSION.SDK_INT >= 33) {
                    manager.resolveService(service, Executor { it.run() }, this)
                } else manager.resolveService(service, this)
            } catch (_: RuntimeException) {
                deliver(TransferNsdResult.Failed)
                stop()
            }
        }

        @Synchronized
        fun stop() {
            if (stopped) return
            stopped = true
            if (!submitted) { terminal = true; lease.complete(); return }
            if (!terminal && Build.VERSION.SDK_INT >= 34) {
                try { manager.stopServiceResolution(this) }
                catch (_: RuntimeException) { }
            }
        }

        @Synchronized
        override fun onServiceResolved(service: NsdServiceInfo) {
            complete(TransferNsdResult.Resolved(service))
        }

        @Synchronized
        override fun onResolveFailed(service: NsdServiceInfo, error: Int) {
            complete(TransferNsdResult.Failed)
        }

        @Synchronized
        override fun onResolutionStopped(service: NsdServiceInfo) {
            complete(TransferNsdResult.Failed)
        }

        override fun onStopResolutionFailed(service: NsdServiceInfo, error: Int) {
            // Failure to stop is not evidence that the original operation has ended.
        }

        private fun complete(result: TransferNsdResult) {
            if (terminal) return
            terminal = true
            lease.complete()
            deliver(result)
        }

        private fun deliver(result: TransferNsdResult) {
            if (delivered.compareAndSet(false, true)) {
                waiter.get()?.takeIf { it.isActive }?.resume(result)
            }
        }
    }
}
