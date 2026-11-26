package com.ditzzy.nuvyro.ui.lan

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.lan.LanStatus

@Composable
fun LanActions(status: LanStatus, permission: LanPermissionState, onRetry: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (status == LanStatus.PermissionRequired) {
            FilledTonalButton(
                onClick = permission.request,
                modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
            ) {
                Text(stringResource(
                    if (permission.declined || permission.granted) R.string.lan_open_settings
                    else R.string.lan_allow
                ))
            }
        } else if (status == LanStatus.Failed) {
            FilledTonalButton(
                onClick = onRetry,
                modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
            ) { Text(stringResource(R.string.setup_retry)) }
        }
        if (permission.settingsUnavailable) {
            Text(
                text = stringResource(R.string.lan_settings_unavailable),
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodyMedium
            )
        }
    }
}
