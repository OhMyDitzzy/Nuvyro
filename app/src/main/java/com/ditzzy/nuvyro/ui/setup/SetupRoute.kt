package com.ditzzy.nuvyro.ui.setup

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Intent
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.ditzzy.nuvyro.core.background.BackgroundActivitySettings
import com.ditzzy.nuvyro.core.permissions.NotificationPermission
import com.ditzzy.nuvyro.core.storage.ReceiveFolderContract

@Composable
fun SetupRoute(state: SetupUiState, viewModel: SetupViewModel) {
    val context = LocalContext.current
    var notificationsEnabled by remember { mutableStateOf(NotificationPermission.isEnabled(context)) }
    var batteryOptimizationDisabled by remember {
        mutableStateOf(BackgroundActivitySettings.isBatteryOptimizationDisabled(context))
    }
    var notificationDeclined by rememberSaveable { mutableStateOf(false) }
    val notificationLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        notificationsEnabled = NotificationPermission.isEnabled(context)
        notificationDeclined = !granted
    }
    val folderLauncher = rememberLauncherForActivityResult(ReceiveFolderContract()) { selection ->
        if (selection != null) viewModel.selectFolder(selection)
    }
    val settingsLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) {
        notificationsEnabled = NotificationPermission.isEnabled(context)
        batteryOptimizationDisabled =
            BackgroundActivitySettings.isBatteryOptimizationDisabled(context)
    }
    val openSettings: (Intent) -> Unit = { intent ->
        viewModel.clearError()
        try {
            settingsLauncher.launch(intent)
        } catch (_: ActivityNotFoundException) {
            viewModel.showSettingsError()
        } catch (_: SecurityException) {
            viewModel.showSettingsError()
        }
    }
    LifecycleResumeEffect(Unit) {
        notificationsEnabled = NotificationPermission.isEnabled(context)
        batteryOptimizationDisabled =
            BackgroundActivitySettings.isBatteryOptimizationDisabled(context)
        onPauseOrDispose { }
    }
    SetupScreen(
        state = state,
        notificationsEnabled = notificationsEnabled,
        notificationDeclined = notificationDeclined,
        batteryOptimizationDisabled = batteryOptimizationDisabled,
        onNotificationsClick = {
            viewModel.clearError()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !notificationDeclined) {
                notificationLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
            } else {
                openSettings(NotificationPermission.settingsIntent(context))
            }
        },
        onBackgroundClick = { openSettings(BackgroundActivitySettings.settingsIntent()) },
        onFolderClick = {
            viewModel.clearError()
            try {
                folderLauncher.launch(state.folder?.uri)
            } catch (_: ActivityNotFoundException) {
                viewModel.showFolderError()
            } catch (_: SecurityException) {
                viewModel.showFolderError()
            }
        },
        onNext = viewModel::next,
        onBack = viewModel::back
    )
}
