package com.ditzzy.nuvyro.ui.setup

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import com.ditzzy.nuvyro.core.storage.ReceiveFolderAccess
import com.ditzzy.nuvyro.data.setup.SetupPreferences

class SetupViewModelFactory(context: Context) : ViewModelProvider.Factory {
    private val applicationContext = context.applicationContext

    override fun <T : ViewModel> create(modelClass: Class<T>): T {
        require(modelClass.isAssignableFrom(SetupViewModel::class.java))
        @Suppress("UNCHECKED_CAST")
        return SetupViewModel(
            SetupPreferences(applicationContext),
            ReceiveFolderAccess(applicationContext.contentResolver)
        ) as T
    }
}
