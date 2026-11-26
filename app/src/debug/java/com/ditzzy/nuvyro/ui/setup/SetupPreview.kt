package com.ditzzy.nuvyro.ui.setup

import android.content.res.Configuration
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import com.ditzzy.nuvyro.core.storage.ReceiveFolder
import com.ditzzy.nuvyro.data.setup.SetupStep
import com.ditzzy.nuvyro.ui.theme.NuvyroTheme

class SetupPreviewProvider : PreviewParameterProvider<SetupUiState> {
    override val values: Sequence<SetupUiState> = sequenceOf(
        SetupUiState(loading = false, step = SetupStep.Welcome),
        SetupUiState(loading = false, step = SetupStep.Notifications),
        SetupUiState(loading = false, step = SetupStep.Background),
        SetupUiState(loading = false, step = SetupStep.ReceiveFolder),
        SetupUiState(
            loading = false,
            step = SetupStep.ReceiveFolder,
            folder = ReceiveFolder(Uri.parse("content://preview/tree/nuvyro"), "Nuvyro")
        ),
        SetupUiState(
            loading = false,
            step = SetupStep.ReceiveFolder,
            error = SetupError.FolderUnavailable
        )
    )
}

@Preview(name = "Light", widthDp = 360, heightDp = 800)
@Preview(name = "Dark", uiMode = Configuration.UI_MODE_NIGHT_YES, widthDp = 360, heightDp = 800)
@Preview(name = "Small screen", widthDp = 320, heightDp = 568)
@Preview(name = "Large text", widthDp = 360, heightDp = 800, fontScale = 1.5f)
@Composable
private fun SetupPreview(@PreviewParameter(SetupPreviewProvider::class) state: SetupUiState) {
    NuvyroTheme {
        SetupScreen(
            state = state,
            notificationsEnabled = false,
            notificationDeclined = false,
            batteryOptimizationDisabled = false,
            onNotificationsClick = {},
            onBackgroundClick = {},
            onFolderClick = {},
            onNext = {},
            onBack = {}
        )
    }
}
