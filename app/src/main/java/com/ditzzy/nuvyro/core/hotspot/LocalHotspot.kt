package com.ditzzy.nuvyro.core.hotspot

import android.net.wifi.SoftApConfiguration
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import com.ditzzy.nuvyro.data.hotspot.HotspotCredentials
import com.ditzzy.nuvyro.data.hotspot.HotspotSecurity
import java.io.IOException
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch

class LocalHotspot(private val wifi: WifiManager?) {
    @Suppress("DEPRECATION")
    fun create() = callbackFlow<HotspotCredentials> {
        if (Build.VERSION.SDK_INT < 26 || wifi == null) throw IOException("Local hotspot is unavailable")
        var reservation: WifiManager.LocalOnlyHotspotReservation? = null
        val lock = Any()
        var closed = false
        val startup = launch { delay(20_000); close(IOException("Hotspot startup timed out")) }
        val callback = object : WifiManager.LocalOnlyHotspotCallback() {
            override fun onStarted(current: WifiManager.LocalOnlyHotspotReservation) {
                synchronized(lock) {
                    if (closed) {
                        current.close()
                        return
                    }
                    reservation = current
                    startup.cancel()
                    val credentials = if (Build.VERSION.SDK_INT >= 30) {
                        current.softApConfiguration?.let { config ->
                            HotspotCredentials(
                                config.ssid.orEmpty(), config.passphrase.orEmpty(),
                                if (config.securityType == SoftApConfiguration.SECURITY_TYPE_WPA3_SAE) {
                                    HotspotSecurity.Wpa3
                                } else HotspotSecurity.Wpa2
                            )
                        }
                    } else {
                        current.wifiConfiguration?.let { config ->
                            HotspotCredentials(
                                config.SSID.orEmpty().removeSurrounding("\""),
                                config.preSharedKey.orEmpty().removeSurrounding("\"")
                            )
                        }
                    }
                    if (credentials == null || credentials.ssid.isBlank() || credentials.password.isBlank()) {
                        close(IOException("Hotspot credentials are unavailable"))
                    } else if (!trySend(credentials).isSuccess) {
                        current.close()
                    }
                }
            }

            override fun onStopped() { close(IOException("Hotspot stopped")) }
            override fun onFailed(reason: Int) { close(IOException("Hotspot could not start: $reason")) }
        }
        try {
            wifi.startLocalOnlyHotspot(callback, Handler(Looper.getMainLooper()))
        } catch (error: SecurityException) {
            close(error)
        }
        awaitClose {
            startup.cancel()
            synchronized(lock) {
                closed = true
                reservation?.close()
                reservation = null
            }
        }
    }
}
