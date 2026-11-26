package com.ditzzy.nuvyro.ui.hotspot

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R

@Composable
fun ConnectionGuideScreen(
    receiverCount: Int,
    onBack: () -> Unit,
    onOpenWifi: () -> Unit,
    onShowRadar: () -> Unit,
    actions: @Composable () -> Unit = {}
) {
    ConnectionGuideLayout(
        onBack = onBack,
        bottomBar = {
            if (receiverCount > 0) {
                Text(
                    pluralStringResource(R.plurals.receiver_found_count, receiverCount, receiverCount),
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
                )
            }
            Button(onClick = onShowRadar, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)) {
                Text(stringResource(R.string.connection_back_to_radar))
            }
        }
    ) {
        Text(stringResource(R.string.connection_heading), style = MaterialTheme.typography.headlineSmall)
        Text(
            stringResource(R.string.connection_shared_wifi), style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        ConnectionOptionCard(
            title = stringResource(R.string.connection_wifi_title),
            description = stringResource(R.string.connection_wifi_hint)
        ) {
            FilledTonalButton(onClick = onOpenWifi, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 48.dp)) {
                Text(stringResource(R.string.connection_open_wifi))
            }
        }
        ConnectionOptionCard(
            title = stringResource(R.string.hotspot_join_title),
            description = stringResource(R.string.hotspot_join_steps)
        ) {
            Text(
                stringResource(R.string.hotspot_join_no_internet), style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        actions()
    }
}
