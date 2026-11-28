package com.ditzzy.nuvyro.ui.receive

import android.content.Context
import android.net.ConnectivityManager
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import com.ditzzy.nuvyro.core.lan.LanNetworkMonitor
import com.ditzzy.nuvyro.core.transfer.receiver.AndroidReceiverSession

internal class ReceiverSessionViewModelFactory(context: Context) : ViewModelProvider.Factory {
    private val appContext = context.applicationContext

    override fun <T : ViewModel> create(modelClass: Class<T>): T {
        require(modelClass.isAssignableFrom(ReceiverSessionViewModel::class.java))
        @Suppress("UNCHECKED_CAST")
        return ReceiverSessionViewModel(
            LanNetworkMonitor(appContext.getSystemService(ConnectivityManager::class.java))::networks,
            { AndroidReceiverSession.create(appContext) }
        ) as T
    }
}
