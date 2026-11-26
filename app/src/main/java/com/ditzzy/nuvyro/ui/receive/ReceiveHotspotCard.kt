package com.ditzzy.nuvyro.ui.receive

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.hotspot.HotspotState
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus

@Composable
internal fun ReceiveHotspotCard(hotspot: HotspotState, onShowDetails: () -> Unit, onStop: () -> Unit) {
    val hosting = hotspot.status == HotspotStatus.Active || hotspot.status == HotspotStatus.Starting
    Surface(
        shape = MaterialTheme.shapes.large,
        color = if (hosting) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surfaceContainer,
        modifier = Modifier.fillMaxWidth()
    ) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                stringResource(when (hotspot.status) {
                    HotspotStatus.Active -> R.string.hotspot_on
                    HotspotStatus.Starting -> R.string.hotspot_starting
                    else -> R.string.hotspot_off
                }),
                style = MaterialTheme.typography.titleMedium
            )
            hotspot.credentials?.let {
                Text(stringResource(R.string.hotspot_network_name, it.ssid), style = MaterialTheme.typography.bodyLarge)
            }
            if (!hosting) {
                Text(
                    stringResource(if (hotspot.status == HotspotStatus.Failed) R.string.hotspot_failed else R.string.hotspot_paused),
                    style = MaterialTheme.typography.bodyMedium
                )
            }
            FilledTonalButton(onClick = onShowDetails, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 48.dp)) {
                Text(stringResource(if (hotspot.status == HotspotStatus.Active) R.string.hotspot_show_qr else R.string.connection_title))
            }
            if (hosting) {
                TextButton(onClick = onStop, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 48.dp)) {
                    Text(stringResource(if (hotspot.status == HotspotStatus.Starting) R.string.hotspot_cancel else R.string.hotspot_stop))
                }
            }
        }
    }
}
