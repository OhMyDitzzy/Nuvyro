package com.ditzzy.nuvyro.ui.hotspot

import androidx.activity.compose.BackHandler
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.core.hotspot.HotspotPermission
import com.ditzzy.nuvyro.data.hotspot.HotspotState
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus
import com.ditzzy.nuvyro.data.lan.LanStatus
import com.ditzzy.nuvyro.ui.lan.LanActions
import com.ditzzy.nuvyro.ui.lan.rememberLanPermissionState

@Composable
internal fun HotspotSetupRoute(state: HotspotState, viewModel: HotspotViewModel, onBack: () -> Unit) {
    val permission = rememberHotspotPermissionState()
    val lanPermission = rememberLanPermissionState()
    BackHandler(onBack = onBack)
    HotspotSetupScreen(
        hotspot = state,
        supported = HotspotPermission.supported,
        onBack = onBack,
        onOpenWifi = permission.openWifi,
        onStopHotspot = viewModel::stop
    ) {
        if (HotspotPermission.supported && state.status != HotspotStatus.Active && state.status != HotspotStatus.Starting) {
            if (!lanPermission.granted) {
                LanActions(LanStatus.PermissionRequired, lanPermission, onRetry = {})
            } else {
                HotspotCreateAction(permission, onCreate = viewModel::create)
            }
        }
        if (permission.settingsUnavailable || lanPermission.settingsUnavailable) {
            Text(stringResource(R.string.lan_settings_unavailable), color = MaterialTheme.colorScheme.error)
        }
    }
}
