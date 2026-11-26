package com.ditzzy.nuvyro.ui.send.receiver

import android.graphics.Bitmap

enum class ReceiverDiscoveryStatus {
    Unavailable,
    Searching,
    Paused,
    Failed
}

data class ReceiverItem(
    val id: String,
    val name: String,
    val photo: Bitmap? = null
)

data class ReceiverDiscoveryUiState(
    val status: ReceiverDiscoveryStatus = ReceiverDiscoveryStatus.Unavailable,
    val receivers: List<ReceiverItem> = emptyList(),
    val selectedId: String? = null
) {
    val selectedReceiver: ReceiverItem?
        get() = receivers.firstOrNull { it.id == selectedId }
}
