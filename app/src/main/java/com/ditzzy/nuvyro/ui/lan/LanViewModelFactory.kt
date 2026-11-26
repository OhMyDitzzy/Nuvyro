package com.ditzzy.nuvyro.ui.lan

import android.content.Context
import android.net.ConnectivityManager
import android.net.nsd.NsdManager
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewmodel.CreationExtras
import com.ditzzy.nuvyro.core.lan.LanAdvertiser
import com.ditzzy.nuvyro.core.lan.LanBrowser
import com.ditzzy.nuvyro.core.lan.LanNetworkMonitor
import com.ditzzy.nuvyro.core.lan.LanPermission
import com.ditzzy.nuvyro.core.profile.ProfilePhotoStore
import com.ditzzy.nuvyro.data.profile.ProfilePreferences

class LanViewModelFactory(context: Context) : ViewModelProvider.Factory {
    private val context = context.applicationContext

    override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T {
        require(modelClass.isAssignableFrom(LanViewModel::class.java))
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val nsd = context.getSystemService(NsdManager::class.java)
        @Suppress("UNCHECKED_CAST")
        return LanViewModel(
            LanNetworkMonitor(connectivity), LanBrowser(connectivity, nsd),
            LanAdvertiser(connectivity, nsd, ProfilePreferences(context), ProfilePhotoStore(context)),
            { LanPermission.isGranted(context) }, extras.createSavedStateHandle()
        ) as T
    }
}
