package com.ditzzy.nuvyro.ui.send

import com.ditzzy.nuvyro.data.send.OutgoingFile

enum class SendStep {
    Files,
    Receiver
}

data class SendUiState(
    val active: Boolean = false,
    val step: SendStep = SendStep.Files,
    val files: List<OutgoingFile> = emptyList(),
    val loading: Boolean = false,
    val picking: Boolean = false,
    val pickerUnavailable: Boolean = false,
    val unavailableCount: Int = 0
) {
    val busy: Boolean get() = loading || picking
}
