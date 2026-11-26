package com.ditzzy.nuvyro.ui.hotspot

import androidx.activity.compose.BackHandler
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.lan.LanStatus
import com.ditzzy.nuvyro.ui.lan.LanActions
import com.ditzzy.nuvyro.ui.lan.rememberLanPermissionState

@Composable
fun ConnectionGuideRoute(receiverCount: Int, onBack: () -> Unit, onShowRadar: () -> Unit) {
    val wifi = rememberHotspotPermissionState()
    val permission = rememberLanPermissionState()
    BackHandler(onBack = onBack)
    ConnectionGuideScreen(
        receiverCount = receiverCount,
        onBack = onBack,
        onOpenWifi = wifi.openWifi,
        onShowRadar = onShowRadar
    ) {
        if (!permission.granted) LanActions(LanStatus.PermissionRequired, permission, onRetry = {})
        if (wifi.settingsUnavailable || permission.settingsUnavailable) {
            Text(stringResource(R.string.lan_settings_unavailable), color = MaterialTheme.colorScheme.error)
        }
    }
}
