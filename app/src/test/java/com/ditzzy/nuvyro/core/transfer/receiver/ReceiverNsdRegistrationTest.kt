package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.discovery.TransferNsdBudget
import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId
import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test

class ReceiverNsdRegistrationTest {
    private fun binding() = ReceiverAdvertisement(3, SecureSessionId.parse("22".repeat(16))!!, 1, -1L)

    private class Platform : ReceiverNsdPlatform {
        val submitted = CompletableDeferred<ReceiverNsdCallbacks>()
        var stops = 0
        var throwOnRegister = false
        var throwOnStop = false
        var registerImmediately = false
        private var requested: ReceiverNsdCallbacks? = null
        override fun register(binding: ReceiverAdvertisement, callback: ReceiverNsdCallbacks) {
            requested = callback
            submitted.complete(callback)
            if (throwOnRegister) throw IllegalStateException()
            if (registerImmediately) callback.registered()
        }
        override fun unregister(callback: ReceiverNsdCallbacks) {
            assertSame(requested, callback)
            stops++
            if (throwOnStop) throw IllegalArgumentException()
        }
    }

    private class Setup {
        val platform = Platform()
        val budget = TransferNsdBudget()
        val sink = Channel<Unit>(1)
        val registration = ReceiverNsdRegistration(platform,
            ReceiverAdvertisement(3, SecureSessionId.parse("22".repeat(16))!!, 1, -1L),
            budget.advertisement()!!, sink)
    }

    @Test fun only_public_contract_fields_are_advertised() {
        val value = binding()
        assertEquals("_nuvyro-xfer._tcp.", value.serviceType)
        assertEquals(47739, value.port)
        assertEquals(mapOf("v" to "1.0", "sid" to "22".repeat(16)), value.attributes())
        assertEquals("Nuvyro-" + "22".repeat(16), value.serviceName)
        assertEquals(-1L, value.networkHandle)
        assertFalse(value.toString().contains("22".repeat(16)))
    }

    @Test fun registered_emits_once_and_stop_waits_for_terminal_callback() {
        val s = Setup()
        s.registration.submit()
        assertFalse(s.registration.isRegistered())
        s.registration.registered()
        s.registration.registered()
        assertTrue(s.sink.tryReceive().isSuccess)
        assertTrue(s.sink.tryReceive().isFailure)
        assertTrue(s.registration.isRegistered())
        s.registration.stop(); s.registration.stop()
        assertEquals(1, s.platform.stops)
        assertNull(s.budget.advertisement())
        s.registration.unregistered()
        assertNotNull(s.budget.advertisement())
        assertFalse(s.registration.isRegistered())
    }

    @Test fun cancel_before_submit_releases_without_platform_calls() {
        val s = Setup()
        s.registration.stop(); s.registration.submit()
        assertFalse(s.platform.submitted.isCompleted)
        assertEquals(0, s.platform.stops)
        assertNotNull(s.budget.advertisement())
    }

    @Test fun late_registration_after_cancel_cannot_emit_and_retries_unregister_once() {
        val s = Setup()
        s.platform.throwOnStop = true
        s.registration.submit(); s.registration.stop()
        s.registration.registered(); s.registration.registered()
        assertEquals(2, s.platform.stops)
        assertFalse(s.registration.isRegistered())
        assertTrue(s.sink.tryReceive().isFailure)
        assertNull(s.budget.advertisement())
        s.registration.unregistered()
        assertNotNull(s.budget.advertisement())
    }

    @Test fun failed_unregister_does_not_release_or_retry_without_new_evidence() {
        val s = Setup()
        s.registration.submit(); s.registration.registered()
        s.sink.tryReceive()
        s.registration.stop(); s.registration.unregistrationFailed(); s.registration.stop()
        assertEquals(1, s.platform.stops)
        assertNull(s.budget.advertisement())
        assertTrue(s.sink.tryReceive().isClosed)
        s.registration.unregistered()
        assertNotNull(s.budget.advertisement())
    }

    @Test fun duplicate_terminal_callbacks_cannot_release_a_new_owners_slot() {
        val s = Setup()
        s.registration.submit(); s.registration.registrationFailed()
        val replacement = s.budget.advertisement()!!
        s.registration.registrationFailed(); s.registration.unregistered(); s.registration.registered()
        assertNull(s.budget.advertisement())
        assertEquals(0, s.platform.stops)
        replacement.complete()
        assertNotNull(s.budget.advertisement())
    }

    @Test fun closed_sink_revokes_success_without_publishing_ready() {
        val s = Setup()
        s.registration.submit(); s.sink.close(); s.registration.registered()
        assertEquals(1, s.platform.stops)
        assertFalse(s.registration.isRegistered())
        assertNull(s.budget.advertisement())
    }

    @Test fun submission_exception_keeps_slot_until_platform_terminal_confirmation() {
        val s = Setup()
        s.platform.throwOnRegister = true
        s.registration.submit()
        assertEquals(1, s.platform.stops)
        assertTrue(s.sink.tryReceive().isClosed)
        assertNull(s.budget.advertisement())
        s.registration.registrationFailed()
        assertNotNull(s.budget.advertisement())
    }

    @Test fun startup_timeout_stops_pending_registration_and_late_success_stays_revoked() = runBlocking {
        val platform = Platform()
        val budget = TransferNsdBudget()
        val advertiser = ReceiverNsdAdvertiser({ platform }, budget, 30)
        val failed = async { runCatching { advertiser.advertise(binding()).collect() }.exceptionOrNull() }
        val callback = withTimeout(2000) { platform.submitted.await() }
        assertTrue(withTimeout(2000) { failed.await() } is IOException)
        assertEquals(1, platform.stops)
        assertNull(budget.advertisement())
        callback.registered()
        assertEquals(2, platform.stops)
        callback.unregistered()
        assertNotNull(budget.advertisement())
    }

    @Test fun registration_survives_startup_deadline_and_cancellation_unregisters() = runBlocking {
        val platform = Platform().apply { registerImmediately = true }
        val budget = TransferNsdBudget()
        val ready = CompletableDeferred<Unit>()
        val advertiser = ReceiverNsdAdvertiser({ platform }, budget, 50)
        val job = launch { advertiser.advertise(binding()).collect { ready.complete(Unit) } }
        val callback = withTimeout(2000) { platform.submitted.await() }
        callback.registered()
        withTimeout(2000) { ready.await() }
        delay(100)
        assertTrue(job.isActive)
        job.cancelAndJoin()
        assertEquals(1, platform.stops)
        assertNull(budget.advertisement())
        callback.unregistered()
        assertNotNull(budget.advertisement())
    }

    @Test fun busy_advertiser_never_submits_a_second_platform_operation() = runBlocking {
        val platform = Platform()
        val budget = TransferNsdBudget()
        val held = budget.advertisement()!!
        val advertiser = ReceiverNsdAdvertiser({ platform }, budget)
        val error = runCatching { advertiser.advertise(binding()).collect() }.exceptionOrNull()
        assertTrue(error is IOException)
        assertFalse(platform.submitted.isCompleted)
        held.complete()
    }
}
