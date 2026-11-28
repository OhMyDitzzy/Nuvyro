package com.ditzzy.nuvyro.core.transfer.pairing

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ScannedPairingQrTest {
    @Test fun accepts_contract_payload_and_wipes_owned_text_on_close() {
        val text = validQr(payload())
        val scanned = ScannedPairingQr.parse(text)!!
        val copy = scanned.copyEncodedTextForNative()!!

        assertArrayEquals(text.toByteArray(Charsets.US_ASCII), copy)
        assertEquals("ScannedPairingQr([REDACTED])", scanned.toString())
        scanned.close()
        assertTrue(scanned.isCleared())
        assertNull(scanned.copyEncodedTextForNative())
        assertFalse(copy.isEmpty())
        copy.fill(0)
    }

    @Test fun rejects_prefix_length_alphabet_padding_and_non_canonical_encoding() {
        val valid = validQr(payload())
        assertNull(ScannedPairingQr.parse(valid.replaceFirst("NUVYRO", "nuvyro")))
        assertNull(ScannedPairingQr.parse(" $valid"))
        assertNull(ScannedPairingQr.parse(valid + "="))
        assertNull(ScannedPairingQr.parse(valid.dropLast(1) + "!"))
        assertNull(ScannedPairingQr.parse(valid.dropLast(1) + "B"))
        assertNull(ScannedPairingQr.parse("NUVYRO:PAIR:1:" + "A".repeat(600)))
    }

    @Test fun rejects_wrong_version_port_ttl_and_zero_secret_fields() {
        val good = payload()
        val invalid = listOf(
            good.copyOf().also { it[1] = 2 },
            good.copyOf().also { it[2] = 1 },
            good.copyOf().also { it[100] = 0; it[101] = 1 },
            good.copyOf().also { it[103] = 119 },
            good.copyOf().also { it.fill(0, 4, 20) },
            good.copyOf().also { it.fill(0, 20, 36) },
            good.copyOf().also { it.fill(0, 36, 68) },
            good.copyOf().also { it.fill(0, 68, 100) }
        )

        invalid.forEach { assertNull(ScannedPairingQr.parse(validQr(it))) }
        invalid.forEach { it.fill(0) }
        good.fill(0)
    }

    private fun payload(): ByteArray = ByteArray(104).also { bytes ->
        bytes[0] = 0
        bytes[1] = 1
        for (index in 4 until 100) bytes[index] = (index + 1).toByte()
        bytes[100] = 0xba.toByte()
        bytes[101] = 0x7b
        bytes[103] = 120
    }

    private fun validQr(payload: ByteArray): String {
        val alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
        val result = StringBuilder("NUVYRO:PAIR:1:")
        var accumulator = 0
        var bits = 0
        for (byte in payload) {
            accumulator = (accumulator shl 8) or (byte.toInt() and 0xff)
            bits += 8
            while (bits >= 6) {
                bits -= 6
                result.append(alphabet[(accumulator shr bits) and 63])
                accumulator = accumulator and ((1 shl bits) - 1)
            }
        }
        if (bits > 0) result.append(alphabet[(accumulator shl (6 - bits)) and 63])
        return result.toString()
    }
}
