package com.ditzzy.nuvyro.ui.hotspot

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.hotspot.HotspotCredentials

@Composable
internal fun HotspotCredentialsCard(credentials: HotspotCredentials) {
    Surface(
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surfaceContainer,
        modifier = Modifier.fillMaxWidth()
    ) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            HotspotWifiQr(credentials)
            SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(stringResource(R.string.hotspot_network_name, credentials.ssid), style = MaterialTheme.typography.bodyLarge)
                    Text(stringResource(R.string.hotspot_password, credentials.password), style = MaterialTheme.typography.bodyLarge)
                }
            }
        }
    }
}
