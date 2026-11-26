package com.ditzzy.nuvyro.ui.send

import android.net.Uri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ditzzy.nuvyro.core.storage.OutgoingFileAccess
import com.ditzzy.nuvyro.data.send.FileSelection
import com.ditzzy.nuvyro.data.send.OutgoingFile
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

class SendViewModel(
    private val fileAccess: OutgoingFileAccess,
    private val savedStateHandle: SavedStateHandle
) : ViewModel() {
    private val restoredUris = savedStateHandle.get<ArrayList<String>>("file_uris")
        .orEmpty().map(Uri::parse)
    private val mutableState = MutableStateFlow(SendUiState(
        active = savedStateHandle.get<Boolean>("active") ?: false,
        step = if (restoredUris.isEmpty()) SendStep.Files else SendStep.entries.firstOrNull {
            it.name == savedStateHandle.get<String>("step")
        } ?: SendStep.Files,
        loading = restoredUris.isNotEmpty(),
        picking = savedStateHandle.get<Boolean>("picking") ?: false
    ))
    val state = mutableState.asStateFlow()
    private val selectionMutex = Mutex()
    private var pendingReads = 0

    init {
        if (restoredUris.isNotEmpty()) readFiles(restoredUris, restoring = true)
    }

    fun open() {
        savedStateHandle["active"] = true
        mutableState.update { it.copy(active = true) }
    }

    fun back() {
        if (state.value.step == SendStep.Receiver) {
            showFiles()
        } else {
            savedStateHandle["active"] = false
            mutableState.update { it.copy(active = false) }
        }
    }

    fun showFiles() {
        savedStateHandle["step"] = SendStep.Files.name
        mutableState.update { it.copy(step = SendStep.Files) }
    }

    fun chooseReceiver() {
        val current = state.value
        if (current.busy || current.files.isEmpty()) return
        savedStateHandle["step"] = SendStep.Receiver.name
        mutableState.update { it.copy(step = SendStep.Receiver) }
    }

    fun beginPicking(): Boolean {
        if (state.value.busy) return false
        savedStateHandle["picking"] = true
        mutableState.update { it.copy(picking = true, pickerUnavailable = false) }
        return true
    }

    fun pickerFailed() {
        savedStateHandle["picking"] = false
        mutableState.update { it.copy(picking = false, pickerUnavailable = true) }
    }

    fun filesPicked(uris: List<Uri>) {
        savedStateHandle["picking"] = false
        mutableState.update { it.copy(picking = false) }
        if (uris.isNotEmpty()) readFiles(uris)
    }

    fun removeFile(uri: Uri) {
        if (state.value.busy) return
        val files = state.value.files.filterNot { it.uri == uri }
        rememberFiles(files)
        mutableState.update { it.copy(files = files, unavailableCount = 0) }
    }

    private fun readFiles(uris: List<Uri>, restoring: Boolean = false) {
        pendingReads++
        mutableState.update { it.copy(loading = true, pickerUnavailable = false) }
        viewModelScope.launch {
            try {
                selectionMutex.withLock {
                    val result = fileAccess.read(uris)
                    val files = if (restoring) result.files
                        else FileSelection.merge(state.value.files, result.files)
                    rememberFiles(files)
                    if (files.isEmpty() || (restoring && result.unavailableCount > 0)) showFiles()
                    mutableState.update {
                        it.copy(files = files, unavailableCount = result.unavailableCount)
                    }
                }
            } finally {
                pendingReads--
                mutableState.update { it.copy(loading = pendingReads > 0) }
            }
        }
    }

    private fun rememberFiles(files: List<OutgoingFile>) {
        savedStateHandle["file_uris"] = ArrayList(files.map { it.uri.toString() })
    }
}
