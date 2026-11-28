package com.ditzzy.nuvyro.ui.send.receiver

import android.os.SystemClock
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import com.ditzzy.nuvyro.core.transfer.pairing.ScannedPairingQr
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.ChecksumException
import com.google.zxing.DecodeHintType
import com.google.zxing.FormatException
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.LuminanceSource
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean

internal class PairingQrAnalyzer(
    private val onScanned: (ScannedPairingQr) -> Unit,
    private val onInvalidQr: () -> Unit
) : ImageAnalysis.Analyzer {
    private val reader = MultiFormatReader().apply {
        setHints(mapOf(
            DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
            DecodeHintType.TRY_HARDER to true
        ))
    }
    private val delivered = AtomicBoolean(false)
    private var lastInvalidAt = 0L

    override fun analyze(image: ImageProxy) {
        try {
            if (delivered.get() || image.width <= 0 || image.height <= 0 ||
                image.width.toLong() * image.height > MAX_IMAGE_PIXELS ||
                image.planes.isEmpty() ||
                image.imageInfo.rotationDegrees % 90 != 0
            ) return

            val source = ImagePlaneLuminanceSource(image, image.planes[0])
            val result = reader.decodeWithState(BinaryBitmap(HybridBinarizer(source)))
            val scanned = ScannedPairingQr.parse(result.text)
            if (scanned == null) {
                notifyInvalidQr()
            } else if (delivered.compareAndSet(false, true)) {
                onScanned(scanned)
            } else {
                scanned.close()
            }
        } catch (_: NotFoundException) {
            // Most camera frames do not contain a QR code.
        } catch (_: ChecksumException) {
            // Keep scanning when a frame contains an unreadable code.
        } catch (_: FormatException) {
            // Keep scanning when a frame contains an unreadable code.
        } catch (_: IllegalArgumentException) {
            // Ignore malformed camera frames without stopping the analyzer.
        } catch (_: IndexOutOfBoundsException) {
            // Ignore malformed camera frames without stopping the analyzer.
        } finally {
            reader.reset()
            image.close()
        }
    }

    private fun notifyInvalidQr() {
        val now = SystemClock.elapsedRealtime()
        if (now - lastInvalidAt >= INVALID_MESSAGE_INTERVAL_MS) {
            lastInvalidAt = now
            onInvalidQr()
        }
    }

    private class ImagePlaneLuminanceSource(image: ImageProxy, plane: ImageProxy.PlaneProxy) : LuminanceSource(
        if (image.imageInfo.rotationDegrees % 180 == 0) image.width else image.height,
        if (image.imageInfo.rotationDegrees % 180 == 0) image.height else image.width
    ) {
        private val buffer: ByteBuffer = plane.buffer.duplicate()
        private val bufferStart = buffer.position()
        private val rowStride = plane.rowStride
        private val pixelStride = plane.pixelStride
        private val sourceWidth = image.width
        private val sourceHeight = image.height
        private val rotation = ((image.imageInfo.rotationDegrees % 360) + 360) % 360

        init {
            val lastOffset = bufferStart.toLong() +
                (sourceHeight - 1L) * rowStride + (sourceWidth - 1L) * pixelStride
            require(rotation in setOf(0, 90, 180, 270) && lastOffset < buffer.limit())
        }

        override fun getRow(y: Int, row: ByteArray?): ByteArray {
            require(y in 0 until height)
            val output = row?.takeIf { it.size >= width } ?: ByteArray(width)
            for (x in 0 until width) output[x] = sample(x, y).toByte()
            return output
        }

        override fun getMatrix(): ByteArray = ByteArray(width * height).also { output ->
            var index = 0
            for (y in 0 until height) {
                for (x in 0 until width) output[index++] = sample(x, y).toByte()
            }
        }

        private fun sample(x: Int, y: Int): Int {
            val sourceX: Int
            val sourceY: Int
            when (rotation) {
                90 -> { sourceX = y; sourceY = sourceHeight - 1 - x }
                180 -> { sourceX = sourceWidth - 1 - x; sourceY = sourceHeight - 1 - y }
                270 -> { sourceX = sourceWidth - 1 - y; sourceY = x }
                else -> { sourceX = x; sourceY = y }
            }
            val offset = bufferStart + sourceY * rowStride + sourceX * pixelStride
            return buffer.get(offset).toInt() and 0xff
        }
    }

    private companion object {
        const val MAX_IMAGE_PIXELS = 1_200_000L
        const val INVALID_MESSAGE_INTERVAL_MS = 1_500L
    }
}
