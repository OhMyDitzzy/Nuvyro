package com.ditzzy.nuvyro.ui.send

import android.content.ActivityNotFoundException
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable

@Composable
fun SendRoute(state: SendUiState, viewModel: SendViewModel) {
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
        SendStep.Receiver -> ReceiverScreen(
            state = state,
            onEditFiles = viewModel::showFiles,
            onBack = viewModel::back,
            onSelectReceiver = viewModel::selectReceiver
        )
    }
}
