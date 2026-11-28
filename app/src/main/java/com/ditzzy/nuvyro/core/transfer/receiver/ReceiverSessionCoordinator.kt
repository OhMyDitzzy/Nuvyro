package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.NativeCommandCodec
import com.ditzzy.nuvyro.core.transfer.NativeStatus
import com.ditzzy.nuvyro.core.transfer.NativeTransferEvent
import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

internal enum class ReceiverSessionPhase { Starting, Attaching, Listening, Stopped }
internal enum class ReceiverSessionFailure { NetworkUnavailable, NetworkChanged, NativeFailure, AdvertisementFailed, TimedOut, Closed }
internal data class ReceiverSessionState(
    val phase: ReceiverSessionPhase,
    val failure: ReceiverSessionFailure? = null,
    val advertised: Boolean = false
)

/** Single-use listener and advertisement owner. Neither operation authorizes file I/O. */
internal class ReceiverSessionCoordinator(
    private val native: ReceiverNativePort,
    private val advertiser: ReceiverAdvertiser? = null
) {
    private val used = AtomicBoolean(false)
    private val mutableState = MutableStateFlow(ReceiverSessionState(ReceiverSessionPhase.Starting))
    val state = mutableState.asStateFlow()

    private sealed interface Signal {
        data class Routes(val snapshot: ReceiverRouteSnapshot?) : Signal
        data class Events(val events: List<NativeTransferEvent>) : Signal
        data class Advertised(val binding: ReceiverAdvertisement) : Signal
        data class AdvertisementFailed(val binding: ReceiverAdvertisement) : Signal
        data object Failed : Signal
    }
    private class Stopped(val reason: ReceiverSessionFailure) : RuntimeException()

    suspend fun run(routes: Flow<ReceiverRouteSnapshot?>) {
        check(used.compareAndSet(false, true))
        var generation: Long? = null
        var session: String? = null
        try {
            coroutineScope {
                val signals = Channel<Signal>(Channel.RENDEZVOUS)
                var advertising: Job? = null
                var advertisement: ReceiverAdvertisement? = null
                val children = listOf(
                    launch { routes.collect { signals.send(Signal.Routes(it)) }; signals.send(Signal.Routes(null)) },
                    launch { native.pendingEvents.collect { signals.send(Signal.Events(it)) } },
                    launch { native.failure.collect { if (it != null) signals.send(Signal.Failed) } }
                )
                try {
                    val request = NativeCommandCodec.requestId()
                    accepted(native.startLocalSession(request))
                    var snapshot: ReceiverRouteSnapshot? = null
                    var attached = false
                    var lastEvent = NativeCommandCodec.ZERO_ID
                    suspend fun consume() {
                        when (val signal = signals.receive()) {
                            Signal.Failed -> throw Stopped(ReceiverSessionFailure.NativeFailure)
                            is Signal.Advertised -> if (signal.binding === advertisement) {
                                mutableState.value = mutableState.value.copy(advertised = true)
                            }
                            is Signal.AdvertisementFailed -> if (signal.binding === advertisement) {
                                throw Stopped(ReceiverSessionFailure.AdvertisementFailed)
                            }
                            is Signal.Routes -> {
                                val next = signal.snapshot ?: throw Stopped(ReceiverSessionFailure.NetworkUnavailable)
                                if (snapshot?.sameRoutingAs(next) == false) {
                                    if (attached) {
                                        // Native drift revokes the old listener; a new identity requires a new owner.
                                        native.receiverRoutes(generation!!, session!!, 2, next, true)
                                    }
                                    throw Stopped(ReceiverSessionFailure.NetworkChanged)
                                }
                                snapshot = next
                            }
                            is Signal.Events -> {
                                for (event in signal.events) {
                                    if (event.id <= lastEvent) continue
                                    lastEvent = event.id
                                    if (event.kind == "SessionStarted" && event.requestId == request && generation == null) {
                                        generation = event.generation
                                        session = event.sessionId
                                    } else if (event.generation == generation && event.sessionId == session) {
                                        when (event.kind) {
                                            "ReceiverListening" -> if (attached && event.routeGeneration == 1L) {
                                                mutableState.value = mutableState.value.copy(phase = ReceiverSessionPhase.Listening)
                                            }
                                            "ReceiverStopped", "SessionClosed" -> throw Stopped(ReceiverSessionFailure.NativeFailure)
                                        }
                                    }
                                }
                                signal.events.lastOrNull()?.let { native.acknowledgeThrough(it.id) }
                            }
                        }
                        if (!attached && generation != null && snapshot != null) {
                            mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Attaching)
                            accepted(native.receiverRoutes(generation, session!!, 1, snapshot, false))
                            attached = true
                        }
                        val handle = snapshot?.networkHandle
                        if (mutableState.value.phase == ReceiverSessionPhase.Listening && advertisement == null &&
                            advertiser != null && handle != null) {
                            val binding = ReceiverAdvertisement(generation!!, SecureSessionId.parse(session!!)!!, 1, handle)
                            advertisement = binding
                            advertising = launch {
                                try {
                                    advertiser.advertise(binding).collect { signals.send(Signal.Advertised(binding)) }
                                    signals.send(Signal.AdvertisementFailed(binding))
                                } catch (error: CancellationException) {
                                    throw error
                                } catch (_: Exception) {
                                    signals.send(Signal.AdvertisementFailed(binding))
                                }
                            }
                        }
                    }
                    val listening = withTimeoutOrNull(10_000) {
                        while (mutableState.value.phase != ReceiverSessionPhase.Listening) consume()
                        true
                    }
                    if (listening == null) throw Stopped(ReceiverSessionFailure.TimedOut)
                    while (true) consume()
                } finally {
                    children.forEach { it.cancel() }
                    withContext(NonCancellable) { advertising?.cancelAndJoin() }
                    signals.cancel()
                }
            }
        } catch (error: CancellationException) {
            mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Stopped, ReceiverSessionFailure.Closed)
            throw error
        } catch (error: Stopped) {
            mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Stopped, error.reason)
        } catch (_: LinkageError) {
            mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Stopped, ReceiverSessionFailure.NativeFailure)
        } catch (_: Exception) {
            mutableState.value = ReceiverSessionState(ReceiverSessionPhase.Stopped, ReceiverSessionFailure.NativeFailure)
        } finally {
            withContext(NonCancellable) {
                try {
                    if (generation != null && session != null) native.closeLocalSession(generation, session)
                } finally {
                    native.close()
                    native.awaitClosed()
                }
            }
        }
    }

    private fun accepted(status: NativeStatus) {
        if (status != NativeStatus.Accepted) throw Stopped(ReceiverSessionFailure.NativeFailure)
    }
}
