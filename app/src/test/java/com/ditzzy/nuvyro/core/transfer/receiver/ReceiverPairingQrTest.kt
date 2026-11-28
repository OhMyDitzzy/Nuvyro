package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId
import org.junit.Assert.*
import org.junit.Test

class ReceiverPairingQrTest {
    private val session = SecureSessionId.parse("22".repeat(16))!!
    private fun payload() = ("NUVYRO:PAIR:1:" + "A".repeat(139)).toByteArray(Charsets.US_ASCII)

    @Test fun closing_wipes_owned_bytes_and_preserves_redaction() {
        val raw = payload()
        val qr = ReceiverPairingQr(raw, 1, session, 1, 100, 120100) { 100 }
        val renderingCopy = qr.copyForRendering()!!
        assertEquals("ReceiverPairingQr([REDACTED])", qr.toString())
        qr.close()
        qr.close()
        assertTrue(raw.all { it == 0.toByte() })
        assertNull(qr.copyForRendering())
        assertFalse(qr.isAvailable())
        assertTrue(renderingCopy.contentEquals(payload()))
        renderingCopy.fill(0)
    }

    @Test fun expired_or_backward_clock_clears_payload_permanently() {
        for (invalidTime in listOf(99L, 120100L)) {
            var now = 100L
            val raw = payload()
            val qr = ReceiverPairingQr(raw, 1, session, 1, 100, 120100) { now }
            assertTrue(qr.isAvailable())
            now = invalidTime
            assertNull(qr.copyForRendering())
            assertTrue(raw.all { it == 0.toByte() })
            now = 100
            assertFalse(qr.isAvailable())
        }
    }

    @Test fun rejects_unbounded_or_non_pairing_text() {
        for (raw in listOf(ByteArray(513), payload().dropLast(1).toByteArray(),
            payload().also { it[0] = 'X'.code.toByte() },
            payload().also { it[it.lastIndex] = '='.code.toByte() })) {
            assertThrows(IllegalArgumentException::class.java) {
                ReceiverPairingQr(raw, 1, session, 1, 100, 120100) { 100 }
            }
            raw.fill(0)
        }
    }
}
