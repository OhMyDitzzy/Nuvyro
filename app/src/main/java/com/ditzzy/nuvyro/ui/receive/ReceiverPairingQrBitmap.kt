package com.ditzzy.nuvyro.ui.receive

import android.graphics.Bitmap
import android.graphics.Color
import androidx.core.graphics.createBitmap
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverPairingQrEncoder

internal class ReceiverPairingQrBitmap : AutoCloseable {
    private var bitmap: Bitmap? = null

    fun render(payload: ByteArray): Bitmap {
        check(bitmap == null)
        val matrix = ReceiverPairingQrEncoder.encode(payload)
        var created: Bitmap? = null
        var pixels: IntArray? = null
        try {
            val size = matrix.width
            val colors = IntArray(size * size)
            pixels = colors
            colors.indices.forEach { index ->
                colors[index] = if (matrix[index % size, index / size]) Color.BLACK else Color.WHITE
            }
            val result = createBitmap(size, size)
            created = result
            result.setPixels(colors, 0, size, 0, 0, size, size)
            bitmap = result
            created = null
            return result
        } finally {
            pixels?.fill(0)
            matrix.clear()
            created?.recycle()
        }
    }

    fun erase() { bitmap?.eraseColor(Color.WHITE) }

    override fun close() {
        erase()
        bitmap?.recycle()
        bitmap = null
    }
}
