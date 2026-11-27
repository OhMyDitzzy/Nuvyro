package com.ditzzy.nuvyro.core.transfer.discovery

import com.ditzzy.nuvyro.core.transfer.endpoint.TransferNetworkEpoch
import java.io.IOException

internal enum class TransferDiscoveryFailure {
    Closed, AlreadyUsed, PermissionRequired, NetworkUnavailable, NetworkChanged,
    NsdUnavailable, NsdBusy, NsdStartFailed, NsdStopped, NsdStopFailed, EventOverflow
}

internal class TransferDiscoveryException(val reason: TransferDiscoveryFailure) :
    IOException(reason.name)

/** Conflated routing status; it says nothing about TLS, pairing, or receiver readiness. */
internal data class TransferDiscoveryUpdate(
    val epoch: TransferNetworkEpoch,
    val nsdActive: Boolean,
    val nsdFailure: TransferDiscoveryFailure?,
    val serviceCount: Int,
    val resolutionCapacityAvailable: Boolean
)
