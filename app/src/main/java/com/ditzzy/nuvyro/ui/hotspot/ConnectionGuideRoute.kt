package com.ditzzy.nuvyro.ui.hotspot

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.core.hotspot.HotspotPermission
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus
import com.ditzzy.nuvyro.data.lan.LanStatus
import com.ditzzy.nuvyro.ui.lan.LanActions
import com.ditzzy.nuvyro.ui.lan.rememberLanPermissionState

@Composable
fun ConnectionGuideRoute(receiving: Boolean, onBack: () -> Unit, onFindReceiver: () -> Unit) {
    val context = LocalContext.current
    val factory = remember(context.applicationContext) { HotspotViewModelFactory(context) }
    val hotspotViewModel: HotspotViewModel = viewModel(factory = factory)
    val hotspot by hotspotViewModel.state.collectAsStateWithLifecycle()
    val permission = rememberHotspotPermissionState()
    val lanPermission = rememberLanPermissionState()
    LifecycleResumeEffect(Unit) {
        onPauseOrDispose { hotspotViewModel.stop() }
    }
    val leave = {
        hotspotViewModel.stop()
        onBack()
    }
    BackHandler(onBack = leave)
    ConnectionGuideScreen(
        receiving, hotspot, HotspotPermission.supported, leave, permission.openWifi,
        onFindReceiver = {
            if (lanPermission.granted) onFindReceiver() else lanPermission.request()
        }
    ) {
        if (!receiving && !lanPermission.granted) {
            LanActions(LanStatus.PermissionRequired, lanPermission, onRetry = {})
        }
        if (receiving && HotspotPermission.supported && hotspot.status != HotspotStatus.Active && hotspot.status != HotspotStatus.Starting) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (!lanPermission.granted) {
                    LanActions(LanStatus.PermissionRequired, lanPermission, onRetry = {})
                } else {
                    val label = when {
                        !permission.granted && permission.declined -> R.string.lan_open_settings
                        !permission.granted -> R.string.hotspot_allow
                        !permission.locationReady -> R.string.hotspot_enable_location
                        else -> R.string.hotspot_create
                    }
                    if (!permission.granted) Text(stringResource(R.string.hotspot_permission_hint))
                    if (!permission.locationReady && permission.granted) Text(stringResource(R.string.hotspot_location_hint))
                    FilledTonalButton(
                        onClick = {
                            when {
                                !permission.granted -> permission.request()
                                !permission.locationReady -> permission.openLocation()
                                else -> hotspotViewModel.create()
                            }
                        },
                        modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
                    ) { Text(stringResource(label)) }
                }
            }
        }
        if (permission.settingsUnavailable || lanPermission.settingsUnavailable) {
            Text(stringResource(R.string.lan_settings_unavailable), color = MaterialTheme.colorScheme.error)
        }
    }
}
