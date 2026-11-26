package com.ditzzy.nuvyro.data.setup

enum class SetupStep {
    Welcome,
    Profile,
    Notifications,
    Background,
    ReceiveFolder
}

data class SetupProgress(
    val step: SetupStep = SetupStep.Welcome,
    val receiveFolderUri: String? = null,
    val completed: Boolean = false
)
