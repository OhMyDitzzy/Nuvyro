package com.ditzzy.nuvyro.ui.lan

import androidx.compose.runtime.Composable
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.ditzzy.nuvyro.data.lan.LanMode

@Composable
fun LanSessionEffect(mode: LanMode, permissionGranted: Boolean, viewModel: LanViewModel) {
    LifecycleResumeEffect(mode, permissionGranted) {
        viewModel.start(mode)
        onPauseOrDispose { viewModel.pause() }
    }
}
