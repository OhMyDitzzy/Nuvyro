package com.ditzzy.nuvyro.ui.hotspot

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.hotspot.HotspotState
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus
import com.ditzzy.nuvyro.ui.send.components.SendScaffold

@Composable
fun ConnectionGuideScreen(
    receiving: Boolean,
    hotspot: HotspotState,
    supported: Boolean,
    onBack: () -> Unit,
    onOpenWifi: () -> Unit,
    onFindReceiver: () -> Unit,
    hostActions: @Composable () -> Unit = {}
) {
    SendScaffold(
        title = stringResource(R.string.connection_title),
        onBack = onBack,
        bottomBar = {
            if (receiving) {
                Button(onClick = onBack, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)) {
                    Text(stringResource(R.string.hotspot_back_to_wifi))
                }
            } else {
                Button(onClick = onFindReceiver, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)) {
                    Text(stringResource(R.string.hotspot_find_receiver))
                }
            }
        }
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            Column(
                modifier = Modifier.widthIn(max = 568.dp).fillMaxSize()
                    .verticalScroll(rememberScrollState()).padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp)
            ) {
                Text(stringResource(R.string.connection_heading), style = MaterialTheme.typography.headlineSmall)
                Text(
                    stringResource(R.string.connection_shared_wifi), style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                if (hotspot.status != HotspotStatus.Active && hotspot.status != HotspotStatus.Starting) {
                    FilledTonalButton(onClick = onOpenWifi, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)) {
                        Text(stringResource(R.string.connection_open_wifi))
                    }
                }
                if (receiving) {
                    Text(stringResource(R.string.hotspot_host_title), style = MaterialTheme.typography.titleLarge)
                    Text(
                        stringResource(if (supported) R.string.hotspot_host_hint else R.string.hotspot_unsupported),
                        style = MaterialTheme.typography.bodyLarge
                    )
                    if (hotspot.status == HotspotStatus.Starting) {
                        CircularProgressIndicator()
                        Text(stringResource(R.string.hotspot_starting))
                    }
                    hotspot.credentials?.let { credentials ->
                        Surface(
                            shape = MaterialTheme.shapes.large,
                            color = MaterialTheme.colorScheme.secondaryContainer,
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Text(stringResource(R.string.hotspot_ready), style = MaterialTheme.typography.titleLarge)
                                HotspotWifiQr(credentials)
                                SelectionContainer {
                                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                        Text(stringResource(R.string.hotspot_network_name, credentials.ssid))
                                        Text(stringResource(R.string.hotspot_password, credentials.password))
                                    }
                                }
                                Text(stringResource(R.string.hotspot_host_instructions), style = MaterialTheme.typography.bodyMedium)
                            }
                        }
                    }
                    if (hotspot.status == HotspotStatus.Failed || hotspot.status == HotspotStatus.Paused) {
                        Text(
                            stringResource(if (hotspot.status == HotspotStatus.Failed) R.string.hotspot_failed else R.string.hotspot_paused),
                            color = if (hotspot.status == HotspotStatus.Failed) MaterialTheme.colorScheme.error
                                else MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
                        )
                    }
                    Text(stringResource(R.string.hotspot_lifetime_hint), style = MaterialTheme.typography.bodyMedium)
                    Text(stringResource(R.string.receive_transfer_unavailable), style = MaterialTheme.typography.bodyMedium)
                } else {
                    Text(stringResource(R.string.hotspot_join_title), style = MaterialTheme.typography.titleLarge)
                    Text(stringResource(R.string.hotspot_join_steps), style = MaterialTheme.typography.bodyLarge)
                    Text(
                        stringResource(R.string.hotspot_join_no_internet), style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                hostActions()
            }
        }
    }
}
