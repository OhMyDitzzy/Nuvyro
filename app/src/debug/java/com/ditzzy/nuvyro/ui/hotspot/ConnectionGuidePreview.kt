package com.ditzzy.nuvyro.ui.hotspot

import android.content.res.Configuration
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import com.ditzzy.nuvyro.data.hotspot.HotspotCredentials
import com.ditzzy.nuvyro.data.hotspot.HotspotState
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus
import com.ditzzy.nuvyro.ui.theme.NuvyroTheme

data class GuidePreviewState(
    val receiving: Boolean = true,
    val hotspot: HotspotState = HotspotState(),
    val supported: Boolean = true
)

class ConnectionGuidePreviewProvider : PreviewParameterProvider<GuidePreviewState> {
    override val values = sequenceOf(
        GuidePreviewState(),
        GuidePreviewState(hotspot = HotspotState(HotspotStatus.Starting)),
        GuidePreviewState(hotspot = HotspotState(
            HotspotStatus.Active, HotspotCredentials("Nuvyro preview network", "sample-only-123")
        )),
        GuidePreviewState(hotspot = HotspotState(HotspotStatus.Failed)),
        GuidePreviewState(hotspot = HotspotState(HotspotStatus.Paused)),
        GuidePreviewState(supported = false),
        GuidePreviewState(receiving = false)
    )
}

@Preview(name = "Connection guide light", widthDp = 360, heightDp = 800)
@Preview(name = "Connection guide dark", uiMode = Configuration.UI_MODE_NIGHT_YES, widthDp = 360, heightDp = 800)
@Preview(name = "Connection guide small screen", widthDp = 320, heightDp = 568)
@Preview(name = "Connection guide large text", widthDp = 360, heightDp = 800, fontScale = 1.5f)
@Composable
private fun ConnectionGuidePreview(@PreviewParameter(ConnectionGuidePreviewProvider::class) state: GuidePreviewState) {
    NuvyroTheme {
        ConnectionGuideScreen(
            state.receiving, state.hotspot, state.supported,
            onBack = {}, onOpenWifi = {}, onFindReceiver = {}
        ) {
            if (state.receiving && state.supported && state.hotspot.status != HotspotStatus.Active &&
                state.hotspot.status != HotspotStatus.Starting
            ) {
                FilledTonalButton(onClick = {}) { Text("Create hotspot") }
            }
        }
    }
}
