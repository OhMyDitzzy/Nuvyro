package com.ditzzy.nuvyro.ui.receive

import android.graphics.Bitmap
import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus
import com.ditzzy.nuvyro.data.lan.LanMode
import com.ditzzy.nuvyro.data.lan.LanState
import com.ditzzy.nuvyro.ui.lan.LanActions
import com.ditzzy.nuvyro.ui.lan.LanSessionEffect
import com.ditzzy.nuvyro.ui.lan.LanViewModel
import com.ditzzy.nuvyro.ui.lan.rememberLanPermissionState
import com.ditzzy.nuvyro.ui.hotspot.ConnectionGuideEntry
import com.ditzzy.nuvyro.ui.hotspot.HotspotSetupRoute
import com.ditzzy.nuvyro.ui.hotspot.HotspotViewModel
import com.ditzzy.nuvyro.ui.hotspot.HotspotViewModelFactory

@Composable
fun ReceiveRoute(state: LanState, name: String, photo: Bitmap?, lanViewModel: LanViewModel) {
    var showConnectionGuide by rememberSaveable { mutableStateOf(false) }
    val context = LocalContext.current
    val factory = remember(context.applicationContext) { HotspotViewModelFactory(context) }
    val hotspotViewModel: HotspotViewModel = viewModel(factory = factory)
    val hotspot by hotspotViewModel.state.collectAsStateWithLifecycle()
    val receiverFactory = remember(context.applicationContext) { ReceiverSessionViewModelFactory(context) }
    val receiverViewModel: ReceiverSessionViewModel = viewModel(factory = receiverFactory)
    val receiverState by receiverViewModel.state.collectAsStateWithLifecycle()
    val hosting = hotspot.status == HotspotStatus.Starting || hotspot.status == HotspotStatus.Active
    val permission = rememberLanPermissionState()
    // The Receive session owns the hotspot; opening or closing its QR details keeps it alive.
    LifecycleResumeEffect(Unit) {
        onPauseOrDispose { hotspotViewModel.pause() }
    }
    LifecycleResumeEffect(hotspot.status, permission.granted) {
        when {
            !permission.granted || hotspot.status == HotspotStatus.Starting -> receiverViewModel.stop()
            hotspot.status == HotspotStatus.Active -> receiverViewModel.startHotspot(hotspotViewModel::ownsActiveReservation)
            else -> receiverViewModel.startWifi()
        }
        onPauseOrDispose { receiverViewModel.stop() }
    }
    LanSessionEffect(LanMode.Advertise, permission.granted, lanViewModel, enabled = !hosting)
    if (showConnectionGuide) {
        HotspotSetupRoute(hotspot, hotspotViewModel, onBack = { showConnectionGuide = false })
        return
    }
    val stopReceiving = {
        receiverViewModel.stop()
        hotspotViewModel.stop()
        lanViewModel.closeReceive()
    }
    BackHandler(onBack = stopReceiving)
    ReceiveScreen(
        status = state.status,
        name = name,
        photo = photo,
        onStop = stopReceiving,
        hotspot = hotspot,
        receiverSession = receiverState,
        showReceiverSession = permission.granted && hotspot.status != HotspotStatus.Starting,
        onRetryReceiverSession = receiverViewModel::restart
    ) {
        if (!hosting) LanActions(state.status, permission, lanViewModel::retry)
        if (hotspot.status == HotspotStatus.Idle) {
            ConnectionGuideEntry(receiving = true, onClick = { showConnectionGuide = true })
        } else {
            ReceiveHotspotCard(hotspot, onShowDetails = { showConnectionGuide = true }, onStop = hotspotViewModel::stop)
        }
    }
}
