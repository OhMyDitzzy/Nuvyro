package com.ditzzy.nuvyro.core.transfer

import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

internal data class NativeTransferEvent(
    val kind: String,
    val id: String,
    val requestId: String,
    val generation: Long,
    val sessionId: String,
    val transferId: String,
    val dataJson: String,
    val routeGeneration: Long? = null
) {
    override fun toString(): String = "NativeTransferEvent(kind=$kind, id=$id)"
}

internal object NativeEventCodec {
    private val fields = setOf("abi", "kind", "event_id", "request_id", "generation", "session_id", "transfer_id", "data")
    private val idPattern = Regex("[0-9a-f]{32}")
    private val generationPattern = Regex("0|[1-9][0-9]{0,18}")
    private val kinds = setOf(
        "SessionStarted", "ReceiverListening", "ReceiverStopped", "InvitationCreated", "Paired", "OfferReceived", "AwaitingSource", "AwaitingDestination",
        "FileVerified", "DestinationCommitRequired", "Progress", "FileCommitted", "TransferCompleted", "Declined",
        "Cancelled", "DeliveryUnconfirmed", "SessionClosed", "Failed"
    )

    fun decode(bytes: ByteArray): List<NativeTransferEvent> {
        require(bytes.size <= 65_536)
        val text = Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()
        val tokener = JSONTokener(text)
        val array = tokener.nextValue() as? JSONArray ?: error("Invalid native event batch")
        require(tokener.nextClean() == '\u0000' && array.length() <= 32)
        return List(array.length()) { index ->
            val item = array.getJSONObject(index)
            require(item.keys().asSequence().toSet() == fields && item.get("abi") == NativeLibrary.ABI_VERSION)
            require(item.toString().toByteArray(Charsets.UTF_8).size <= 16_384)
            val kind = item.strictString("kind")
            require(kind in kinds)
            val id = item.identifier("event_id", allowZero = false)
            val generationText = item.strictString("generation")
            require(generationText.matches(generationPattern))
            val generation = generationText.toLongOrNull() ?: error("Invalid native generation")
            val sessionId = item.identifier("session_id")
            val data = item.getJSONObject("data")
            if (kind in setOf("ReceiverListening", "ReceiverStopped")) {
                require(data.keys().asSequence().toSet() == setOf("transport_ready", "route_generation"))
                val route = data.strictString("route_generation")
                require(route.matches(generationPattern) && (route.toLongOrNull() ?: 0) > 0)
            }
            if (kind in setOf("SessionStarted", "SessionClosed", "ReceiverListening", "ReceiverStopped")) {
                require(generation > 0 && sessionId != NativeCommandCodec.ZERO_ID)
                require(data.get("transport_ready") == false)
            }
            NativeTransferEvent(kind, id, item.identifier("request_id"), generation, sessionId,
                item.identifier("transfer_id"), data.toString(),
                if (kind in setOf("ReceiverListening", "ReceiverStopped")) data.strictString("route_generation").toLong() else null)
        }
    }

    private fun JSONObject.strictString(name: String): String = get(name) as? String ?: error("Invalid native field")
    private fun JSONObject.identifier(name: String, allowZero: Boolean = true): String = strictString(name).also {
        require(it.matches(idPattern) && (allowZero || it != NativeCommandCodec.ZERO_ID))
    }
}
