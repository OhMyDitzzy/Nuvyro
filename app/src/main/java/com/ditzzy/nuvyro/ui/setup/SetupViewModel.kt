package com.ditzzy.nuvyro.ui.setup

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ditzzy.nuvyro.core.storage.ReceiveFolderAccess
import com.ditzzy.nuvyro.core.storage.ReceiveFolderSelection
import com.ditzzy.nuvyro.data.setup.SetupPreferences
import com.ditzzy.nuvyro.data.setup.SetupProgress
import com.ditzzy.nuvyro.data.setup.SetupStep
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class SetupViewModel(
    private val preferences: SetupPreferences,
    private val folderAccess: ReceiveFolderAccess
) : ViewModel() {
    private val mutableState = MutableStateFlow(SetupUiState())
    val state = mutableState.asStateFlow()

    init {
        load()
    }

    fun load() {
        mutableState.value = SetupUiState()
        viewModelScope.launch {
            try {
                val progress = preferences.load()
                val folder = if (progress.completed) null
                    else folderAccess.restore(progress.receiveFolderUri)
                mutableState.value = SetupUiState(
                    loading = false,
                    step = progress.step,
                    folder = folder,
                    completed = progress.completed
                )
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                mutableState.value = SetupUiState(error = SetupError.SaveFailed)
            }
        }
    }

    fun next() {
        val current = state.value
        if (current.step == SetupStep.ReceiveFolder) {
            finish()
        } else {
            saveStep(SetupStep.entries[current.step.ordinal + 1])
        }
    }

    fun back() {
        val previous = SetupStep.entries.getOrNull(state.value.step.ordinal - 1) ?: return
        saveStep(previous)
    }

    fun selectFolder(selection: ReceiveFolderSelection) {
        val current = state.value
        if (current.loading || current.saving || current.completed) return
        mutableState.update { it.copy(saving = true, error = null) }
        viewModelScope.launch {
            try {
                val folder = folderAccess.persist(selection)
                try {
                    preferences.save(SetupProgress(current.step, folder.uri.toString()))
                } catch (error: CancellationException) {
                    throw error
                } catch (_: Exception) {
                    if (folder.uri != current.folder?.uri) folderAccess.release(folder.uri)
                    mutableState.update { it.copy(saving = false, error = SetupError.SaveFailed) }
                    return@launch
                }
                mutableState.update { it.copy(folder = folder, saving = false) }
                current.folder?.uri?.takeIf { it != folder.uri }?.let { folderAccess.release(it) }
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                mutableState.update { it.copy(saving = false, error = SetupError.FolderUnavailable) }
            }
        }
    }

    fun showSettingsError() {
        mutableState.update { it.copy(error = SetupError.SettingsUnavailable) }
    }

    fun showFolderError() {
        mutableState.update { it.copy(error = SetupError.FolderUnavailable) }
    }

    fun clearError() {
        mutableState.update { it.copy(error = null) }
    }

    private fun saveStep(step: SetupStep) {
        val current = state.value
        if (current.loading || current.saving || current.completed) return
        mutableState.update { it.copy(saving = true, error = null) }
        viewModelScope.launch {
            try {
                preferences.save(SetupProgress(step, current.folder?.uri?.toString()))
                mutableState.update { it.copy(step = step, saving = false) }
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                mutableState.update { it.copy(saving = false, error = SetupError.SaveFailed) }
            }
        }
    }

    private fun finish() {
        val current = state.value
        if (current.loading || current.saving || current.folder == null || current.completed) return
        mutableState.update { it.copy(saving = true, error = null) }
        viewModelScope.launch {
            try {
                val folder = folderAccess.restore(current.folder.uri.toString())
                if (folder == null) {
                    mutableState.update {
                        it.copy(folder = null, saving = false, error = SetupError.FolderUnavailable)
                    }
                    return@launch
                }
                preferences.save(
                    SetupProgress(SetupStep.ReceiveFolder, folder.uri.toString(), completed = true)
                )
                mutableState.update { it.copy(folder = folder, completed = true, saving = false) }
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                mutableState.update { it.copy(saving = false, error = SetupError.SaveFailed) }
            }
        }
    }
}
