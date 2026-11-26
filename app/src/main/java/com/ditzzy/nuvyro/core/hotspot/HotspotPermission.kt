package com.ditzzy.nuvyro.core.hotspot

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.core.location.LocationManagerCompat

object HotspotPermission {
    val supported: Boolean get() = Build.VERSION.SDK_INT >= 26
    val required: String get() = if (Build.VERSION.SDK_INT >= 33) Manifest.permission.NEARBY_WIFI_DEVICES
        else Manifest.permission.ACCESS_FINE_LOCATION

    fun isGranted(context: Context): Boolean =
        ContextCompat.checkSelfPermission(context, required) == PackageManager.PERMISSION_GRANTED

    fun locationReady(context: Context): Boolean {
        if (Build.VERSION.SDK_INT >= 33) return true
        val manager = context.getSystemService(LocationManager::class.java) ?: return false
        return LocationManagerCompat.isLocationEnabled(manager)
    }
}
