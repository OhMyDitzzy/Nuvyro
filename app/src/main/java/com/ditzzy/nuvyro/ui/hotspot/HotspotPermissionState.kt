package com.ditzzy.nuvyro.ui.hotspot

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
import com.ditzzy.nuvyro.core.hotspot.HotspotPermission

data class HotspotPermissionState(
    val granted: Boolean,
    val locationReady: Boolean,
    val declined: Boolean,
    val settingsUnavailable: Boolean,
    val request: () -> Unit,
    val openLocation: () -> Unit,
    val openWifi: () -> Unit
)

@Composable
fun rememberHotspotPermissionState(): HotspotPermissionState {
    val context = LocalContext.current
    var granted by remember { mutableStateOf(HotspotPermission.isGranted(context)) }
    var locationReady by remember { mutableStateOf(HotspotPermission.locationReady(context)) }
    var declined by rememberSaveable { mutableStateOf(false) }
    var unavailable by remember { mutableStateOf(false) }
    val permissions = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        granted = HotspotPermission.isGranted(context)
        declined = !granted
    }
    val settings = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        granted = HotspotPermission.isGranted(context)
        locationReady = HotspotPermission.locationReady(context)
    }
    val open: (Intent) -> Unit = { intent ->
        unavailable = false
        try { settings.launch(intent) }
        catch (_: ActivityNotFoundException) { unavailable = true }
        catch (_: SecurityException) { unavailable = true }
    }
    LifecycleResumeEffect(Unit) {
        granted = HotspotPermission.isGranted(context)
        locationReady = HotspotPermission.locationReady(context)
        onPauseOrDispose { }
    }
    return HotspotPermissionState(
        granted, locationReady, declined, unavailable,
        request = {
            if (declined) {
                open(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${context.packageName}")))
            } else {
                val required = if (Build.VERSION.SDK_INT >= 33) arrayOf(Manifest.permission.NEARBY_WIFI_DEVICES)
                    else arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION, Manifest.permission.ACCESS_FINE_LOCATION)
                permissions.launch(required)
            }
        },
        openLocation = { open(Intent(Settings.ACTION_LOCATION_SOURCE_SETTINGS)) },
        openWifi = { open(Intent(Settings.ACTION_WIFI_SETTINGS)) }
    )
}
