package com.ditzzy.nuvyro.ui.send.receiver

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import com.ditzzy.nuvyro.data.lan.LanMode
import com.ditzzy.nuvyro.data.lan.LanState
import com.ditzzy.nuvyro.data.lan.LanStatus
import com.ditzzy.nuvyro.ui.lan.LanActions
import com.ditzzy.nuvyro.ui.lan.LanSessionEffect
import com.ditzzy.nuvyro.ui.lan.LanViewModel
import com.ditzzy.nuvyro.ui.lan.rememberLanPermissionState
import com.ditzzy.nuvyro.ui.send.ReceiverScreen
import com.ditzzy.nuvyro.ui.send.SendUiState
import com.ditzzy.nuvyro.ui.send.SendViewModel

@Composable
fun ReceiverRoute(state: SendUiState, viewModel: SendViewModel, lan: LanState, lanViewModel: LanViewModel) {
    val permission = rememberLanPermissionState()
    LanSessionEffect(LanMode.Browse, permission.granted, lanViewModel)
    val discovery = remember(lan.status, lan.receivers) {
        ReceiverDiscoveryUiState(
            status = when (lan.status) {
                LanStatus.Paused -> ReceiverDiscoveryStatus.Paused
                LanStatus.PermissionRequired -> ReceiverDiscoveryStatus.PermissionRequired
                LanStatus.NoWifi -> ReceiverDiscoveryStatus.NoWifi
                LanStatus.Starting -> ReceiverDiscoveryStatus.Starting
                LanStatus.Active -> ReceiverDiscoveryStatus.Searching
                LanStatus.Failed -> ReceiverDiscoveryStatus.Failed
            },
            receivers = lan.receivers.map { ReceiverItem(it.id, it.name, it.photo) }
        )
    }
    LaunchedEffect(discovery) { viewModel.updateReceivers(discovery) }
    val selected = state.receiverDiscovery.selectedId?.takeIf { id -> discovery.receivers.any { it.id == id } }
    ReceiverScreen(
        state = state.copy(receiverDiscovery = discovery.copy(selectedId = selected)),
        onEditFiles = viewModel::showFiles,
        onBack = viewModel::back,
        onSelectReceiver = viewModel::selectReceiver,
        statusActions = { LanActions(lan.status, permission, lanViewModel::retry) }
    )
}
