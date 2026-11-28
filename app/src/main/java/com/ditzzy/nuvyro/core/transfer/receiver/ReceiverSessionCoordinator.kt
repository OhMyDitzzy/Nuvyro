package com.ditzzy.nuvyro.core.transfer.receiver

import android.os.SystemClock
import com.ditzzy.nuvyro.core.transfer.NativeCommandCodec
import com.ditzzy.nuvyro.core.transfer.NativeStatus
import com.ditzzy.nuvyro.core.transfer.NativeTransferEvent
import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

internal enum class ReceiverSessionPhase { Starting, Attaching, Listening, Stopped }
internal enum class ReceiverSessionFailure { NetworkUnavailable, NetworkChanged, NativeFailure, AdvertisementFailed, TimedOut, Closed }
internal data class ReceiverSessionState(
    val phase: ReceiverSessionPhase,
    val failure: ReceiverSessionFailure? = null,
    val advertised: Boolean = false,
    val invitation: ReceiverPairingQr? = null
)

/** Single-use listener and advertisement owner. Neither operation authorizes file I/O. */
internal class ReceiverSessionCoordinator(
    private val native: ReceiverNativePort,
    private val advertiser: ReceiverAdvertiser? = null,
    private val elapsedRealtime: () -> Long = SystemClock::elapsedRealtime,
    private val waitForInvitationExpiry: suspend (Long) -> Unit = { delay(it) }
) {
    private val used = AtomicBoolean(false)
    private val mutableState = MutableStateFlow(ReceiverSessionState(ReceiverSessionPhase.Starting))
    private val routeRevision = AtomicLong()
    private val invitationLock = Any()
    private val activeSignals = AtomicReference<Channel<Signal>?>(null)
    private val observedRoutes = AtomicReference<ReceiverRouteSnapshot?>(null)
    val state = mutableState.asStateFlow()

    private sealed interface Signal {
        data class Routes(val snapshot: ReceiverRouteSnapshot?, val revision: Long) : Signal
        data class Events(val events: List<NativeTransferEvent>) : Signal
        data class Advertised(val binding: ReceiverAdvertisement) : Signal
        data class AdvertisementFailed(val binding: ReceiverAdvertisement) : Signal
        data class CreateInvitation(val revision: Long, val result: CompletableDeferred<ReceiverPairingQr?>) : Signal
        data class InvitationExpired(val invitation: ReceiverPairingQr) : Signal
        data object Failed : Signal
    }
    private class Stopped(val reason: ReceiverSessionFailure) : RuntimeException()

    suspend fun createInvitation(): ReceiverPairingQr? {
        if (mutableState.value.phase != ReceiverSessionPhase.Listening) return null
        mutableState.value.invitation?.let { current ->
            if (current.isAvailable()) return current
        }
        val signals = activeSignals.get() ?: return null
        val result = CompletableDeferred<ReceiverPairingQr?>()
        if (signals.trySend(Signal.CreateInvitation(routeRevision.get(), result)).isFailure) return null
        return result.await()
    }

    suspend fun run(routes: Flow<ReceiverRouteSnapshot?>) {
        check(used.compareAndSet(false, true))
        var generation: Long? = null
        var session: String? = null
        var currentRouteGeneration: Long? = null
        try {
            coroutineScope {
                val signals = Channel<Signal>(Channel.BUFFERED, onUndeliveredElement = {
                    if (it is Signal.CreateInvitation) it.result.complete(null)
                })
                activeSignals.set(signals)
                var advertising: Job? = null
                var invitationExpiry: Job? = null
                var advertisement: ReceiverAdvertisement? = null
                val children = listOf(
                    launch {
                        var first = true
                        routes.collect {
                            val revision = synchronized(invitationLock) {
                                val previous = observedRoutes.getAndSet(it)
                                val changed = first || (it == null && previous != null) ||
                                    (it != null && previous?.sameRoutingAs(it) != true)
                                first = false
                                if (changed) {
                                    mutableState.value.invitation?.close()
                                    mutableState.update { state -> state.copy(invitation = null) }
                                    routeRevision.incrementAndGet()
                                } else null
                            }
                            if (revision != null) {
                                signals.send(Signal.Routes(it, revision))
                            }
                        }
                        val revision = synchronized(invitationLock) {
                            observedRoutes.set(null)
                            mutableState.value.invitation?.close()
                            mutableState.update { it.copy(invitation = null) }
                            routeRevision.incrementAndGet()
                        }
                        signals.send(Signal.Routes(null, revision))
                    },
                    launch { native.pendingEvents.collect { signals.send(Signal.Events(it)) } },
                    launch { native.failure.collect { if (it != null) signals.send(Signal.Failed) } }
                )
                try {
                    val request = NativeCommandCodec.requestId()
                    accepted(native.startLocalSession(request))
                    var snapshot: ReceiverRouteSnapshot? = null
                    var attached = false
                    currentRouteGeneration = 1L
                    var lastEvent = NativeCommandCodec.ZERO_ID
                    suspend fun consume() {
                        when (val signal = signals.receive()) {
                            Signal.Failed -> throw Stopped(ReceiverSessionFailure.NativeFailure)
                            is Signal.Advertised -> if (signal.binding === advertisement) {
                                mutableState.update { it.copy(advertised = true) }
                            }
                            is Signal.AdvertisementFailed -> if (signal.binding === advertisement) {
                                throw Stopped(ReceiverSessionFailure.AdvertisementFailed)
                            }
                            is Signal.CreateInvitation -> try {
                                val currentSnapshot = snapshot
                                if (signal.revision != routeRevision.get() ||
                                    mutableState.value.phase != ReceiverSessionPhase.Listening ||
                                    generation == null || session == null || currentSnapshot == null ||
                                    observedRoutes.get()?.sameRoutingAs(currentSnapshot) != true
                                ) {
                                    signal.result.complete(null)
                                } else if (mutableState.value.invitation?.isAvailable() == true) {
                                    signal.result.complete(mutableState.value.invitation)
                                } else {
                                    val route = currentRouteGeneration ?: 1L
                                    val createdAt = elapsedRealtime()
                                    val raw = try {
                                        native.createInvitation(generation!!, session!!, route)
                                    } catch (error: CancellationException) {
                                        signal.result.cancel(error)
                                        throw error
                                    } catch (error: LinkageError) {
                                        signal.result.complete(null)
                                        throw Stopped(ReceiverSessionFailure.NativeFailure)
                                    } catch (_: Exception) {
                                        signal.result.complete(null)
                                        throw Stopped(ReceiverSessionFailure.NativeFailure)
                                    }
                                    val deadline = createdAt + ReceiverPairingQr.TTL_MILLIS
                                    val invitation = try {
                                        ReceiverPairingQr(raw, generation!!,
                                            SecureSessionId.parse(session!!)!!, route, createdAt, deadline, elapsedRealtime)
                                    } catch (_: RuntimeException) {
                                        raw.fill(0)
                                        runCatching { native.clearInvitation(generation!!, session!!, route) }
                                        signal.result.complete(null)
                                        throw Stopped(ReceiverSessionFailure.NativeFailure)
                                    }
                                    val published = synchronized(invitationLock) {
                                        if (signal.revision == routeRevision.get() &&
                                            mutableState.value.phase == ReceiverSessionPhase.Listening &&
                                            invitation.isAvailable()
                                        ) {
                                            mutableState.value.invitation?.close()
                                            mutableState.update { it.copy(invitation = invitation) }
                                            true
                                        } else false
                                    }
                                    if (!published) {
                                        invitation.close()
                                        native.clearInvitation(generation!!, session!!, route)
                                        signal.result.complete(null)
                                    } else {
                                        invitationExpiry?.cancel()
                                        invitationExpiry = launch {
                                            waitForInvitationExpiry((deadline - elapsedRealtime()).coerceAtLeast(0))
                                            signals.send(Signal.InvitationExpired(invitation))
                                        }
                                        signal.result.complete(invitation)
                                    }
                                }
                            } finally {
                                signal.result.complete(null)
                            }
                            is Signal.InvitationExpired -> if (mutableState.value.invitation === signal.invitation) {
                                val invitation = signal.invitation
                                invitation.close()
                                mutableState.update { it.copy(invitation = null) }
                                if (generation == invitation.generation && session == invitation.sessionId.hex) {
                                    native.clearInvitation(generation!!, session!!, invitation.routeGeneration)
                                }
                            }
                            is Signal.Routes -> {
                                if (signal.revision != routeRevision.get()) return
                                val next = signal.snapshot ?: throw Stopped(ReceiverSessionFailure.NetworkUnavailable)
                                if (snapshot?.sameRoutingAs(next) == false) {
                                    mutableState.value.invitation?.close()
                                    mutableState.update { it.copy(invitation = null, advertised = false) }
                                    invitationExpiry?.cancel()
                                    if (generation != null && session != null) {
                                        native.clearInvitation(generation!!, session!!, currentRouteGeneration ?: 1L)
                                    }
                                    if (attached) {
                                        // Native drift revokes the old listener; a new identity requires a new owner.
                                        val nextRouteGeneration = Math.addExact(currentRouteGeneration ?: 1L, 1L)
                                        currentRouteGeneration = nextRouteGeneration
                                        native.receiverRoutes(generation!!, session!!, nextRouteGeneration, next, true)
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
                                            "ReceiverListening" -> if (attached && event.routeGeneration == currentRouteGeneration) {
                                                mutableState.update { it.copy(phase = ReceiverSessionPhase.Listening) }
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
                            val routeGeneration = checkNotNull(currentRouteGeneration)
                            accepted(native.receiverRoutes(generation, session!!, routeGeneration, snapshot, false))
                            attached = true
                        }
                        val handle = snapshot?.networkHandle
                        if (mutableState.value.phase == ReceiverSessionPhase.Listening && advertisement == null &&
                            advertiser != null && handle != null) {
                            val routeGeneration = checkNotNull(currentRouteGeneration)
                            val binding = ReceiverAdvertisement(generation!!, SecureSessionId.parse(session!!)!!,
                                routeGeneration, handle)
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
                    invitationExpiry?.cancel()
                    mutableState.value.invitation?.close()
                    mutableState.update { it.copy(invitation = null, advertised = false) }
                    val cleanupGeneration = generation
                    val cleanupSession = session
                    val cleanupRouteGeneration = currentRouteGeneration
                    if (cleanupGeneration != null && cleanupSession != null && cleanupRouteGeneration != null) {
                        runCatching { native.clearInvitation(cleanupGeneration, cleanupSession, cleanupRouteGeneration) }
                    }
                    while (true) {
                        val pending = signals.tryReceive().getOrNull() ?: break
                        if (pending is Signal.CreateInvitation) pending.result.complete(null)
                    }
                    activeSignals.compareAndSet(signals, null)
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
            mutableState.value.invitation?.close()
            mutableState.update { it.copy(invitation = null, advertised = false) }
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
