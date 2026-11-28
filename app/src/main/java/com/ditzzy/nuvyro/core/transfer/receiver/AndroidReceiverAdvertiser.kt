package com.ditzzy.nuvyro.core.transfer.receiver

import android.net.Network
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import com.ditzzy.nuvyro.core.transfer.discovery.TransferNsdBudget
import java.io.IOException
import java.util.concurrent.Executor

internal class AndroidReceiverAdvertiser(
    manager: NsdManager?,
    network: Network,
    budget: TransferNsdBudget = TransferNsdBudget.Shared
) : ReceiverAdvertiser {
    private val advertiser = ReceiverNsdAdvertiser({ binding ->
        require(binding.networkHandle == network.networkHandle)
        val manager = manager ?: throw IOException("Transfer advertisement unavailable")
        AndroidRegistration(manager, network)
    }, budget)

    override fun advertise(binding: ReceiverAdvertisement) = advertiser.advertise(binding)
}

private class AndroidRegistration(
    manager: NsdManager,
    network: Network
) : ReceiverNsdPlatform, NsdManager.RegistrationListener {
    private val manager = manager
    private val network = network
    @Volatile private var callback: ReceiverNsdCallbacks? = null

    override fun register(binding: ReceiverAdvertisement, callback: ReceiverNsdCallbacks) {
        this.callback = callback
        val service = NsdServiceInfo().apply {
            serviceName = binding.serviceName
            serviceType = binding.serviceType
            port = binding.port
            for ((key, value) in binding.attributes()) setAttribute(key, value)
            if (Build.VERSION.SDK_INT >= 33) setNetwork(network)
        }
        if (Build.VERSION.SDK_INT >= 33) {
            manager.registerService(service, NsdManager.PROTOCOL_DNS_SD, Executor { it.run() }, this)
        } else manager.registerService(service, NsdManager.PROTOCOL_DNS_SD, this)
    }

    override fun unregister(callback: ReceiverNsdCallbacks) {
        check(this.callback === callback)
        manager.unregisterService(this)
    }

    override fun onServiceRegistered(info: NsdServiceInfo) { callback?.registered() }
    override fun onRegistrationFailed(info: NsdServiceInfo, error: Int) { callback?.registrationFailed() }
    override fun onServiceUnregistered(info: NsdServiceInfo) { callback?.unregistered() }
    override fun onUnregistrationFailed(info: NsdServiceInfo, error: Int) { callback?.unregistrationFailed() }
}
