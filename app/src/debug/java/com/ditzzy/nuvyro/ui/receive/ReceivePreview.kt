package com.ditzzy.nuvyro.ui.receive

import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import com.ditzzy.nuvyro.data.lan.LanStatus
import com.ditzzy.nuvyro.data.hotspot.HotspotCredentials
import com.ditzzy.nuvyro.data.hotspot.HotspotState
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus
import com.ditzzy.nuvyro.ui.lan.LanActions
import com.ditzzy.nuvyro.ui.lan.LanPermissionState
import com.ditzzy.nuvyro.ui.theme.NuvyroTheme

class ReceivePreviewProvider : PreviewParameterProvider<LanStatus> {
    override val values = LanStatus.entries.asSequence()
}

class ReceiveHotspotPreviewProvider : PreviewParameterProvider<HotspotState> {
    override val values = sequenceOf(
        HotspotState(HotspotStatus.Starting),
        HotspotState(HotspotStatus.Active, HotspotCredentials("Nuvyro preview network", "sample-only-123")),
        HotspotState(HotspotStatus.Paused),
        HotspotState(HotspotStatus.Failed)
    )
}

@Preview(name = "Receive hotspot light", widthDp = 360, heightDp = 800)
@Preview(name = "Receive hotspot dark", uiMode = Configuration.UI_MODE_NIGHT_YES, widthDp = 360, heightDp = 800)
@Preview(name = "Receive hotspot small screen", widthDp = 320, heightDp = 568)
@Preview(name = "Receive hotspot large text", widthDp = 360, heightDp = 800, fontScale = 1.5f)
@Composable
private fun ReceiveHotspotPreview(@PreviewParameter(ReceiveHotspotPreviewProvider::class) hotspot: HotspotState) {
    NuvyroTheme {
        ReceiveScreen(LanStatus.NoWifi, name = "Alex", photo = null, onStop = {}, hotspot = hotspot) {
            ReceiveHotspotCard(hotspot, onShowDetails = {}, onStop = {})
        }
    }
}

@Preview(name = "Receive light", widthDp = 360, heightDp = 800)
@Preview(name = "Receive dark", uiMode = Configuration.UI_MODE_NIGHT_YES, widthDp = 360, heightDp = 800)
@Preview(name = "Receive small screen", widthDp = 320, heightDp = 568)
@Preview(name = "Receive large text", widthDp = 360, heightDp = 800, fontScale = 1.5f)
@Composable
private fun ReceivePreview(@PreviewParameter(ReceivePreviewProvider::class) status: LanStatus) {
    NuvyroTheme {
        ReceiveScreen(status, name = "Alex", photo = null, onStop = {}) {
            LanActions(
                status, LanPermissionState(granted = false, declined = false, settingsUnavailable = false, request = {}),
                onRetry = {}
            )
        }
    }
}
