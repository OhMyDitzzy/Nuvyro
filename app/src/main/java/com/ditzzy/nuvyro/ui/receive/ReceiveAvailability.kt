package com.ditzzy.nuvyro.ui.receive

import androidx.annotation.StringRes
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus
import com.ditzzy.nuvyro.data.lan.LanStatus

internal data class ReceiveAvailability(
    @param:StringRes val title: Int,
    @param:StringRes val hint: Int,
    val starting: Boolean = false
)

internal fun receiveAvailability(lan: LanStatus, hotspot: HotspotStatus): ReceiveAvailability = when (hotspot) {
    HotspotStatus.Active -> ReceiveAvailability(R.string.receive_available, R.string.receive_hotspot_visibility_hint)
    HotspotStatus.Starting -> ReceiveAvailability(R.string.hotspot_starting, R.string.hotspot_starting_hint, starting = true)
    else -> when (lan) {
        LanStatus.Active -> ReceiveAvailability(R.string.receive_available, R.string.receive_visibility_hint)
        LanStatus.Starting -> ReceiveAvailability(R.string.lan_starting, R.string.receive_visibility_hint, starting = true)
        LanStatus.PermissionRequired -> ReceiveAvailability(R.string.lan_permission_title, R.string.lan_permission_hint)
        LanStatus.NoWifi -> ReceiveAvailability(R.string.lan_wifi_title, R.string.lan_wifi_hint)
        LanStatus.Paused -> ReceiveAvailability(R.string.receive_paused, R.string.receive_visibility_hint)
        LanStatus.Failed -> ReceiveAvailability(R.string.receive_failed, R.string.receive_failed_hint)
    }
}
