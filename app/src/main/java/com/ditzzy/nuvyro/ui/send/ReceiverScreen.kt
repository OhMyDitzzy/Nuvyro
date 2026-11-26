package com.ditzzy.nuvyro.ui.send

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.ui.send.components.FileSelectionSummary
import com.ditzzy.nuvyro.ui.send.components.SendScaffold
import com.ditzzy.nuvyro.ui.send.receiver.ReceiverDiscoveryStatus
import com.ditzzy.nuvyro.ui.send.receiver.components.ReceiverRadar
import com.ditzzy.nuvyro.ui.send.receiver.components.ReceiverStatus
import com.ditzzy.nuvyro.ui.send.receiver.components.SelectedReceiver

@Composable
fun ReceiverScreen(
    state: SendUiState,
    onEditFiles: () -> Unit,
    onBack: () -> Unit,
    onSelectReceiver: (String) -> Unit = {}
) {
    var resumed by remember { mutableStateOf(false) }
    LifecycleResumeEffect(Unit) {
        resumed = true
        onPauseOrDispose { resumed = false }
    }
    val discovery = state.receiverDiscovery
    val scanning = discovery.status == ReceiverDiscoveryStatus.Searching &&
        (resumed || LocalInspectionMode.current)
    SendScaffold(
        title = stringResource(R.string.send_receiver_title),
        onBack = onBack,
        bottomBar = {
            FileSelectionSummary(state.files)
            FilledTonalButton(
                onClick = onEditFiles,
                modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
            ) {
                Text(stringResource(R.string.send_edit_files))
            }
        }
    ) { innerPadding ->
        Box(
            modifier = Modifier.fillMaxSize().padding(innerPadding),
            contentAlignment = Alignment.TopCenter
        ) {
            Column(
                modifier = Modifier.widthIn(max = 568.dp).fillMaxSize()
                    .verticalScroll(rememberScrollState()).padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                ReceiverStatus(discovery)
                ReceiverRadar(
                    receivers = discovery.receivers,
                    selectedId = discovery.selectedId,
                    scanning = scanning,
                    onSelect = onSelectReceiver
                )
                discovery.selectedReceiver?.let { receiver ->
                    SelectedReceiver(receiver)
                }
            }
        }
    }
}
