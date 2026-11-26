package com.ditzzy.nuvyro.ui.setup.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.setup.SetupStep
import com.ditzzy.nuvyro.ui.setup.SetupUiState

@Composable
internal fun SetupNavigation(
    state: SetupUiState,
    notificationsEnabled: Boolean,
    batteryOptimizationDisabled: Boolean,
    onNext: () -> Unit,
    onBack: () -> Unit,
    onSkipProfile: () -> Unit,
    profileReady: Boolean
) {
    val nextLabel = when (state.step) {
        SetupStep.Welcome -> R.string.setup_get_started
        SetupStep.Profile -> R.string.profile_save_continue
        SetupStep.ReceiveFolder -> R.string.setup_finish
        SetupStep.Notifications -> if (notificationsEnabled) R.string.setup_continue
            else R.string.setup_skip
        SetupStep.Background -> if (batteryOptimizationDisabled) R.string.setup_continue
            else R.string.setup_skip
    }
    Surface(color = MaterialTheme.colorScheme.surface) {
        Column(
            modifier = Modifier.fillMaxWidth().navigationBarsPadding().padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Column(
                modifier = Modifier.widthIn(max = 520.dp).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                Button(
                    onClick = onNext,
                    enabled = !state.saving && (state.step != SetupStep.Profile || profileReady) && (
                        state.step != SetupStep.ReceiveFolder || state.folder != null
                    ),
                    modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
                ) {
                    Text(stringResource(when {
                        state.saving && state.step == SetupStep.Profile -> R.string.profile_working
                        state.saving -> R.string.setup_saving
                        else -> nextLabel
                    }))
                }
                if (state.step != SetupStep.Welcome) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        TextButton(
                            onClick = onBack,
                            enabled = !state.saving,
                            modifier = Modifier.weight(1f).sizeIn(minHeight = 48.dp)
                        ) {
                            Text(stringResource(R.string.setup_back))
                        }
                        if (state.step == SetupStep.Profile) {
                            TextButton(
                                onClick = onSkipProfile,
                                enabled = !state.saving,
                                modifier = Modifier.weight(1f).sizeIn(minHeight = 48.dp)
                            ) {
                                Text(stringResource(R.string.setup_skip))
                            }
                        }
                    }
                }
            }
        }
    }
}
