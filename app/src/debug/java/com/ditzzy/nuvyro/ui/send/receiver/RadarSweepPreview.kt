package com.ditzzy.nuvyro.ui.send.receiver

import android.content.res.Configuration
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.ui.send.receiver.components.RadarBackground
import com.ditzzy.nuvyro.ui.theme.NuvyroTheme

@Preview(name = "Sweep and afterglow light", widthDp = 360, heightDp = 360)
@Preview(
    name = "Sweep and afterglow dark", uiMode = Configuration.UI_MODE_NIGHT_YES,
    widthDp = 360, heightDp = 360
)
@Composable
private fun RadarSweepPreview() {
    NuvyroTheme {
        Surface {
            RadarBackground(scanning = true, modifier = Modifier.fillMaxSize().padding(16.dp))
        }
    }
}

@Preview(name = "Idle radar light", widthDp = 360, heightDp = 360)
@Preview(
    name = "Idle radar dark", uiMode = Configuration.UI_MODE_NIGHT_YES,
    widthDp = 360, heightDp = 360
)
@Composable
private fun IdleRadarPreview() {
    NuvyroTheme {
        Surface {
            RadarBackground(scanning = false, modifier = Modifier.fillMaxSize().padding(16.dp))
        }
    }
}
