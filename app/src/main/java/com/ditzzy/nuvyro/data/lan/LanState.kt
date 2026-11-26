package com.ditzzy.nuvyro.data.lan

import android.graphics.Bitmap

enum class LanMode { Browse, Advertise }

enum class LanStatus { Paused, PermissionRequired, NoWifi, Starting, Active, Failed }

data class NearbyReceiver(val id: String, val name: String, val photo: Bitmap?)

data class LanState(
    val receiving: Boolean = false,
    val status: LanStatus = LanStatus.Paused,
    val receivers: List<NearbyReceiver> = emptyList()
)
