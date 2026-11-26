package com.ditzzy.nuvyro.ui.profile

import android.graphics.Bitmap
import com.ditzzy.nuvyro.data.profile.LocalProfile

enum class ProfileError {
    LoadFailed,
    SaveFailed,
    PhotoUnavailable
}

data class ProfileUiState(
    val loading: Boolean = true,
    val profile: LocalProfile? = null,
    val savedPhoto: Bitmap? = null,
    val name: String = "",
    val photo: Bitmap? = null,
    val busy: Boolean = false,
    val editing: Boolean = false,
    val nameInvalid: Boolean = false,
    val error: ProfileError? = null
)
