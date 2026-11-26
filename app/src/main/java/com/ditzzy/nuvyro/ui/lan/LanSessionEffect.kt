package com.ditzzy.nuvyro.ui.lan

import androidx.compose.runtime.Composable
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.ditzzy.nuvyro.data.lan.LanMode

@Composable
fun LanSessionEffect(mode: LanMode, permissionGranted: Boolean, viewModel: LanViewModel, enabled: Boolean = true) {
    LifecycleResumeEffect(mode, permissionGranted, enabled) {
        if (enabled) viewModel.start(mode) else viewModel.pause()
        onPauseOrDispose { viewModel.pause() }
    }
}
