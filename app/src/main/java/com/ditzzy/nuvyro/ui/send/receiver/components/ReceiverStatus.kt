package com.ditzzy.nuvyro.ui.send.receiver.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.ui.send.receiver.ReceiverDiscoveryStatus
import com.ditzzy.nuvyro.ui.send.receiver.ReceiverDiscoveryUiState

@Composable
internal fun ReceiverStatus(state: ReceiverDiscoveryUiState) {
    val title = when (state.status) {
        ReceiverDiscoveryStatus.PermissionRequired -> stringResource(R.string.lan_permission_title)
        ReceiverDiscoveryStatus.NoWifi -> stringResource(R.string.lan_wifi_title)
        ReceiverDiscoveryStatus.Starting -> stringResource(R.string.lan_starting)
        ReceiverDiscoveryStatus.Searching -> if (state.receivers.isEmpty()) {
            stringResource(R.string.receiver_searching)
        } else {
            pluralStringResource(R.plurals.receiver_found_count, state.receivers.size, state.receivers.size)
        }
        ReceiverDiscoveryStatus.Paused -> stringResource(R.string.receiver_paused)
        ReceiverDiscoveryStatus.Failed -> stringResource(R.string.receiver_failed)
    }
    val description = when (state.status) {
        ReceiverDiscoveryStatus.PermissionRequired -> R.string.lan_permission_hint
        ReceiverDiscoveryStatus.NoWifi -> R.string.lan_wifi_hint
        ReceiverDiscoveryStatus.Starting -> R.string.receiver_starting_hint
        ReceiverDiscoveryStatus.Searching -> R.string.receiver_search_hint
        ReceiverDiscoveryStatus.Paused -> R.string.receiver_paused_hint
        ReceiverDiscoveryStatus.Failed -> R.string.receiver_failed_hint
    }
    Column(
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
    ) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleLarge,
            color = if (state.status == ReceiverDiscoveryStatus.Failed) MaterialTheme.colorScheme.error
                else MaterialTheme.colorScheme.onSurface
        )
        Text(
            text = stringResource(description),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}
