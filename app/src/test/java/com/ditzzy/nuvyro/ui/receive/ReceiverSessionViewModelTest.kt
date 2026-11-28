package com.ditzzy.nuvyro.ui.receive

import android.net.Network
import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverPairingQr
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSession
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionFailure
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionPhase
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionState
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.yield
import org.junit.Assert.*
import org.junit.Test

class ReceiverSessionViewModelTest {
    private class Session : ReceiverSession {
        override val state = MutableStateFlow(ReceiverSessionState(ReceiverSessionPhase.Starting))
        val started = CompletableDeferred<Unit>()
        val finish = CompletableDeferred<Unit>()
        val cleaning = CompletableDeferred<Unit>()
        val cleaned = CompletableDeferred<Unit>()
        var cleanupGate: CompletableDeferred<Unit>? = null
        var ownsReservation: (() -> Boolean)? = null
        var creates = 0
        var closes = 0
        var failInvitation = false
        val raw = ("NUVYRO:PAIR:1:" + "A".repeat(139)).toByteArray(Charsets.US_ASCII)
        var qr: ReceiverPairingQr? = null

        override suspend fun runWifi(network: Network) = error("Unexpected Wi-Fi session")
        override suspend fun runHotspot(ownsReservation: () -> Boolean) {
            this.ownsReservation = ownsReservation
            started.complete(Unit)
            try { finish.await() }
            finally {
                withContext(NonCancellable) {
                    cleaning.complete(Unit)
                    cleanupGate?.await()
                    qr?.close()
                    state.value = ReceiverSessionState(
                        ReceiverSessionPhase.Stopped, ReceiverSessionFailure.NetworkChanged
                    )
                    cleaned.complete(Unit)
                }
            }
        }
        override suspend fun createPairingQr(): ReceiverPairingQr? {
            creates++
            if (failInvitation) return null
            return ReceiverPairingQr(raw.copyOf(), 1, SecureSessionId.parse("22".repeat(16))!!,
                1, 100, 120100) { 100 }.also {
                qr = it
                state.value = state.value.copy(invitation = it)
            }
        }
        override fun close() { closes++ }
        fun listen() { state.value = ReceiverSessionState(ReceiverSessionPhase.Listening) }
    }

    @Test fun invitation_waits_for_listener_and_stop_immediately_revokes_it() = runBlocking {
        withTimeout(2000) {
            val session = Session()
            val vm = ReceiverSessionViewModel({ MutableStateFlow(null) }, { session }, this)
            vm.startHotspot { true }
            session.started.await()
            yield()
            assertEquals(0, session.creates)
            session.listen()
            val qr = vm.state.first { it.invitation != null }.invitation!!
            assertEquals(1, session.creates)
            vm.stop()
            assertTrue(qr.isCleared())
            assertNull(vm.state.value.invitation)
            assertEquals(ReceiverSessionFailure.Closed, vm.state.value.failure)
            session.cleaned.await()
            yield()
            assertEquals(ReceiverSessionFailure.Closed, vm.state.value.failure)
        }
    }

    @Test fun rapid_retries_wait_for_the_original_owner_to_finish_cleanup() = runBlocking {
        withTimeout(2000) {
            val gate = CompletableDeferred<Unit>()
            val first = Session().apply { cleanupGate = gate }
            val replacement = Session()
            var opens = 0
            val vm = ReceiverSessionViewModel({ MutableStateFlow(null) }, {
                opens++
                if (opens == 1) first else replacement
            }, this)
            vm.startHotspot { true }
            first.started.await()
            vm.restart()
            first.cleaning.await()
            vm.restart()
            vm.restart()
            yield()
            assertEquals(1, opens)
            gate.complete(Unit)
            replacement.started.await()
            assertTrue(first.cleaned.isCompleted)
            assertEquals(2, opens)
            assertEquals(ReceiverSessionPhase.Starting, vm.state.value.phase)
            vm.stop()
        }
    }

    @Test fun stop_cancels_a_replacement_waiting_for_cleanup() = runBlocking {
        withTimeout(2000) {
            val gate = CompletableDeferred<Unit>()
            val session = Session().apply { cleanupGate = gate }
            var opens = 0
            val vm = ReceiverSessionViewModel({ MutableStateFlow(null) }, { opens++; session }, this)
            vm.startHotspot { true }
            session.started.await()
            vm.restart()
            session.cleaning.await()
            vm.stop()
            gate.complete(Unit)
            session.cleaned.await()
            yield()
            assertEquals(1, opens)
            assertEquals(ReceiverSessionFailure.Closed, vm.state.value.failure)
        }
    }

    @Test fun duplicate_start_keeps_the_same_owner_and_retry_keeps_reservation_callback() = runBlocking {
        withTimeout(2000) {
            val first = Session()
            val second = Session()
            var opens = 0
            var owns = true
            val ownership = { owns }
            val vm = ReceiverSessionViewModel({ MutableStateFlow(null) }, {
                if (opens++ == 0) first else second
            }, this)
            vm.startHotspot(ownership)
            first.started.await()
            vm.startHotspot(ownership)
            yield()
            assertEquals(1, opens)
            vm.restart()
            second.started.await()
            assertSame(ownership, second.ownsReservation)
            owns = false
            assertFalse(second.ownsReservation!!())
            vm.stop()
        }
    }

    @Test fun failed_invitation_ends_the_owner_and_exposes_retry_state() = runBlocking {
        withTimeout(2000) {
            val session = Session().apply { failInvitation = true }
            val vm = ReceiverSessionViewModel({ MutableStateFlow(null) }, { session }, this)
            vm.startHotspot { true }
            session.started.await()
            session.listen()
            vm.state.first { it.failure == ReceiverSessionFailure.NativeFailure }
            assertTrue(session.cleaned.isCompleted)
            assertTrue(session.closes > 0)
            assertNull(vm.state.value.invitation)
            vm.stop()
        }
    }

    @Test fun terminal_session_state_is_published_even_when_run_returns_immediately() = runBlocking {
        withTimeout(2000) {
            val session = Session()
            val vm = ReceiverSessionViewModel({ MutableStateFlow(null) }, { session }, this)
            vm.startHotspot { true }
            session.started.await()
            session.finish.complete(Unit)
            vm.state.first { it.failure == ReceiverSessionFailure.NetworkChanged }
            assertNull(vm.state.value.invitation)
            vm.stop()
        }
    }

    @Test fun absent_wifi_does_not_open_native_session_and_monitor_failure_can_retry() = runBlocking {
        withTimeout(2000) {
            var subscriptions = 0
            var opens = 0
            val vm = ReceiverSessionViewModel({ flow {
                subscriptions++
                if (subscriptions == 1) throw SecurityException("Denied")
                emit(null)
            } }, { opens++; Session() }, this)
            vm.startWifi()
            vm.state.first { it.failure == ReceiverSessionFailure.NativeFailure }
            vm.restart()
            vm.state.first { it.failure == ReceiverSessionFailure.NetworkUnavailable }
            assertEquals(2, subscriptions)
            assertEquals(0, opens)
            vm.stop()
            vm.restart()
            yield()
            assertEquals(2, subscriptions)
        }
    }

    @Test fun unavailable_native_library_is_shown_as_a_retryable_failure() = runBlocking {
        withTimeout(2000) {
            val vm = ReceiverSessionViewModel({ MutableStateFlow(null) }, {
                throw UnsatisfiedLinkError("Unavailable")
            }, this)
            vm.startHotspot { true }
            vm.state.first { it.failure == ReceiverSessionFailure.NativeFailure }
            assertNull(vm.state.value.invitation)
            vm.stop()
        }
    }

    @Test fun expired_invitation_is_replaced_only_after_owner_removes_it() = runBlocking {
        withTimeout(2000) {
            val session = Session()
            val vm = ReceiverSessionViewModel({ MutableStateFlow(null) }, { session }, this)
            vm.startHotspot { true }
            session.started.await()
            session.listen()
            val first = vm.state.first { it.invitation != null }.invitation!!
            first.close()
            yield()
            assertEquals(1, session.creates)
            session.state.value = session.state.value.copy(invitation = null)
            val second = vm.state.first { it.invitation != null && it.invitation !== first }.invitation!!
            assertEquals(2, session.creates)
            assertFalse(second.isCleared())
            vm.stop()
        }
    }
}
