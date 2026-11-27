package com.ditzzy.nuvyro.core.transfer.endpoint

internal object TransferEndpointContract {
    const val ServiceType = "_nuvyro-xfer._tcp."
    const val Version = "1.0"
    const val Port = 47739
    const val MaxServices = 32
    const val MaxPendingResolutions = 4
    const val MaxResolvedAddresses = 8
    const val MaxAddressesPerService = 4
    const val MaxCandidates = 8
    const val FreshnessMillis = 30_000L
    const val ResolveTimeoutMillis = 8_000L

    fun acceptsType(value: String): Boolean =
        value.equals(ServiceType, ignoreCase = true) ||
            value.equals(ServiceType.dropLast(1), ignoreCase = true)

    fun acceptsName(value: String): Boolean {
        if (value.length !in 1..63 || value.any(Char::isISOControl)) return false
        val encoded = value.toByteArray(Charsets.UTF_8)
        return encoded.size <= 63 && encoded.toString(Charsets.UTF_8) == value
    }
}

internal class SecureSessionId private constructor(val hex: String) {
    override fun equals(other: Any?): Boolean = other is SecureSessionId && hex == other.hex
    override fun hashCode(): Int = hex.hashCode()
    override fun toString(): String = "SecureSessionId"

    companion object {
        fun parse(value: String): SecureSessionId? =
            if (value.length == 32 && value.all { it in '0'..'9' || it in 'a'..'f' } &&
                value.any { it != '0' }
            ) SecureSessionId(value) else null
    }
}

/** Registry-local generation, separate from native engine/session generations. */
internal class TransferNetworkEpoch internal constructor(
    val generation: Long,
    val networkHandle: Long
)

internal class TransferTarget internal constructor(
    val epoch: TransferNetworkEpoch,
    val expectedSessionId: SecureSessionId
)

internal class TransferResolution internal constructor(
    val epoch: TransferNetworkEpoch,
    val serviceName: String,
    internal val startedAt: Long
)

internal enum class TransferEndpointSource { Nsd, Gateway }

internal class TransferRoute internal constructor(
    val host: TransferHost,
    val source: TransferEndpointSource,
    val advertisedSessionId: SecureSessionId?
)

/** A routing hint for a selected QR session, never a pairing or I/O permission. */
internal class TransferEndpointCandidate internal constructor(
    val target: TransferTarget,
    internal val route: TransferRoute,
    internal val issuedAt: Long
) {
    val networkHandle get() = target.epoch.networkHandle
    val generation get() = target.epoch.generation
    val expectedSessionId get() = target.expectedSessionId
    val host get() = route.host
    val port get() = TransferEndpointContract.Port
    val source get() = route.source
}
