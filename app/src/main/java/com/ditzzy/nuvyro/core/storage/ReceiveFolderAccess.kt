package com.ditzzy.nuvyro.core.storage

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class ReceiveFolderAccess(private val resolver: ContentResolver) {
    suspend fun restore(uri: String?): ReceiveFolder? = withContext(Dispatchers.IO) {
        if (uri == null) return@withContext null
        runCatching { readFolder(Uri.parse(uri)) }.getOrNull()
    }

    suspend fun persist(selection: ReceiveFolderSelection): ReceiveFolder =
        withContext(Dispatchers.IO) {
            val uri = selection.uri
            val alreadyGranted = hasAccess(uri)
            val flags = selection.grantFlags and (
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            )
            if (uri.scheme != ContentResolver.SCHEME_CONTENT ||
                (flags and Intent.FLAG_GRANT_READ_URI_PERMISSION) == 0 ||
                (flags and Intent.FLAG_GRANT_WRITE_URI_PERMISSION) == 0 ||
                !DocumentsContract.isTreeUri(uri)
            ) {
                throw IOException("The selected folder does not grant read and write access")
            }
            try {
                resolver.takePersistableUriPermission(uri, flags)
                readFolder(uri) ?: throw IOException("The selected folder cannot receive files")
            } catch (error: Exception) {
                if (!alreadyGranted) releaseAccess(uri)
                throw error
            }
        }

    suspend fun release(uri: Uri) = withContext(Dispatchers.IO) {
        releaseAccess(uri)
    }

    private fun hasAccess(uri: Uri): Boolean = resolver.persistedUriPermissions.any {
        it.uri == uri && it.isReadPermission && it.isWritePermission
    }

    private fun readFolder(uri: Uri): ReceiveFolder? {
        if (!DocumentsContract.isTreeUri(uri) || !hasAccess(uri)) return null
        val document = DocumentsContract.buildDocumentUriUsingTree(
            uri,
            DocumentsContract.getTreeDocumentId(uri)
        )
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_FLAGS
        )
        return resolver.query(document, columns, null, null, null)?.use { cursor ->
            if (!cursor.moveToFirst()) return@use null
            val name = cursor.getString(0) ?: return@use null
            val mimeType = cursor.getString(1)
            val flags = cursor.getInt(2)
            if (mimeType != DocumentsContract.Document.MIME_TYPE_DIR ||
                (flags and DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE) == 0
            ) {
                return@use null
            }
            ReceiveFolder(uri, name)
        }
    }

    private fun releaseAccess(uri: Uri) {
        runCatching {
            resolver.releasePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            )
        }
    }
}
