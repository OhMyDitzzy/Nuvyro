package com.ditzzy.nuvyro.ui.receive

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.lan.LanStatus
import com.ditzzy.nuvyro.ui.profile.components.ProfileAvatar
import com.ditzzy.nuvyro.ui.send.components.SendScaffold

@Composable
fun ReceiveScreen(
    status: LanStatus,
    name: String,
    photo: Bitmap?,
    onStop: () -> Unit,
    actions: @Composable () -> Unit = {}
) {
    SendScaffold(
        title = stringResource(R.string.home_receive),
        onBack = onStop,
        bottomBar = {
            Button(onClick = onStop, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)) {
                Text(stringResource(R.string.receive_stop))
            }
        }
    ) { innerPadding ->
        Box(Modifier.fillMaxSize().padding(innerPadding), contentAlignment = Alignment.TopCenter) {
            Column(
                modifier = Modifier.widthIn(max = 568.dp).fillMaxSize()
                    .verticalScroll(rememberScrollState()).padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(24.dp)
            ) {
                ProfileAvatar(name, photo, Modifier.size(96.dp))
                Text(name, style = MaterialTheme.typography.headlineSmall)
                if (status == LanStatus.Starting) CircularProgressIndicator()
                Column(
                    modifier = Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite },
                    verticalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    Text(
                        text = stringResource(when (status) {
                            LanStatus.Active -> R.string.receive_available
                            LanStatus.Starting -> R.string.lan_starting
                            LanStatus.PermissionRequired -> R.string.lan_permission_title
                            LanStatus.NoWifi -> R.string.lan_wifi_title
                            LanStatus.Paused -> R.string.receive_paused
                            LanStatus.Failed -> R.string.receive_failed
                        }),
                        style = MaterialTheme.typography.titleLarge
                    )
                    Text(
                        text = stringResource(when (status) {
                            LanStatus.PermissionRequired -> R.string.lan_permission_hint
                            LanStatus.NoWifi -> R.string.lan_wifi_hint
                            LanStatus.Failed -> R.string.receive_failed_hint
                            else -> R.string.receive_visibility_hint
                        }),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    actions()
                    Text(
                        text = stringResource(R.string.receive_transfer_unavailable),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        }
    }
}
