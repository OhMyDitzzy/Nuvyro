package com.ditzzy.nuvyro.ui.profile

import android.content.ActivityNotFoundException
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import com.ditzzy.nuvyro.ui.profile.components.ProfileForm

@Composable
fun ProfileRoute(state: ProfileUiState, viewModel: ProfileViewModel) {
    ProfileScreen(
        state = state,
        onSave = { viewModel.save() },
        onBack = viewModel::discardChanges
    ) {
        ProfileFormRoute(state, viewModel)
    }
}

@Composable
fun ProfileFormRoute(
    state: ProfileUiState,
    viewModel: ProfileViewModel,
    enabled: Boolean = true
) {
    val photoLauncher = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri != null) viewModel.selectPhoto(uri)
    }
    ProfileForm(
        state = state.copy(busy = state.busy || !enabled),
        onNameChange = viewModel::changeName,
        onChoosePhoto = {
            try {
                photoLauncher.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
            } catch (_: ActivityNotFoundException) {
                viewModel.showPhotoError()
            } catch (_: SecurityException) {
                viewModel.showPhotoError()
            }
        },
        onRemovePhoto = viewModel::removePhoto,
        onRetry = viewModel::load
    )
}
