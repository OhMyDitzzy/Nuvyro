package com.ditzzy.nuvyro.ui.hotspot

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.hotspot.HotspotState
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus

@Composable
internal fun HotspotSetupScreen(
    hotspot: HotspotState,
    supported: Boolean,
    onBack: () -> Unit,
    onOpenWifi: () -> Unit,
    onStopHotspot: () -> Unit,
    createAction: @Composable () -> Unit = {}
) {
    val hosting = hotspot.status == HotspotStatus.Active || hotspot.status == HotspotStatus.Starting
    ConnectionGuideLayout(
        onBack = onBack,
        bottomBar = {
            Button(onClick = onBack, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)) {
                Text(stringResource(if (hosting) R.string.hotspot_continue_receiving else R.string.hotspot_back_to_receive))
            }
        }
    ) {
        Text(
            stringResource(when (hotspot.status) {
                HotspotStatus.Active -> R.string.hotspot_ready
                HotspotStatus.Starting -> R.string.hotspot_starting
                else -> R.string.connection_heading
            }),
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
        )
        if (hosting) {
            Text(
                stringResource(if (hotspot.status == HotspotStatus.Active) R.string.hotspot_host_instructions else R.string.hotspot_starting_hint),
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            if (hotspot.status == HotspotStatus.Starting) CircularProgressIndicator()
            hotspot.credentials?.let { HotspotCredentialsCard(it) }
            Text(
                stringResource(R.string.hotspot_lifetime_hint), style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            TextButton(onClick = onStopHotspot, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 48.dp)) {
                Text(stringResource(if (hotspot.status == HotspotStatus.Starting) R.string.hotspot_cancel else R.string.hotspot_stop))
            }
        } else {
            ConnectionOptionCard(
                title = stringResource(R.string.connection_wifi_title),
                description = stringResource(R.string.connection_wifi_hint)
            ) {
                FilledTonalButton(onClick = onOpenWifi, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 48.dp)) {
                    Text(stringResource(R.string.connection_open_wifi))
                }
            }
            ConnectionOptionCard(
                title = stringResource(R.string.hotspot_host_title),
                description = stringResource(if (supported) R.string.hotspot_host_hint else R.string.hotspot_unsupported)
            ) {
                if (hotspot.status == HotspotStatus.Failed || hotspot.status == HotspotStatus.Paused) {
                    Text(
                        stringResource(if (hotspot.status == HotspotStatus.Failed) R.string.hotspot_failed else R.string.hotspot_paused),
                        color = if (hotspot.status == HotspotStatus.Failed) MaterialTheme.colorScheme.error
                            else MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
                    )
                }
                createAction()
            }
        }
    }
}
