package com.ditzzy.nuvyro.core.profile

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.ExifInterface
import android.net.Uri
import java.io.File
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class ProfilePhotoStore(context: Context) {
    private val resolver = context.applicationContext.contentResolver
    private val directory = File(context.applicationContext.filesDir, "profile_photos")

    suspend fun prepare(uri: Uri): Bitmap = withContext(Dispatchers.IO) {
        require(uri.scheme == "content")
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        resolver.openInputStream(uri).use { BitmapFactory.decodeStream(it, null, bounds) }
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) throw IOException("Invalid photo")

        val options = BitmapFactory.Options().apply {
            inSampleSize = 1
            while (maxOf(bounds.outWidth, bounds.outHeight) / inSampleSize > 1024) {
                inSampleSize *= 2
            }
        }
        val decoded = resolver.openInputStream(uri).use {
            BitmapFactory.decodeStream(it, null, options)
        } ?: throw IOException("Could not read photo")
        val orientation = try {
            resolver.openInputStream(uri)?.use {
                ExifInterface(it).getAttributeInt(
                    ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL
                )
            } ?: ExifInterface.ORIENTATION_NORMAL
        } catch (_: IOException) {
            ExifInterface.ORIENTATION_NORMAL
        }
        val oriented = orient(decoded, orientation)
        val side = minOf(oriented.width, oriented.height)
        val cropped = Bitmap.createBitmap(
            oriented, (oriented.width - side) / 2, (oriented.height - side) / 2, side, side
        )
        Bitmap.createScaledBitmap(cropped, minOf(side, 512), minOf(side, 512), true)
    }

    suspend fun load(fileName: String?): Bitmap? = withContext(Dispatchers.IO) {
        val file = photoFile(fileName) ?: return@withContext null
        if (!file.isFile) return@withContext null
        BitmapFactory.decodeFile(file.path)
    }

    suspend fun save(bitmap: Bitmap): String = withContext(Dispatchers.IO) {
        if (!directory.isDirectory && !directory.mkdirs()) {
            throw IOException("Could not create photo directory")
        }
        val file = File(directory, "${UUID.randomUUID()}.jpg")
        try {
            file.outputStream().use {
                if (!bitmap.compress(Bitmap.CompressFormat.JPEG, 85, it)) {
                    throw IOException("Could not save photo")
                }
            }
        } catch (error: Exception) {
            file.delete()
            throw error
        }
        file.name
    }

    suspend fun removeUnused(keep: String?) = withContext(Dispatchers.IO) {
        directory.listFiles()?.forEach { file ->
            if (file.name != keep && photoFile(file.name) != null) file.delete()
        }
        Unit
    }

    private fun photoFile(name: String?): File? = name
        ?.takeIf { it.matches(Regex("[a-f0-9-]{36}\\.jpg")) }
        ?.let { File(directory, it) }

    private fun orient(bitmap: Bitmap, orientation: Int): Bitmap {
        val matrix = Matrix().apply {
            when (orientation) {
                ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> setScale(-1f, 1f)
                ExifInterface.ORIENTATION_ROTATE_180 -> setRotate(180f)
                ExifInterface.ORIENTATION_FLIP_VERTICAL -> setScale(1f, -1f)
                ExifInterface.ORIENTATION_TRANSPOSE -> {
                    setRotate(90f)
                    postScale(-1f, 1f)
                }
                ExifInterface.ORIENTATION_ROTATE_90 -> setRotate(90f)
                ExifInterface.ORIENTATION_TRANSVERSE -> {
                    setRotate(-90f)
                    postScale(-1f, 1f)
                }
                ExifInterface.ORIENTATION_ROTATE_270 -> setRotate(-90f)
                else -> return bitmap
            }
        }
        return Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, matrix, true)
    }
}
