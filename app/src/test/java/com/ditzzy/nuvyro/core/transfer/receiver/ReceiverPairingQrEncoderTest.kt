package com.ditzzy.nuvyro.core.transfer.receiver

import com.google.zxing.qrcode.decoder.Decoder
import com.google.zxing.qrcode.detector.Detector
import org.junit.Assert.*
import org.junit.Test

class ReceiverPairingQrEncoderTest {
    @Test fun encoded_pairing_text_round_trips_without_modifying_caller_bytes() {
        val text = "NUVYRO:PAIR:1:" + "Ab09_-".repeat(23) + "A"
        val payload = text.toByteArray(Charsets.US_ASCII)
        val matrix = ReceiverPairingQrEncoder.encode(payload)
        try {
            assertEquals(text, Decoder().decode(Detector(matrix).detect().bits).text)
            assertArrayEquals(text.toByteArray(Charsets.US_ASCII), payload)
        } finally {
            payload.fill(0)
            matrix.clear()
        }
    }

    @Test fun matrix_has_four_white_modules_around_all_edges() {
        val matrix = ReceiverPairingQrEncoder.encode(
            ("NUVYRO:PAIR:1:" + "A".repeat(139)).toByteArray(Charsets.US_ASCII)
        )
        try {
            assertEquals(384, matrix.width)
            assertEquals(384, matrix.height)
            val rectangle = matrix.enclosingRectangle!!
            var finderWidth = 0
            while (rectangle[0] + finderWidth < matrix.width &&
                matrix[rectangle[0] + finderWidth, rectangle[1]]) finderWidth++
            assertEquals(0, finderWidth % 7)
            val scale = finderWidth / 7
            assertTrue(scale > 0)
            assertEquals(0, rectangle[2] % scale)
            assertTrue(rectangle[0] >= 4 * scale)
            assertTrue(rectangle[1] >= 4 * scale)
            assertTrue(matrix.width - rectangle[0] - rectangle[2] >= 4 * scale)
            assertTrue(matrix.height - rectangle[1] - rectangle[3] >= 4 * scale)
        } finally { matrix.clear() }
    }
}
