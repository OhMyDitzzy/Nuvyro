package com.ditzzy.nuvyro.core.transfer.receiver

import android.net.Network
import kotlinx.coroutines.flow.StateFlow

internal interface ReceiverSession : AutoCloseable {
    val state: StateFlow<ReceiverSessionState>
    suspend fun createPairingQr(): ReceiverPairingQr?
    suspend fun runWifi(network: Network)
    suspend fun runHotspot(ownsReservation: () -> Boolean)
}
