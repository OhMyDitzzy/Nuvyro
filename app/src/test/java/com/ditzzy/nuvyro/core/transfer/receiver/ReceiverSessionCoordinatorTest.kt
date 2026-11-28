package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.NativeStatus
import com.ditzzy.nuvyro.core.transfer.NativeTransferEvent
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferLocalAddress
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferNetworkSnapshot
import java.net.InetAddress
import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test

class ReceiverSessionCoordinatorTest {
    private fun routes(last: Int = 1): ReceiverRouteSnapshot = ReceiverRouteSnapshot.wifi(
        TransferNetworkSnapshot.create(-1L, 7,
            listOf(TransferLocalAddress(InetAddress.getByAddress(byteArrayOf(192.toByte(),168.toByte(),42,last.toByte())),24)),
            emptyList())!!
    )

    private class FakePort : ReceiverNativePort {
        override val pendingEvents = MutableStateFlow<List<NativeTransferEvent>>(emptyList())
        override val failure = MutableStateFlow<Int?>(null)
        val started = CompletableDeferred<String>()
        val attached = CompletableDeferred<Unit>()
        val updated = CompletableDeferred<Unit>()
        var attachStatus = NativeStatus.Accepted
        var autoStart = true
        var closed = 0
        var sessionCloses = 0
        var attachments = 0
        var networkHandle: Long? = -1L
        var beforeSessionClose: () -> Unit = {}
        var nextId = 1L
        val session = "22".repeat(16)

        fun event(kind: String, request: String = "00".repeat(16), generation: Long = 3,
            sid: String = session, route: Long? = null) {
            pendingEvents.value = pendingEvents.value + NativeTransferEvent(kind, "%032x".format(nextId++),
                request, generation, sid, "00".repeat(16), "{}", route)
        }
        override suspend fun startLocalSession(requestId: String): NativeStatus {
            started.complete(requestId)
            if (autoStart) event("SessionStarted", requestId)
            return NativeStatus.Accepted
        }
        override suspend fun receiverRoutes(generation: Long, sessionId: String, routeGeneration: Long,
            routes: ReceiverRouteSnapshot, update: Boolean): NativeStatus {
            assertEquals(3L, generation); assertEquals(session, sessionId)
            assertEquals(networkHandle, routes.networkHandle)
            if (update) { assertEquals(2L,routeGeneration); updated.complete(Unit) }
            else { assertEquals(1L,routeGeneration); attachments++; attached.complete(Unit) }
            return attachStatus
        }
        override suspend fun closeLocalSession(generation: Long, sessionId: String): NativeStatus {
            assertEquals(3L,generation); assertEquals(session,sessionId)
            beforeSessionClose()
            sessionCloses++
            return NativeStatus.Accepted
        }
        override fun acknowledgeThrough(eventId: String): Boolean {
            pendingEvents.value = pendingEvents.value.filter { it.id > eventId }
            return true
        }
        override fun close() { closed++ }
        override suspend fun awaitClosed() { assertEquals(1,closed) }
    }

    private class Advertiser : ReceiverAdvertiser {
        val started = CompletableDeferred<ReceiverAdvertisement>()
        val stopped = CompletableDeferred<Unit>()
        val callbacks = Channel<Unit>(Channel.UNLIMITED)
        var starts = 0
        override fun advertise(binding: ReceiverAdvertisement) = flow {
            starts++
            started.complete(binding)
            try { for (callback in callbacks) emit(callback) }
            finally { stopped.complete(Unit) }
        }
    }

    @Test fun advertisement_requires_matching_listener_and_registers_only_once() = runBlocking {
        val port = FakePort(); val advertiser = Advertiser()
        val owner = ReceiverSessionCoordinator(port, advertiser)
        val job = launch { owner.run(MutableStateFlow(routes())) }
        withTimeout(2000) { port.attached.await() }
        port.event("ReceiverListening", generation=2, route=1)
        port.event("ReceiverListening", route=2)
        withTimeout(2000) { port.pendingEvents.first { it.isEmpty() } }
        assertFalse(advertiser.started.isCompleted)
        port.event("ReceiverListening", route=1)
        val binding = withTimeout(2000) { advertiser.started.await() }
        assertEquals(3L, binding.generation)
        assertEquals(port.session, binding.sessionId.hex)
        assertEquals(1L, binding.routeGeneration)
        assertEquals(-1L, binding.networkHandle)
        assertFalse(owner.state.value.advertised)
        advertiser.callbacks.send(Unit)
        withTimeout(2000) { owner.state.first { it.advertised } }
        port.event("ReceiverListening", route=1)
        withTimeout(2000) { port.pendingEvents.first { it.isEmpty() } }
        assertTrue(owner.state.value.advertised)
        assertEquals(1, advertiser.starts)
        job.cancelAndJoin()
        assertTrue(advertiser.stopped.isCompleted)
        assertFalse(owner.state.value.advertised)
    }

    @Test fun route_drift_cancels_advertisement_before_native_session_close() = runBlocking {
        val port = FakePort(); val advertiser = Advertiser()
        port.beforeSessionClose = { assertTrue(advertiser.stopped.isCompleted) }
        val owner = ReceiverSessionCoordinator(port, advertiser)
        val snapshots = MutableStateFlow<ReceiverRouteSnapshot?>(routes())
        val job = launch { owner.run(snapshots) }
        withTimeout(2000) { port.attached.await() }
        port.event("ReceiverListening", route=1)
        withTimeout(2000) { advertiser.started.await() }
        snapshots.value = routes(2)
        withTimeout(2000) { job.join() }
        assertEquals(ReceiverSessionFailure.NetworkChanged, owner.state.value.failure)
        assertEquals(1, port.sessionCloses)
        advertiser.callbacks.trySend(Unit)
        assertFalse(owner.state.value.advertised)
    }

    @Test fun failed_advertisement_closes_native_owner_without_retry() = runBlocking {
        val port = FakePort(); val advertiser = Advertiser()
        val owner = ReceiverSessionCoordinator(port, advertiser)
        val job = launch { owner.run(MutableStateFlow(routes())) }
        withTimeout(2000) { port.attached.await() }
        port.event("ReceiverListening", route=1)
        withTimeout(2000) { advertiser.started.await() }
        advertiser.callbacks.close(IOException())
        withTimeout(2000) { job.join() }
        assertEquals(ReceiverSessionFailure.AdvertisementFailed, owner.state.value.failure)
        assertEquals(1, advertiser.starts)
        assertEquals(1, port.closed)
        assertFalse(owner.state.value.advertised)
    }

    @Test fun listener_stop_in_same_batch_prevents_advertisement() = runBlocking {
        val port = FakePort(); val advertiser = Advertiser()
        val owner = ReceiverSessionCoordinator(port, advertiser)
        val job = launch { owner.run(MutableStateFlow(routes())) }
        withTimeout(2000) { port.attached.await() }
        port.event("ReceiverListening", route=1)
        port.event("ReceiverStopped", route=1)
        withTimeout(2000) { job.join() }
        assertFalse(advertiser.started.isCompleted)
        assertEquals(1, port.closed)
    }

    @Test fun hotspot_listener_uses_gateway_path_without_global_advertisement() = runBlocking {
        val port = FakePort().apply { networkHandle = null }; val advertiser = Advertiser()
        val hotspot = ReceiverRouteSnapshot.hotspot(listOf(ReceiverInterfaceSnapshot(7, true, false,
            routes().localAddresses())), emptyList())!!
        val owner = ReceiverSessionCoordinator(port, advertiser)
        val job = launch { owner.run(MutableStateFlow(hotspot)) }
        withTimeout(2000) { port.attached.await() }
        port.event("ReceiverListening", route=1)
        withTimeout(2000) { owner.state.first { it.phase == ReceiverSessionPhase.Listening } }
        assertFalse(advertiser.started.isCompleted)
        assertFalse(owner.state.value.advertised)
        job.cancelAndJoin()
    }

    @Test fun cancellation_revokes_pending_advertisement_before_native_close() = runBlocking {
        val port = FakePort(); val advertiser = Advertiser()
        port.beforeSessionClose = { assertTrue(advertiser.stopped.isCompleted) }
        val owner = ReceiverSessionCoordinator(port, advertiser)
        val job = launch { owner.run(MutableStateFlow(routes())) }
        withTimeout(2000) { port.attached.await() }
        port.event("ReceiverListening", route=1)
        withTimeout(2000) { advertiser.started.await() }
        job.cancelAndJoin()
        assertEquals(ReceiverSessionFailure.Closed, owner.state.value.failure)
        assertEquals(1, port.closed)
    }

    @Test fun listening_requires_current_listener_event() = runBlocking {
        val port = FakePort(); val owner = ReceiverSessionCoordinator(port)
        val job = launch { owner.run(MutableStateFlow(routes())) }
        withTimeout(2000) { port.attached.await() }
        assertEquals(ReceiverSessionPhase.Attaching,owner.state.value.phase)
        port.event("ReceiverListening",route=1)
        withTimeout(2000) { owner.state.first { it.phase == ReceiverSessionPhase.Listening } }
        job.cancelAndJoin()
        assertEquals(1,port.closed); assertEquals(1,port.sessionCloses)
        assertEquals(ReceiverSessionFailure.Closed,owner.state.value.failure)
    }

    @Test fun stale_generation_session_and_route_cannot_publish_listening() = runBlocking {
        val port = FakePort(); val owner = ReceiverSessionCoordinator(port)
        val job = launch { owner.run(MutableStateFlow(routes())) }
        withTimeout(2000) { port.attached.await() }
        port.event("ReceiverListening",generation=1,route=1)
        port.event("ReceiverListening",sid="33".repeat(16),route=1)
        port.event("ReceiverListening",route=2)
        withTimeout(2000) { port.pendingEvents.first { it.isEmpty() } }
        assertEquals(ReceiverSessionPhase.Attaching,owner.state.value.phase)
        port.event("ReceiverListening",route=1)
        withTimeout(2000) { owner.state.first { it.phase == ReceiverSessionPhase.Listening } }
        job.cancelAndJoin()
    }

    @Test fun ignores_start_event_for_another_request() = runBlocking {
        val port = FakePort().apply { autoStart=false }; val owner = ReceiverSessionCoordinator(port)
        val job = launch { owner.run(MutableStateFlow(routes())) }
        val request = withTimeout(2000) { port.started.await() }
        port.event("SessionStarted",request="44".repeat(16))
        withTimeout(2000) { port.pendingEvents.first { it.isEmpty() } }
        assertEquals(0,port.attachments)
        port.event("SessionStarted",request=request)
        withTimeout(2000) { port.attached.await() }
        job.cancelAndJoin()
    }

    @Test fun route_drift_revokes_native_listener_then_closes_owner() = runBlocking {
        val port = FakePort(); val owner = ReceiverSessionCoordinator(port)
        val snapshots = MutableStateFlow<ReceiverRouteSnapshot?>(routes())
        val job = launch { owner.run(snapshots) }
        withTimeout(2000) { port.attached.await() }
        snapshots.value = routes(2)
        withTimeout(2000) { job.join() }
        assertTrue(port.updated.isCompleted)
        assertEquals(ReceiverSessionFailure.NetworkChanged,owner.state.value.failure)
        assertEquals(1,port.closed); assertEquals(1,port.sessionCloses)
        port.event("ReceiverListening",route=1)
        assertEquals(ReceiverSessionPhase.Stopped,owner.state.value.phase)
    }

    @Test fun route_changes_before_session_event_prevent_attach() = runBlocking {
        val port = FakePort().apply { autoStart=false }; val owner = ReceiverSessionCoordinator(port)
        val snapshots = MutableStateFlow<ReceiverRouteSnapshot?>(routes())
        val job = launch { owner.run(snapshots) }
        withTimeout(2000) { port.started.await() }
        // Let the route collector deliver its initial snapshot before emitting a replacement.
        kotlinx.coroutines.yield(); kotlinx.coroutines.yield()
        snapshots.value = routes(2)
        withTimeout(2000) { job.join() }
        assertEquals(0,port.attachments); assertEquals(1,port.closed)
    }

    @Test fun missing_route_closes_without_listener() = runBlocking {
        val port = FakePort(); val owner = ReceiverSessionCoordinator(port)
        withTimeout(2000) { owner.run(MutableStateFlow(null)) }
        assertEquals(0,port.attachments); assertEquals(1,port.closed)
        assertEquals(ReceiverSessionFailure.NetworkUnavailable,owner.state.value.failure)
    }

    @Test fun rejected_attach_closes_session_and_engine() = runBlocking {
        val port = FakePort().apply { attachStatus=NativeStatus.InvalidState }; val owner = ReceiverSessionCoordinator(port)
        withTimeout(2000) { owner.run(MutableStateFlow(routes())) }
        assertEquals(ReceiverSessionFailure.NativeFailure,owner.state.value.failure)
        assertEquals(1,port.closed); assertEquals(1,port.sessionCloses)
    }

    @Test fun native_stop_or_pump_failure_closes_owner() = runBlocking {
        for (stopEvent in listOf(true,false)) {
            val port = FakePort(); val owner = ReceiverSessionCoordinator(port)
            val job = launch { owner.run(MutableStateFlow(routes())) }
            withTimeout(2000) { port.attached.await() }
            if (stopEvent) port.event("ReceiverStopped",route=1) else port.failure.value=7
            withTimeout(2000) { job.join() }
            assertEquals(ReceiverSessionFailure.NativeFailure,owner.state.value.failure)
            assertEquals(1,port.closed)
        }
    }

    @Test fun cancellation_before_started_event_still_destroys_native_owner() = runBlocking {
        val port = FakePort().apply { autoStart=false }; val owner = ReceiverSessionCoordinator(port)
        val job = launch { owner.run(MutableStateFlow(routes())) }
        withTimeout(2000) { port.started.await() }
        job.cancelAndJoin()
        assertEquals(1,port.closed); assertEquals(0,port.sessionCloses)
        assertEquals(ReceiverSessionFailure.Closed,owner.state.value.failure)
    }
}
