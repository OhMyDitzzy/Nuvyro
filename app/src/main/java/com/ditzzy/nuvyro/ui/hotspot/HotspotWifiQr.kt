package com.ditzzy.nuvyro.ui.hotspot

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.core.hotspot.HotspotQrCode
import com.ditzzy.nuvyro.data.hotspot.HotspotCredentials
import com.google.zxing.common.BitMatrix
import kotlin.math.floor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
internal fun HotspotWifiQr(credentials: HotspotCredentials) {
    val preview = LocalInspectionMode.current
    val initial = remember(credentials, preview) {
        if (preview) runCatching { HotspotQrCode.encode(credentials) } else null
    }
    val result by produceState<Result<BitMatrix>?>(initial, credentials) {
        value = null
        value = withContext(Dispatchers.Default) { runCatching { HotspotQrCode.encode(credentials) } }
    }
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(stringResource(R.string.hotspot_qr_title), style = MaterialTheme.typography.titleMedium)
        val matrix = result?.getOrNull()
        if (matrix != null) {
            val description = stringResource(R.string.hotspot_qr_description, credentials.ssid)
            Canvas(
                Modifier.widthIn(max = 280.dp).fillMaxWidth().aspectRatio(1f)
                    .semantics { contentDescription = description }
            ) {
                drawRect(Color.White)
                val cell = floor(size.minDimension / matrix.width).coerceAtLeast(1f)
                val left = floor((size.width - cell * matrix.width) / 2)
                val top = floor((size.height - cell * matrix.height) / 2)
                for (y in 0 until matrix.height) {
                    for (x in 0 until matrix.width) {
                        if (matrix[x, y]) {
                            drawRect(Color.Black, Offset(left + x * cell, top + y * cell), Size(cell, cell))
                        }
                    }
                }
            }
        } else if (result == null) {
            Box(Modifier.widthIn(max = 280.dp).fillMaxWidth().aspectRatio(1f), contentAlignment = Alignment.Center) {
                CircularProgressIndicator()
            }
        } else {
            Text(stringResource(R.string.hotspot_qr_unavailable), style = MaterialTheme.typography.bodyMedium)
        }
        Text(stringResource(R.string.hotspot_qr_hint), style = MaterialTheme.typography.bodyMedium)
    }
}
