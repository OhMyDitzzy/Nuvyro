package com.ditzzy.nuvyro.data.send

import android.net.Uri

data class OutgoingFile(
    val uri: Uri,
    val name: String?,
    val size: Long?
)
