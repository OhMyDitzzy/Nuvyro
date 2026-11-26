package com.ditzzy.nuvyro.ui.setup

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.setup.SetupStep

@get:StringRes
internal val SetupStep.title: Int
    get() = when (this) {
        SetupStep.Welcome -> R.string.setup_welcome_title
        SetupStep.Profile -> R.string.setup_profile_title
        SetupStep.Notifications -> R.string.setup_notifications_title
        SetupStep.Background -> R.string.setup_background_title
        SetupStep.ReceiveFolder -> R.string.setup_folder_title
    }

@get:StringRes
internal val SetupStep.description: Int
    get() = when (this) {
        SetupStep.Welcome -> R.string.setup_welcome_description
        SetupStep.Profile -> R.string.setup_profile_description
        SetupStep.Notifications -> R.string.setup_notifications_description
        SetupStep.Background -> R.string.setup_background_description
        SetupStep.ReceiveFolder -> R.string.setup_folder_description
    }

@get:DrawableRes
internal val SetupStep.icon: Int
    get() = when (this) {
        SetupStep.Welcome -> R.drawable.ic_transfer
        SetupStep.Profile -> R.drawable.ic_profile
        SetupStep.Notifications -> R.drawable.ic_notifications
        SetupStep.Background -> R.drawable.ic_battery
        SetupStep.ReceiveFolder -> R.drawable.ic_folder
    }
