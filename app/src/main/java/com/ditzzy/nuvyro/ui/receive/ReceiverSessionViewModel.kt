package com.ditzzy.nuvyro.ui.receive

import android.net.Network
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSession
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionFailure
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionPhase
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal class ReceiverSessionViewModel(
    private val networks: () -> Flow<Network?>,
    private val createSession: () -> ReceiverSession,
    scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
) : ViewModel(scope) {
    private enum class Mode { Wifi, Hotspot }

    private val mutableState = MutableStateFlow(ReceiverSessionState(ReceiverSessionPhase.Starting))
    val state = mutableState.asStateFlow()

    private var mode: Mode? = null
    private var hotspotOwnership: (() -> Boolean)? = null
    private var sessionJob: Job? = null
    private var activeSession: ReceiverSession? = null
    private var generation = 0L

    fun startWifi() = startWifi(force = false)

    private fun startWifi(force: Boolean) {
        if (!force && mode == Mode.Wifi && sessionJob?.isActive == true) return
        start(Mode.Wifi, null) { attempt ->
            networks().collectLatest { network ->
                if (network == null) {
                    publish(attempt, ReceiverSessionState(
                        ReceiverSessionPhase.Stopped, ReceiverSessionFailure.NetworkUnavailable
                    ))
                } else runSession(attempt) { runWifi(network) }
            }
        }
    }

    fun startHotspot(ownsReservation: () -> Boolean) = startHotspot(ownsReservation, force = false)

    private fun startHotspot(ownsReservation: () -> Boolean, force: Boolean) {
        if (!force && mode == Mode.Hotspot && sessionJob?.isActive == true) return
        start(Mode.Hotspot, ownsReservation) { attempt ->
            runSession(attempt) { runHotspot(ownsReservation) }
        }
    }

    fun restart() {
        when (mode) {
            Mode.Wifi -> startWifi(force = true)
            Mode.Hotspot -> hotspotOwnership?.let { startHotspot(it, force = true) }
            null -> Unit
        }
    }

    private fun start(nextMode: Mode, ownership: (() -> Boolean)?, run: suspend (Long) -> Unit) {
        val previous = sessionJob
        stop()
        val attempt = generation
        mode = nextMode
        hotspotOwnership = ownership
        mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Starting)
        sessionJob = viewModelScope.launch(start = CoroutineStart.UNDISPATCHED) {
            // Keep the cleanup chain intact even when retries cancel a waiting replacement.
            withContext(NonCancellable) { previous?.join() }
            currentCoroutineContext().ensureActive()
            try {
                run(attempt)
            } catch (error: CancellationException) {
                throw error
            } catch (_: LinkageError) {
                failed(attempt)
            } catch (_: Exception) {
                failed(attempt)
            }
        }
    }

    private suspend fun runSession(attempt: Long, run: suspend ReceiverSession.() -> Unit) = coroutineScope {
        val session = createSession()
        activeSession = session
        val observer = launch {
            session.state.collect { current ->
                publish(attempt, current)
                if (current.phase == ReceiverSessionPhase.Listening && current.invitation == null) {
                    val invitation = session.createPairingQr()
                    if (invitation == null && session.state.value.phase == ReceiverSessionPhase.Listening) {
                        error("Pairing invitation unavailable")
                    }
                }
            }
        }
        try {
            run(session)
            currentCoroutineContext().ensureActive()
            publish(attempt, session.state.value)
        } finally {
            observer.cancel()
            session.close()
            session.state.value.invitation?.close()
            if (activeSession === session) activeSession = null
        }
    }

    private fun publish(attempt: Long, next: ReceiverSessionState) {
        if (generation == attempt) mutableState.value = next
    }

    private fun failed(attempt: Long) = publish(attempt, ReceiverSessionState(
        ReceiverSessionPhase.Stopped, ReceiverSessionFailure.NativeFailure
    ))

    fun stop() {
        generation++
        sessionJob?.cancel()
        mutableState.value.invitation?.close()
        activeSession?.close()
        activeSession = null
        mode = null
        hotspotOwnership = null
        mutableState.value = ReceiverSessionState(
            ReceiverSessionPhase.Stopped, ReceiverSessionFailure.Closed
        )
    }

    override fun onCleared() {
        stop()
        super.onCleared()
    }
}
