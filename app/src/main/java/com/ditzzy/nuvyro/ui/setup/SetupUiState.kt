package com.ditzzy.nuvyro.ui.setup

import com.ditzzy.nuvyro.core.storage.ReceiveFolder
import com.ditzzy.nuvyro.data.setup.SetupStep

enum class SetupError {
    SaveFailed,
    FolderUnavailable,
    SettingsUnavailable
}

data class SetupUiState(
    val loading: Boolean = true,
    val step: SetupStep = SetupStep.Welcome,
    val folder: ReceiveFolder? = null,
    val completed: Boolean = false,
    val saving: Boolean = false,
    val error: SetupError? = null
)
