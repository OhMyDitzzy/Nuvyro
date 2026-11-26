package com.ditzzy.nuvyro.ui.send

import android.content.ActivityNotFoundException
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import com.ditzzy.nuvyro.data.lan.LanState
import com.ditzzy.nuvyro.ui.lan.LanViewModel
import com.ditzzy.nuvyro.ui.send.receiver.ReceiverRoute

@Composable
fun SendRoute(state: SendUiState, viewModel: SendViewModel, lanState: LanState, lanViewModel: LanViewModel) {
    val fileLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) {
        viewModel.filesPicked(it)
    }
    BackHandler(onBack = viewModel::back)
    when (if (state.loading) SendStep.Files else state.step) {
        SendStep.Files -> SendFilesScreen(
            state = state,
            onChooseFiles = {
                if (viewModel.beginPicking()) {
                    try {
                        fileLauncher.launch(arrayOf("*/*"))
                    } catch (_: ActivityNotFoundException) {
                        viewModel.pickerFailed()
                    } catch (_: SecurityException) {
                        viewModel.pickerFailed()
                    }
                }
            },
            onRemoveFile = viewModel::removeFile,
            onContinue = viewModel::chooseReceiver,
            onBack = viewModel::back
        )
        SendStep.Receiver -> ReceiverRoute(state, viewModel, lanState, lanViewModel)
    }
}
