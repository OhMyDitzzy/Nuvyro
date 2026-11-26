package com.ditzzy.nuvyro.ui.hotspot

import android.content.Context
import android.net.ConnectivityManager
import android.net.nsd.NsdManager
import android.net.wifi.WifiManager
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import com.ditzzy.nuvyro.core.hotspot.LocalHotspot
import com.ditzzy.nuvyro.core.lan.LanAdvertiser
import com.ditzzy.nuvyro.core.profile.ProfilePhotoStore
import com.ditzzy.nuvyro.data.profile.ProfilePreferences

class HotspotViewModelFactory(context: Context) : ViewModelProvider.Factory {
    private val context = context.applicationContext

    override fun <T : ViewModel> create(modelClass: Class<T>): T {
        require(modelClass.isAssignableFrom(HotspotViewModel::class.java))
        @Suppress("UNCHECKED_CAST")
        return HotspotViewModel(
            LocalHotspot(context.getSystemService(WifiManager::class.java)),
            LanAdvertiser(
                context.getSystemService(ConnectivityManager::class.java),
                context.getSystemService(NsdManager::class.java),
                ProfilePreferences(context), ProfilePhotoStore(context)
            )
        ) as T
    }
}
