package com.ditzzy.nuvyro.core.transfer

import java.util.UUID
import org.json.JSONObject

internal object NativeCommandCodec {
    const val ZERO_ID = "00000000000000000000000000000000"

    fun startSession(): ByteArray = encode("startReceiveSession", 0, ZERO_ID)

    fun closeSession(generation: Long, sessionId: String): ByteArray {
        require(generation > 0 && sessionId.matches(Regex("[0-9a-f]{32}")) && sessionId != ZERO_ID)
        return encode("closeSession", generation, sessionId)
    }

    private fun encode(kind: String, generation: Long, sessionId: String): ByteArray = JSONObject()
        .put("abi", NativeLibrary.ABI_VERSION)
        .put("kind", kind)
        .put("request_id", UUID.randomUUID().toString().replace("-", ""))
        .put("generation", generation.toString())
        .put("session_id", sessionId)
        .put("transfer_id", ZERO_ID)
        .put("data", JSONObject())
        .toString().toByteArray(Charsets.UTF_8)
}
