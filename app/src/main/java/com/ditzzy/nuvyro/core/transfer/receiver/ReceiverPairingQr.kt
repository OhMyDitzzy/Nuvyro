package com.ditzzy.nuvyro.core.transfer.receiver

import android.os.SystemClock
import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId

/** One session-scoped QR payload. Callers that copy bytes own wiping their copy. */
internal class ReceiverPairingQr internal constructor(
    private var payload: ByteArray,
    val generation: Long,
    val sessionId: SecureSessionId,
    val routeGeneration: Long,
    val createdAtElapsedRealtime: Long,
    val expiresAtElapsedRealtime: Long,
    private val elapsedRealtime: () -> Long = SystemClock::elapsedRealtime
) : AutoCloseable {
    private val lock = Any()

    init {
        require(generation > 0 && routeGeneration > 0)
        require(createdAtElapsedRealtime >= 0 && expiresAtElapsedRealtime > createdAtElapsedRealtime)
        require(payload.size == TEXT_BYTES)
        require(payload.copyOfRange(0, PREFIX.size).contentEquals(PREFIX))
        require((PREFIX.size until payload.size).all { index ->
            val byte = payload[index]
            val value = byte.toInt() and 0xff
            value in 'A'.code..'Z'.code || value in 'a'.code..'z'.code ||
                value in '0'.code..'9'.code || value == '-'.code || value == '_'.code
        })
    }

    fun copyForRendering(): ByteArray? = synchronized(lock) {
        val nowElapsedRealtime = elapsedRealtime()
        if (nowElapsedRealtime < createdAtElapsedRealtime || nowElapsedRealtime >= expiresAtElapsedRealtime) {
            clearLocked()
            null
        } else {
            payload.takeIf { it.isNotEmpty() }?.copyOf()
        }
    }

    fun isAvailable(): Boolean = synchronized(lock) {
        val nowElapsedRealtime = elapsedRealtime()
        if (nowElapsedRealtime < createdAtElapsedRealtime || nowElapsedRealtime >= expiresAtElapsedRealtime) {
            clearLocked()
            false
        } else payload.isNotEmpty()
    }

    fun isCleared(): Boolean = synchronized(lock) { payload.isEmpty() }

    override fun close() {
        synchronized(lock) { clearLocked() }
    }

    private fun clearLocked() {
        payload.fill(0)
        payload = ByteArray(0)
    }

    override fun toString(): String = "ReceiverPairingQr([REDACTED])"

    internal companion object {
        const val TTL_MILLIS = 120_000L
        val PREFIX = "NUVYRO:PAIR:1:".toByteArray(Charsets.US_ASCII)
        private const val ENCODED_PAYLOAD_BYTES = 139
        private val TEXT_BYTES = PREFIX.size + ENCODED_PAYLOAD_BYTES
    }
}
