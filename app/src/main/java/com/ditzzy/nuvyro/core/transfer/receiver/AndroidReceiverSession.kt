package com.ditzzy.nuvyro.core.transfer.receiver

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import com.ditzzy.nuvyro.core.lan.LanPermission
import com.ditzzy.nuvyro.core.transfer.NativeEngineOpenResult
import com.ditzzy.nuvyro.core.transfer.NativeTransferEngine
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Explicit single-use receive owner. Creating it performs no native or network operations. */
internal class AndroidReceiverSession private constructor(private val monitor: AndroidReceiverRouteMonitor) : AutoCloseable {
    private val used = AtomicBoolean(false)
    private val closed = AtomicBoolean(false)
    private val running = AtomicReference<Job?>(null)
    private val mutableState = MutableStateFlow(ReceiverSessionState(ReceiverSessionPhase.Starting))
    val state = mutableState.asStateFlow()

    suspend fun runWifi(network: Network) = run(monitor.wifi(network))

    /** The caller retains the reservation and must revoke this callback when it is closed. */
    suspend fun runHotspot(ownsReservation: () -> Boolean) = run(monitor.hotspot(ownsReservation))

    private suspend fun run(routes: Flow<ReceiverRouteSnapshot?>) = withContext(Dispatchers.IO) {
        check(used.compareAndSet(false, true))
        coroutineScope {
            val job = currentCoroutineContext()[Job]!!
            running.set(job)
            try {
                if (closed.get()) {
                    mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Stopped, ReceiverSessionFailure.Closed)
                    return@coroutineScope
                }
                when (val result = NativeTransferEngine.open()) {
                    is NativeEngineOpenResult.Ready -> {
                        val coordinator = ReceiverSessionCoordinator(result.engine)
                        val observer = launch { coordinator.state.collect { mutableState.value = it } }
                        try { coordinator.run(routes) }
                        finally { observer.cancel(); mutableState.value = coordinator.state.value }
                    }
                    else -> mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Stopped, ReceiverSessionFailure.NativeFailure)
                }
            } catch (error: CancellationException) {
                mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Stopped, ReceiverSessionFailure.Closed)
                throw error
            } finally {
                closed.set(true)
                running.compareAndSet(job, null)
            }
        }
    }

    override fun close() {
        closed.set(true)
        running.get()?.cancel()
    }

    companion object {
        fun create(context: Context): AndroidReceiverSession {
            val app = context.applicationContext
            return AndroidReceiverSession(AndroidReceiverRouteMonitor(
                app.getSystemService(ConnectivityManager::class.java), { LanPermission.isGranted(app) }
            ))
        }
    }
}
