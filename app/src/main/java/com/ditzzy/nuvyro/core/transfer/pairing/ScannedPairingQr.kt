package com.ditzzy.nuvyro.core.transfer.pairing

/** A validated, short-lived invitation scan. Callers own and must wipe native copies. */
class ScannedPairingQr private constructor(private var encodedText: ByteArray) : AutoCloseable {
    private val lock = Any()

    fun copyEncodedTextForNative(): ByteArray? = synchronized(lock) {
        encodedText.takeIf { it.isNotEmpty() }?.copyOf()
    }

    fun isCleared(): Boolean = synchronized(lock) { encodedText.isEmpty() }

    override fun close() = synchronized(lock) {
        encodedText.fill(0)
        encodedText = ByteArray(0)
    }

    override fun toString(): String = "ScannedPairingQr([REDACTED])"

    companion object {
        private const val MAX_TEXT_BYTES = 512
        private const val PREFIX = "NUVYRO:PAIR:1:"
        private const val ENCODED_PAYLOAD_CHARS = 139
        private const val PAYLOAD_BYTES = 104
        private const val TRANSFER_PORT = 47739

        /** Returns null for every unsupported or malformed QR without retaining its text. */
        fun parse(text: String): ScannedPairingQr? {
            if (text.length > MAX_TEXT_BYTES || text.length != PREFIX.length + ENCODED_PAYLOAD_CHARS) {
                return null
            }
            if (!text.startsWith(PREFIX) || text.any { it.code > 0x7f }) return null

            val encoded = text.substring(PREFIX.length)
            val decoded = decodeCanonicalBase64Url(encoded) ?: return null
            try {
                if (!isValidPayload(decoded)) return null
                return ScannedPairingQr(text.toByteArray(Charsets.US_ASCII))
            } finally {
                decoded.fill(0)
            }
        }

        private fun decodeCanonicalBase64Url(encoded: String): ByteArray? {
            val output = ByteArray(PAYLOAD_BYTES)
            var outputIndex = 0
            var accumulator = 0
            var bitCount = 0

            for (character in encoded) {
                val value = when (character) {
                    in 'A'..'Z' -> character.code - 'A'.code
                    in 'a'..'z' -> character.code - 'a'.code + 26
                    in '0'..'9' -> character.code - '0'.code + 52
                    '-' -> 62
                    '_' -> 63
                    else -> {
                        output.fill(0)
                        return null
                    }
                }
                accumulator = (accumulator shl 6) or value
                bitCount += 6
                if (bitCount >= 8) {
                    bitCount -= 8
                    if (outputIndex >= output.size) {
                        output.fill(0)
                        return null
                    }
                    output[outputIndex++] = (accumulator shr bitCount).toByte()
                    accumulator = accumulator and ((1 shl bitCount) - 1)
                }
            }

            if (outputIndex != PAYLOAD_BYTES || bitCount != 2 || accumulator != 0) {
                output.fill(0)
                return null
            }
            return output
        }

        private fun isValidPayload(payload: ByteArray): Boolean {
            if (payload.size != PAYLOAD_BYTES) return false
            if (unsignedShort(payload, 0) != 1 || unsignedShort(payload, 2) != 0) return false
            if (unsignedShort(payload, 100) != TRANSFER_PORT || unsignedShort(payload, 102) != 120) return false
            return listOf(4..19, 20..35, 36..67, 68..99).all { range ->
                range.any { payload[it].toInt() != 0 }
            }
        }

        private fun unsignedShort(bytes: ByteArray, offset: Int): Int =
            ((bytes[offset].toInt() and 0xff) shl 8) or (bytes[offset + 1].toInt() and 0xff)
    }
}
