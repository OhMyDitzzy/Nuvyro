package com.ditzzy.nuvyro.ui.send

import android.content.res.Configuration
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.compose.ui.tooling.preview.PreviewParameterProvider
import com.ditzzy.nuvyro.data.send.OutgoingFile
import com.ditzzy.nuvyro.ui.send.receiver.ReceiverDiscoveryStatus
import com.ditzzy.nuvyro.ui.send.receiver.ReceiverDiscoveryUiState
import com.ditzzy.nuvyro.ui.send.receiver.previewReceivers
import com.ditzzy.nuvyro.ui.theme.NuvyroTheme

private val previewFiles = listOf(
    OutgoingFile(Uri.parse("content://preview/document/1"), "Weekend photos.zip", 24_500_000),
    OutgoingFile(Uri.parse("content://preview/document/2"), "Trip plans.pdf", 350_000),
    OutgoingFile(
        Uri.parse("content://preview/document/3"),
        "A document with a longer name to review before sharing.txt",
        null
    )
)

class SendPreviewProvider : PreviewParameterProvider<SendUiState> {
    override val values = sequenceOf(
        SendUiState(active = true),
        SendUiState(active = true, files = previewFiles.take(2)),
        SendUiState(active = true, files = previewFiles),
        SendUiState(active = true, files = previewFiles, loading = true),
        SendUiState(active = true, files = previewFiles.take(1), unavailableCount = 2),
        SendUiState(active = true, pickerUnavailable = true)
    )
}

@Preview(name = "Light", widthDp = 360, heightDp = 800)
@Preview(name = "Dark", uiMode = Configuration.UI_MODE_NIGHT_YES, widthDp = 360, heightDp = 800)
@Preview(name = "Small screen", widthDp = 320, heightDp = 568)
@Preview(name = "Large text", widthDp = 360, heightDp = 800, fontScale = 1.5f)
@Composable
private fun SendFilesPreview(@PreviewParameter(SendPreviewProvider::class) state: SendUiState) {
    NuvyroTheme {
        SendFilesScreen(
            state, onChooseFiles = {}, onRemoveFile = {}, onContinue = {}, onBack = {}
        )
    }
}

class ReceiverPreviewProvider : PreviewParameterProvider<ReceiverDiscoveryUiState> {
    override val values = sequenceOf(
        ReceiverDiscoveryUiState(),
        ReceiverDiscoveryUiState(status = ReceiverDiscoveryStatus.Searching),
        ReceiverDiscoveryUiState(
            status = ReceiverDiscoveryStatus.Searching,
            receivers = previewReceivers.take(3), selectedId = "maya"
        ),
        ReceiverDiscoveryUiState(status = ReceiverDiscoveryStatus.Searching, receivers = previewReceivers),
        ReceiverDiscoveryUiState(status = ReceiverDiscoveryStatus.Paused, receivers = previewReceivers.take(1)),
        ReceiverDiscoveryUiState(status = ReceiverDiscoveryStatus.Failed)
    )
}

@Preview(name = "Receiver light", widthDp = 360, heightDp = 800)
@Preview(name = "Receiver dark", uiMode = Configuration.UI_MODE_NIGHT_YES, widthDp = 360, heightDp = 800)
@Preview(name = "Receiver small screen", widthDp = 320, heightDp = 568)
@Preview(name = "Receiver large text", widthDp = 360, heightDp = 800, fontScale = 1.5f)
@Preview(name = "Receiver largest text", widthDp = 320, heightDp = 800, fontScale = 2f)
@Composable
private fun ReceiverPreview(
    @PreviewParameter(ReceiverPreviewProvider::class) initialState: ReceiverDiscoveryUiState
) {
    var discovery by remember(initialState) { mutableStateOf(initialState) }
    NuvyroTheme {
        ReceiverScreen(
            SendUiState(
                active = true, step = SendStep.Receiver, files = previewFiles.take(2),
                receiverDiscovery = discovery
            ),
            onEditFiles = {}, onBack = {},
            onSelectReceiver = { discovery = discovery.copy(selectedId = it) }
        )
    }
}
