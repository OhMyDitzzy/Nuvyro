package com.ditzzy.nuvyro.data.hotspot

enum class HotspotStatus { Idle, Starting, Active, Paused, Failed }

enum class HotspotSecurity { Wpa2, Wpa3 }

data class HotspotCredentials(
    val ssid: String,
    val password: String,
    val security: HotspotSecurity = HotspotSecurity.Wpa2
)

data class HotspotState(
    val status: HotspotStatus = HotspotStatus.Idle,
    val credentials: HotspotCredentials? = null
)
