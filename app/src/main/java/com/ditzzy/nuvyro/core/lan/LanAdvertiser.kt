package com.ditzzy.nuvyro.core.lan

import android.net.ConnectivityManager
import android.net.Network
import android.net.nsd.NsdManager
import com.ditzzy.nuvyro.core.lan.profile.DiscoveryPhoto
import com.ditzzy.nuvyro.core.lan.profile.ProfilePacket
import com.ditzzy.nuvyro.core.lan.profile.ProfileServer
import com.ditzzy.nuvyro.core.profile.ProfilePhotoStore
import com.ditzzy.nuvyro.data.profile.ProfilePreferences
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class LanAdvertiser(
    private val connectivity: ConnectivityManager?,
    manager: NsdManager?,
    private val profiles: ProfilePreferences,
    private val photos: ProfilePhotoStore
) {
    private val nsd = LanNsd(manager)

    suspend fun advertise(network: Network, onReady: () -> Unit) = coroutineScope {
        val packet = withContext(Dispatchers.IO) {
            val profile = profiles.load()
            ProfilePacket(
                UUID.randomUUID().toString(), profile.displayName,
                DiscoveryPhoto.encode(photos.load(profile.photoFileName))
            )
        }
        val links = connectivity?.getLinkProperties(network)?.linkAddresses.orEmpty()
        if (links.isEmpty()) throw IOException("Wi-Fi has no local address")
        withContext(Dispatchers.IO) {
            ProfileServer(links, packet).use { server ->
                coroutineScope {
                    val serving = launch { server.serve() }
                    try {
                        nsd.register(network, server.port, packet.sessionId).collect { onReady() }
                    } finally {
                        withContext(NonCancellable) { serving.cancelAndJoin() }
                    }
                }
            }
        }
    }
}
