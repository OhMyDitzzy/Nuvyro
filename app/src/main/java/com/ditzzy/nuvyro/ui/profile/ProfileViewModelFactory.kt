package com.ditzzy.nuvyro.ui.profile

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewmodel.CreationExtras
import com.ditzzy.nuvyro.core.profile.ProfilePhotoStore
import com.ditzzy.nuvyro.data.profile.ProfilePreferences

class ProfileViewModelFactory(context: Context) : ViewModelProvider.Factory {
    private val applicationContext = context.applicationContext

    override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T {
        require(modelClass.isAssignableFrom(ProfileViewModel::class.java))
        @Suppress("UNCHECKED_CAST")
        return ProfileViewModel(
            ProfilePreferences(applicationContext), ProfilePhotoStore(applicationContext),
            extras.createSavedStateHandle()
        ) as T
    }
}
