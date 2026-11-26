package com.ditzzy.nuvyro.ui.receive

import android.graphics.Bitmap
import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import com.ditzzy.nuvyro.data.lan.LanMode
import com.ditzzy.nuvyro.data.lan.LanState
import com.ditzzy.nuvyro.ui.lan.LanActions
import com.ditzzy.nuvyro.ui.lan.LanSessionEffect
import com.ditzzy.nuvyro.ui.lan.LanViewModel
import com.ditzzy.nuvyro.ui.lan.rememberLanPermissionState

@Composable
fun ReceiveRoute(state: LanState, name: String, photo: Bitmap?, viewModel: LanViewModel) {
    val permission = rememberLanPermissionState()
    LanSessionEffect(LanMode.Advertise, permission.granted, viewModel)
    BackHandler(onBack = viewModel::closeReceive)
    ReceiveScreen(state.status, name, photo, onStop = viewModel::closeReceive) {
        LanActions(state.status, permission, viewModel::retry)
    }
}
