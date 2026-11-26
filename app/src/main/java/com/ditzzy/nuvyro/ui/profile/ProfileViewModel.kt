package com.ditzzy.nuvyro.ui.profile

import android.net.Uri
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ditzzy.nuvyro.core.profile.ProfilePhotoStore
import com.ditzzy.nuvyro.data.profile.LocalProfile
import com.ditzzy.nuvyro.data.profile.ProfileName
import com.ditzzy.nuvyro.data.profile.ProfilePreferences
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class ProfileViewModel(
    private val preferences: ProfilePreferences,
    private val photos: ProfilePhotoStore,
    private val savedStateHandle: SavedStateHandle
) : ViewModel() {
    private val mutableState = MutableStateFlow(
        ProfileUiState(editing = savedStateHandle.get<Boolean>("editing") ?: false)
    )
    val state = mutableState.asStateFlow()
    private var photoChanged = false
    private var pendingPhoto: Uri? = null

    init {
        load()
    }

    fun load() {
        if (state.value.busy) return
        mutableState.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                val stored = preferences.load()
                val photo = photos.load(stored.photoFileName)
                val profile = stored.copy(photoFileName = stored.photoFileName.takeIf { photo != null })
                cleanUpPhotos(profile.photoFileName)
                photoChanged = false
                mutableState.update {
                    it.copy(
                        loading = false, profile = profile, savedPhoto = photo,
                        name = savedStateHandle.get<String>("draft_name") ?: profile.displayName,
                        photo = photo, nameInvalid = false
                    )
                }
                pendingPhoto?.let { uri ->
                    pendingPhoto = null
                    selectPhoto(uri)
                }
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                mutableState.update { it.copy(loading = false, error = ProfileError.LoadFailed) }
            }
        }
    }

    fun beginEditing() {
        if (state.value.busy) return
        discardChanges()
        savedStateHandle["editing"] = true
        mutableState.update { it.copy(editing = true) }
    }

    fun changeName(name: String) {
        if (state.value.busy || state.value.loading) return
        savedStateHandle["draft_name"] = name
        mutableState.update { it.copy(name = name, nameInvalid = false, error = null) }
    }

    fun selectPhoto(uri: Uri) {
        if (state.value.loading) {
            pendingPhoto = uri
            return
        }
        if (state.value.busy || state.value.profile == null) return
        mutableState.update { it.copy(busy = true, error = null) }
        viewModelScope.launch {
            try {
                val photo = photos.prepare(uri)
                photoChanged = true
                mutableState.update { it.copy(photo = photo, busy = false) }
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                mutableState.update { it.copy(busy = false, error = ProfileError.PhotoUnavailable) }
            }
        }
    }

    fun removePhoto() {
        if (state.value.busy || state.value.loading) return
        photoChanged = true
        mutableState.update { it.copy(photo = null, error = null) }
    }

    fun showPhotoError() {
        mutableState.update { it.copy(error = ProfileError.PhotoUnavailable) }
    }

    fun discardChanges() {
        if (state.value.busy) return
        savedStateHandle["editing"] = false
        savedStateHandle.remove<String>("draft_name")
        pendingPhoto = null
        photoChanged = false
        mutableState.update {
            it.copy(
                name = it.profile?.displayName.orEmpty(), photo = it.savedPhoto,
                nameInvalid = false, error = if (it.profile == null) it.error else null, editing = false
            )
        }
    }

    fun save(onSaved: () -> Unit = {}) {
        val current = state.value
        val previous = current.profile ?: return
        if (current.busy || current.loading) return
        val name = ProfileName.normalize(current.name)
        if (!ProfileName.isValid(name)) {
            mutableState.update { it.copy(nameInvalid = true) }
            return
        }
        mutableState.update { it.copy(busy = true, error = null) }
        viewModelScope.launch {
            try {
                val fileName = if (photoChanged) current.photo?.let { photos.save(it) }
                    else previous.photoFileName
                val profile = LocalProfile(name, fileName)
                preferences.save(profile)
                cleanUpPhotos(fileName)
                photoChanged = false
                savedStateHandle["editing"] = false
                savedStateHandle.remove<String>("draft_name")
                mutableState.update {
                    it.copy(
                        profile = profile, savedPhoto = current.photo, name = name,
                        busy = false, editing = false, nameInvalid = false
                    )
                }
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                cleanUpPhotos(previous.photoFileName)
                mutableState.update { it.copy(busy = false, error = ProfileError.SaveFailed) }
                return@launch
            }
            onSaved()
        }
    }

    private suspend fun cleanUpPhotos(keep: String?) {
        try {
            photos.removeUnused(keep)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // Photo cleanup can be retried the next time the profile is loaded.
        }
    }
}
