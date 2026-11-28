package com.ditzzy.nuvyro.ui.send.receiver

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
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
import com.ditzzy.nuvyro.ui.hotspot.ConnectionGuideEntry
import com.ditzzy.nuvyro.ui.hotspot.ConnectionGuideRoute
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column

@Composable
fun ReceiverRoute(state: SendUiState, viewModel: SendViewModel, lan: LanState, lanViewModel: LanViewModel) {
    var showConnectionGuide by rememberSaveable { mutableStateOf(false) }
    var showPairingScanner by rememberSaveable { mutableStateOf(false) }
    val permission = rememberLanPermissionState()
    LanSessionEffect(LanMode.Browse, permission.granted, lanViewModel)
    BackHandler(enabled = showPairingScanner) {
        viewModel.clearScannedPairingQr()
        showPairingScanner = false
    }
    if (showPairingScanner) {
        PairingQrScannerRoute(
            state = state,
            onBack = {
                viewModel.clearScannedPairingQr()
                showPairingScanner = false
            },
            onScanned = viewModel::pairingQrScanned,
            onScanAgain = viewModel::clearScannedPairingQr
        )
        return
    }
    if (showConnectionGuide) {
        ConnectionGuideRoute(
            receiverCount = lan.receivers.size,
            onBack = { showConnectionGuide = false },
            onShowRadar = { showConnectionGuide = false }
        )
        return
    }
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
        statusActions = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                LanActions(lan.status, permission, lanViewModel::retry)
                ConnectionGuideEntry(onClick = { showConnectionGuide = true })
                FilledTonalButton(
                    onClick = { showPairingScanner = true },
                    modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
                ) {
                    Text(stringResource(R.string.receiver_scan_pairing_qr))
                }
            }
        }
    )
}
