package com.ditzzy.nuvyro.ui.profile.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.profile.ProfileName
import com.ditzzy.nuvyro.ui.profile.ProfileError
import com.ditzzy.nuvyro.ui.profile.ProfileUiState

@Composable
fun ProfileForm(
    state: ProfileUiState,
    onNameChange: (String) -> Unit,
    onChoosePhoto: () -> Unit,
    onRemovePhoto: () -> Unit,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier
) {
    val focusManager = LocalFocusManager.current
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        if (state.loading) {
            CircularProgressIndicator()
        } else if (state.profile != null) {
            ProfileAvatar(state.name, state.photo, Modifier.size(96.dp))
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                FilledTonalButton(onClick = onChoosePhoto, enabled = !state.busy) {
                    Text(stringResource(
                        if (state.photo == null) R.string.profile_choose_photo
                        else R.string.profile_change_photo
                    ))
                }
                if (state.photo != null) {
                    TextButton(onClick = onRemovePhoto, enabled = !state.busy) {
                        Text(stringResource(R.string.profile_remove_photo))
                    }
                }
            }
            val invalid = state.nameInvalid ||
                ProfileName.length(ProfileName.normalize(state.name)) > ProfileName.MaxLength
            OutlinedTextField(
                value = state.name,
                onValueChange = onNameChange,
                enabled = !state.busy,
                label = { Text(stringResource(R.string.profile_display_name)) },
                supportingText = {
                    Text(stringResource(
                        if (invalid) R.string.profile_name_error else R.string.profile_name_hint,
                        ProfileName.MaxLength
                    ))
                },
                isError = invalid,
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done
                ),
                keyboardActions = KeyboardActions(onDone = { focusManager.clearFocus() }),
                modifier = Modifier.fillMaxWidth()
            )
            if (state.busy) {
                CircularProgressIndicator(modifier = Modifier.size(24.dp))
            }
        }
        state.error?.let { error ->
            Text(
                text = stringResource(when (error) {
                    ProfileError.LoadFailed -> R.string.profile_error_load
                    ProfileError.SaveFailed -> R.string.profile_error_save
                    ProfileError.PhotoUnavailable -> R.string.profile_error_photo
                }),
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
            )
            if (error == ProfileError.LoadFailed) {
                TextButton(onClick = onRetry) { Text(stringResource(R.string.setup_retry)) }
            }
        }
    }
}
