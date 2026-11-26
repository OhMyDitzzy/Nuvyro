package com.ditzzy.nuvyro.data.setup

enum class SetupStep {
    Welcome,
    Notifications,
    Background,
    ReceiveFolder
}

data class SetupProgress(
    val step: SetupStep = SetupStep.Welcome,
    val receiveFolderUri: String? = null,
    val completed: Boolean = false
)
