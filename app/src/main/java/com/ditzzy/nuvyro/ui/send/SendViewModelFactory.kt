package com.ditzzy.nuvyro.ui.send

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewmodel.CreationExtras
import com.ditzzy.nuvyro.core.storage.OutgoingFileAccess

class SendViewModelFactory(context: Context) : ViewModelProvider.Factory {
    private val resolver = context.applicationContext.contentResolver

    override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T {
        require(modelClass.isAssignableFrom(SendViewModel::class.java))
        @Suppress("UNCHECKED_CAST")
        return SendViewModel(OutgoingFileAccess(resolver), extras.createSavedStateHandle()) as T
    }
}
