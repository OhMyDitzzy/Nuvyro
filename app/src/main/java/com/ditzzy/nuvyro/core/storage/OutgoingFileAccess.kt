package com.ditzzy.nuvyro.core.storage

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import com.ditzzy.nuvyro.data.send.OutgoingFile
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext

data class FileSelectionResult(val files: List<OutgoingFile>, val unavailableCount: Int)

class OutgoingFileAccess(private val resolver: ContentResolver) {
    suspend fun read(uris: List<Uri>): FileSelectionResult = withContext(Dispatchers.IO) {
        val files = mutableListOf<OutgoingFile>()
        var unavailable = 0
        for (uri in uris.distinct()) {
            ensureActive()
            try {
                if (uri.scheme != ContentResolver.SCHEME_CONTENT) {
                    throw IOException("Unsupported document URI")
                }
                val projection = arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
                val file = resolver.query(uri, projection, null, null, null)?.use { cursor ->
                    if (!cursor.moveToFirst()) throw IOException("Document is unavailable")
                    val nameColumn = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    val sizeColumn = cursor.getColumnIndex(OpenableColumns.SIZE)
                    val name = if (nameColumn >= 0 && !cursor.isNull(nameColumn)) {
                        cursor.getString(nameColumn)?.takeIf { it.isNotBlank() }
                    } else null
                    val size = if (sizeColumn >= 0 && !cursor.isNull(sizeColumn)) {
                        cursor.getLong(sizeColumn).takeIf { it >= 0 }
                    } else null
                    OutgoingFile(uri, name, size)
                } ?: throw IOException("Document metadata is unavailable")
                files += file
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                unavailable++
            }
        }
        FileSelectionResult(files, unavailable)
    }
}
