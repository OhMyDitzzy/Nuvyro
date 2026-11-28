package com.ditzzy.nuvyro.core.transfer

import java.util.UUID

internal object NativeCommandCodec {
    const val ZERO_ID = "00000000000000000000000000000000"

    fun startSession(requestId: String = requestId()): ByteArray = encode("startReceiveSession", 0, ZERO_ID, requestId = requestId)

    fun closeSession(generation: Long, sessionId: String): ByteArray {
        require(generation > 0 && sessionId.matches(Regex("[0-9a-f]{32}")) && sessionId != ZERO_ID)
        return encode("closeSession", generation, sessionId)
    }

    fun requestId(): String = UUID.randomUUID().toString().replace("-", "")

    internal fun encode(kind: String, generation: Long, sessionId: String,
        data: String = "{}", requestId: String = requestId()): ByteArray {
        require(kind in setOf("startReceiveSession", "closeSession", "attachReceiver", "updateReceiverRoutes"))
        require(generation >= 0 && sessionId.matches(Regex("[0-9a-f]{32}")))
        require(requestId.matches(Regex("[0-9a-f]{32}")) && requestId != ZERO_ID)
        // All envelope strings are validated identifiers or fixed command names.
        return ("""{"abi":${NativeLibrary.ABI_VERSION},"kind":"$kind","request_id":"$requestId","generation":"$generation","session_id":"$sessionId","transfer_id":"$ZERO_ID","data":$data}""")
            .toByteArray(Charsets.UTF_8).also { require(it.size <= 65_536) }
    }
}
