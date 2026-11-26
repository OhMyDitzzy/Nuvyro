package com.ditzzy.nuvyro.core.hotspot

import com.ditzzy.nuvyro.data.hotspot.HotspotCredentials
import com.ditzzy.nuvyro.data.hotspot.HotspotSecurity
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.common.BitMatrix
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel

internal object HotspotQrCode {
    fun encode(credentials: HotspotCredentials): BitMatrix = QRCodeWriter().encode(
        payload(credentials), BarcodeFormat.QR_CODE, 0, 0,
        mapOf(
            EncodeHintType.CHARACTER_SET to "UTF-8",
            EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M,
            EncodeHintType.MARGIN to 4
        )
    )

    fun payload(credentials: HotspotCredentials): String {
        val type = if (credentials.security == HotspotSecurity.Wpa3) "SAE" else "WPA"
        return "WIFI:T:$type;S:${escape(credentials.ssid)};P:${escape(credentials.password)};;"
    }

    private fun escape(value: String): String = buildString {
        value.forEach { character ->
            if (character in "\\;,:\"") append('\\')
            append(character)
        }
    }
}
