package com.ditzzy.nuvyro.ui.setup

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.setup.SetupStep
import com.ditzzy.nuvyro.ui.setup.components.SetupNavigation
import com.ditzzy.nuvyro.ui.setup.components.SetupStepContent

@Composable
fun SetupScreen(
    state: SetupUiState,
    notificationsEnabled: Boolean,
    notificationDeclined: Boolean,
    batteryOptimizationDisabled: Boolean,
    onNotificationsClick: () -> Unit,
    onBackgroundClick: () -> Unit,
    onFolderClick: () -> Unit,
    onNext: () -> Unit,
    onBack: () -> Unit,
    onSkipProfile: () -> Unit,
    profileReady: Boolean,
    profileContent: @Composable () -> Unit
) {
    BackHandler(enabled = state.step != SetupStep.Welcome) {
        if (!state.saving) onBack()
    }
    Scaffold(
        modifier = Modifier.imePadding(),
        bottomBar = {
            SetupNavigation(
                state, notificationsEnabled, batteryOptimizationDisabled,
                onNext, onBack, onSkipProfile, profileReady
            )
        }
    ) { innerPadding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(innerPadding)
                .verticalScroll(rememberScrollState())
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Column(
                modifier = Modifier.widthIn(max = 520.dp).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(24.dp)
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = stringResource(R.string.app_name),
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.weight(1f)
                    )
                    Text(
                        text = stringResource(
                            R.string.setup_step_count, state.step.ordinal + 1, SetupStep.entries.size
                        ),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                LinearProgressIndicator(
                    progress = { (state.step.ordinal + 1).toFloat() / SetupStep.entries.size },
                    modifier = Modifier.fillMaxWidth()
                )
                Surface(
                    modifier = Modifier.padding(top = 8.dp).size(96.dp),
                    shape = MaterialTheme.shapes.extraLarge,
                    color = MaterialTheme.colorScheme.primaryContainer,
                    contentColor = MaterialTheme.colorScheme.onPrimaryContainer
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(
                            painter = painterResource(state.step.icon),
                            contentDescription = null,
                            modifier = Modifier.size(44.dp)
                        )
                    }
                }
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(
                        text = stringResource(state.step.title),
                        style = MaterialTheme.typography.headlineLarge,
                        fontWeight = FontWeight.SemiBold
                    )
                    Text(
                        text = stringResource(state.step.description),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                SetupStepContent(
                    state, notificationsEnabled, notificationDeclined, batteryOptimizationDisabled,
                    onNotificationsClick, onBackgroundClick, onFolderClick, profileContent
                )
                state.error?.let { error ->
                    Text(
                        text = stringResource(when (error) {
                            SetupError.SaveFailed -> R.string.setup_error_save
                            SetupError.FolderUnavailable -> R.string.setup_error_folder
                            SetupError.SettingsUnavailable -> R.string.setup_error_settings
                        }),
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
                    )
                }
            }
        }
    }
}
