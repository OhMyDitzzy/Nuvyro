package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId
import com.ditzzy.nuvyro.core.transfer.endpoint.TransferEndpointContract
import kotlinx.coroutines.flow.Flow

/** Public routing hints for one listener lifetime; no invitation or authentication material. */
internal class ReceiverAdvertisement(
    val generation: Long,
    val sessionId: SecureSessionId,
    val routeGeneration: Long,
    val networkHandle: Long
) {
    init { require(generation > 0 && routeGeneration > 0 && networkHandle != 0L) }

    val serviceName get() = "Nuvyro-${sessionId.hex}"
    val serviceType get() = TransferEndpointContract.ServiceType
    val port get() = TransferEndpointContract.Port
    fun attributes(): Map<String, String> = mapOf("v" to TransferEndpointContract.Version, "sid" to sessionId.hex)

    override fun toString(): String = "ReceiverAdvertisement"
}

internal fun interface ReceiverAdvertiser {
    /** Emits once on registration and remains active until cancellation or platform failure. */
    fun advertise(binding: ReceiverAdvertisement): Flow<Unit>
}
