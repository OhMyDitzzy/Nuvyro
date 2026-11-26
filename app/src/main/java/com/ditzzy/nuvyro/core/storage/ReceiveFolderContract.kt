package com.ditzzy.nuvyro.core.storage

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.DocumentsContract
import androidx.activity.result.contract.ActivityResultContract

data class ReceiveFolderSelection(val uri: Uri, val grantFlags: Int)

class ReceiveFolderContract : ActivityResultContract<Uri?, ReceiveFolderSelection?>() {
    override fun createIntent(context: Context, input: Uri?): Intent =
        Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
            )
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && input != null) {
                putExtra(DocumentsContract.EXTRA_INITIAL_URI, input)
            }
        }

    override fun parseResult(resultCode: Int, intent: Intent?): ReceiveFolderSelection? {
        if (resultCode != Activity.RESULT_OK) return null
        val uri = intent?.data ?: return null
        return ReceiveFolderSelection(uri, intent.flags)
    }
}
