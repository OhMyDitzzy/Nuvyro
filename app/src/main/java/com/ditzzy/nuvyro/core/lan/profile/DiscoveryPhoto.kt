package com.ditzzy.nuvyro.core.lan.profile

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.ByteArrayOutputStream
import java.io.IOException

internal object DiscoveryPhoto {
    fun encode(photo: Bitmap?): ByteArray {
        if (photo == null) return byteArrayOf()
        val thumbnail = Bitmap.createScaledBitmap(photo, 128, 128, true)
        val output = ByteArrayOutputStream()
        if (!thumbnail.compress(Bitmap.CompressFormat.JPEG, 65, output)) throw IOException("Could not encode avatar")
        return output.toByteArray().takeIf { it.size <= ProfileProtocol.MaxPhotoBytes } ?: byteArrayOf()
    }

    fun decode(bytes: ByteArray): Bitmap? {
        if (bytes.isEmpty()) return null
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth !in 1..256 || bounds.outHeight !in 1..256) return null
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    }
}
