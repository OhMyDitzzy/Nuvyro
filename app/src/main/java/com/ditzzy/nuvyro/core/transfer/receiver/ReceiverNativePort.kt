package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.NativeStatus
import com.ditzzy.nuvyro.core.transfer.NativeTransferEvent
import kotlinx.coroutines.flow.StateFlow

/** Owned exclusively by one receive coordinator, including its event acknowledgements. */
internal interface ReceiverNativePort : AutoCloseable {
    val pendingEvents: StateFlow<List<NativeTransferEvent>>
    val failure: StateFlow<Int?>
    suspend fun startLocalSession(requestId: String): NativeStatus
    suspend fun receiverRoutes(generation: Long, sessionId: String, routeGeneration: Long,
        routes: ReceiverRouteSnapshot, update: Boolean): NativeStatus
    suspend fun createInvitation(generation: Long, sessionId: String, routeGeneration: Long): ByteArray
    suspend fun clearInvitation(generation: Long, sessionId: String, routeGeneration: Long): NativeStatus
    suspend fun closeLocalSession(generation: Long, sessionId: String): NativeStatus
    fun acknowledgeThrough(eventId: String): Boolean
    suspend fun awaitClosed()
}
