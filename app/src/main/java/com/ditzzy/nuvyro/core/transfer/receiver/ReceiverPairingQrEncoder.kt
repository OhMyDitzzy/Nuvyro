package com.ditzzy.nuvyro.core.transfer.receiver

import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.common.BitMatrix
import com.google.zxing.qrcode.QRCodeWriter

internal object ReceiverPairingQrEncoder {
    fun encode(payload: ByteArray): BitMatrix = QRCodeWriter().encode(
        String(payload, Charsets.US_ASCII), BarcodeFormat.QR_CODE, 384, 384,
        mapOf(EncodeHintType.MARGIN to 4, EncodeHintType.CHARACTER_SET to "US-ASCII")
    )
}
