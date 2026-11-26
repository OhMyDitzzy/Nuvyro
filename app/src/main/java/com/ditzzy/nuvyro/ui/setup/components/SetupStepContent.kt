package com.ditzzy.nuvyro.ui.setup.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.setup.SetupStep
import com.ditzzy.nuvyro.ui.setup.SetupUiState

@Composable
internal fun SetupStepContent(
    state: SetupUiState,
    notificationsEnabled: Boolean,
    notificationDeclined: Boolean,
    batteryOptimizationDisabled: Boolean,
    onNotificationsClick: () -> Unit,
    onBackgroundClick: () -> Unit,
    onFolderClick: () -> Unit
) {
    Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
        when (state.step) {
            SetupStep.Welcome -> Text(
                text = stringResource(R.string.setup_welcome_details),
                style = MaterialTheme.typography.bodyLarge
            )
            SetupStep.Notifications -> {
                StatusCard(stringResource(
                    if (notificationsEnabled) R.string.setup_notifications_enabled
                    else R.string.setup_notifications_optional
                ))
                if (!notificationsEnabled) {
                    if (notificationDeclined) {
                        Text(
                            text = stringResource(R.string.setup_notifications_declined),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    SetupActionButton(
                        label = stringResource(
                            if (notificationDeclined) R.string.setup_open_notification_settings
                            else R.string.setup_allow_notifications
                        ),
                        enabled = !state.saving,
                        onClick = onNotificationsClick
                    )
                }
            }
            SetupStep.Background -> {
                StatusCard(stringResource(
                    if (batteryOptimizationDisabled) R.string.setup_background_unrestricted
                    else R.string.setup_background_default
                ))
                Text(
                    text = stringResource(R.string.setup_background_hint),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                if (!batteryOptimizationDisabled) {
                    SetupActionButton(
                        label = stringResource(R.string.setup_open_battery_settings),
                        enabled = !state.saving,
                        onClick = onBackgroundClick
                    )
                }
            }
            SetupStep.ReceiveFolder -> {
                state.folder?.let { folder ->
                    StatusCard(stringResource(R.string.setup_folder_selected, folder.name))
                }
                SetupActionButton(
                    label = stringResource(
                        if (state.folder == null) R.string.setup_choose_folder
                        else R.string.setup_change_folder
                    ),
                    enabled = !state.saving,
                    onClick = onFolderClick
                )
                Text(
                    text = stringResource(R.string.setup_folder_hint),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
    }
}

@Composable
private fun StatusCard(text: String) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.secondaryContainer,
        contentColor = MaterialTheme.colorScheme.onSecondaryContainer
    ) {
        Text(text = text, modifier = Modifier.padding(20.dp))
    }
}

@Composable
private fun SetupActionButton(label: String, enabled: Boolean, onClick: () -> Unit) {
    FilledTonalButton(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
    ) {
        Text(label)
    }
}
