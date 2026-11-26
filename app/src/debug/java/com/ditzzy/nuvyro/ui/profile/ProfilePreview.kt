package com.ditzzy.nuvyro.ui.profile

import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import com.ditzzy.nuvyro.data.profile.LocalProfile
import com.ditzzy.nuvyro.ui.profile.components.ProfileForm
import com.ditzzy.nuvyro.ui.theme.NuvyroTheme

class ProfilePreviewProvider : PreviewParameterProvider<ProfileUiState> {
    override val values = sequenceOf(
        ProfileUiState(loading = false, profile = LocalProfile("Alex"), name = "Alex"),
        ProfileUiState(
            loading = false, profile = LocalProfile("Alex"), name = "", nameInvalid = true
        ),
        ProfileUiState(loading = false, error = ProfileError.LoadFailed),
        ProfileUiState(
            loading = false, profile = LocalProfile("Alex"), name = "Alex", busy = true
        )
    )
}

@Preview(name = "Light", widthDp = 360, heightDp = 800)
@Preview(name = "Dark", uiMode = Configuration.UI_MODE_NIGHT_YES, widthDp = 360, heightDp = 800)
@Preview(name = "Small screen", widthDp = 320, heightDp = 568)
@Preview(name = "Large text", widthDp = 360, heightDp = 800, fontScale = 1.5f)
@Composable
private fun ProfilePreview(@PreviewParameter(ProfilePreviewProvider::class) state: ProfileUiState) {
    NuvyroTheme {
        ProfileScreen(state, onSave = {}, onBack = {}) {
            ProfileForm(state, onNameChange = {}, onChoosePhoto = {}, onRemovePhoto = {}, onRetry = {})
        }
    }
}
