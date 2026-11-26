package com.ditzzy.nuvyro.ui.hotspot

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R

@Composable
internal fun HotspotCreateAction(permission: HotspotPermissionState, onCreate: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (!permission.granted) Text(stringResource(R.string.hotspot_permission_hint))
        if (!permission.locationReady && permission.granted) Text(stringResource(R.string.hotspot_location_hint))
        val label = when {
            !permission.granted && permission.declined -> R.string.lan_open_settings
            !permission.granted -> R.string.hotspot_allow
            !permission.locationReady -> R.string.hotspot_enable_location
            else -> R.string.hotspot_create
        }
        FilledTonalButton(
            onClick = {
                when {
                    !permission.granted -> permission.request()
                    !permission.locationReady -> permission.openLocation()
                    else -> onCreate()
                }
            },
            modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
        ) { Text(stringResource(label)) }
    }
}
