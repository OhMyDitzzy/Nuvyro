package com.ditzzy.nuvyro.ui.lan

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
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
import com.ditzzy.nuvyro.core.lan.LanPermission

data class LanPermissionState(
    val granted: Boolean,
    val declined: Boolean,
    val settingsUnavailable: Boolean,
    val request: () -> Unit
)

@Composable
fun rememberLanPermissionState(): LanPermissionState {
    val context = LocalContext.current
    var granted by remember { mutableStateOf(LanPermission.isGranted(context)) }
    var declined by rememberSaveable { mutableStateOf(false) }
    var unavailable by remember { mutableStateOf(false) }
    val permissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        granted = LanPermission.isGranted(context)
        declined = !granted
    }
    val settingsLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        granted = LanPermission.isGranted(context)
    }
    LifecycleResumeEffect(Unit) {
        granted = LanPermission.isGranted(context)
        onPauseOrDispose { }
    }
    return LanPermissionState(granted, declined, unavailable) {
        unavailable = false
        try {
            if (Build.VERSION.SDK_INT >= 37 && !declined && !granted) {
                permissionLauncher.launch(Manifest.permission.ACCESS_LOCAL_NETWORK)
            } else {
                settingsLauncher.launch(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                    data = Uri.parse("package:${context.packageName}")
                })
            }
        } catch (_: ActivityNotFoundException) {
            unavailable = true
        } catch (_: SecurityException) {
            unavailable = true
        }
    }
}
