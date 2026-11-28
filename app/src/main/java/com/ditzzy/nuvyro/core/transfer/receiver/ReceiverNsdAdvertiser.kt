package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.discovery.TransferNsdBudget
import java.io.IOException
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch

internal class ReceiverNsdAdvertiser(
    private val platform: (ReceiverAdvertisement) -> ReceiverNsdPlatform,
    private val budget: TransferNsdBudget = TransferNsdBudget.Shared,
    private val startupTimeoutMillis: Long = 10_000
) : ReceiverAdvertiser {
    init { require(startupTimeoutMillis > 0) }

    override fun advertise(binding: ReceiverAdvertisement) = callbackFlow<Unit> {
        val platform = platform(binding)
        val lease = budget.advertisement() ?: throw IOException("Transfer advertisement busy")
        val registration = ReceiverNsdRegistration(platform, binding, lease, channel)
        val startup = launch {
            delay(startupTimeoutMillis)
            if (!registration.isRegistered()) close(IOException("Transfer advertisement timed out"))
        }
        try {
            registration.submit()
            awaitClose { }
        } finally {
            startup.cancel()
            registration.stop()
        }
    }
}
