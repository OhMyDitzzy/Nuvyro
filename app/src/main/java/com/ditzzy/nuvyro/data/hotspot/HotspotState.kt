package com.ditzzy.nuvyro.data.hotspot

enum class HotspotStatus { Idle, Starting, Active, Paused, Failed }

data class HotspotCredentials(val ssid: String, val password: String)

data class HotspotState(
    val status: HotspotStatus = HotspotStatus.Idle,
    val credentials: HotspotCredentials? = null
)
