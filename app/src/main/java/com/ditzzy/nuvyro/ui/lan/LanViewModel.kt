package com.ditzzy.nuvyro.ui.lan

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ditzzy.nuvyro.core.lan.LanAdvertiser
import com.ditzzy.nuvyro.core.lan.LanBrowser
import com.ditzzy.nuvyro.core.lan.LanBrowserEvent
import com.ditzzy.nuvyro.core.lan.LanNetworkMonitor
import com.ditzzy.nuvyro.data.lan.LanMode
import com.ditzzy.nuvyro.data.lan.LanState
import com.ditzzy.nuvyro.data.lan.LanStatus
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class LanViewModel(
    private val networks: LanNetworkMonitor,
    private val browser: LanBrowser,
    private val advertiser: LanAdvertiser,
    private val hasPermission: () -> Boolean,
    private val savedStateHandle: SavedStateHandle
) : ViewModel() {
    private val mutableState = MutableStateFlow(
        LanState(receiving = savedStateHandle.get<Boolean>("receiving") ?: false)
    )
    val state = mutableState.asStateFlow()
    private var session: Job? = null
    private var mode: LanMode? = null

    fun openReceive() {
        savedStateHandle["receiving"] = true
        mutableState.update { it.copy(receiving = true) }
    }

    fun closeReceive() {
        pause()
        savedStateHandle["receiving"] = false
        mutableState.update { it.copy(receiving = false) }
    }

    fun start(mode: LanMode) {
        this.mode = mode
        val previous = session
        previous?.cancel()
        session = viewModelScope.launch {
            previous?.cancelAndJoin()
            mutableState.update { it.copy(status = LanStatus.Starting, receivers = emptyList()) }
            if (!hasPermission()) {
                mutableState.update { it.copy(status = LanStatus.PermissionRequired) }
                return@launch
            }
            try {
                networks.networks().collectLatest { network ->
                    mutableState.update {
                        it.copy(
                            status = if (network == null) LanStatus.NoWifi else LanStatus.Starting,
                            receivers = emptyList()
                        )
                    }
                    if (network == null) return@collectLatest
                    if (mode == LanMode.Advertise) {
                        advertiser.advertise(network) {
                            mutableState.update { it.copy(status = LanStatus.Active) }
                        }
                    } else {
                        browser.browse(network).collect { event ->
                            mutableState.update { current ->
                                when (event) {
                                    LanBrowserEvent.Started -> current.copy(status = LanStatus.Active)
                                    is LanBrowserEvent.Updated -> current.copy(
                                        receivers = (current.receivers + event.receiver)
                                            .associateBy { it.id }.values.toList()
                                    )
                                    is LanBrowserEvent.Lost -> current.copy(
                                        receivers = current.receivers.filterNot { it.id == event.id }
                                    )
                                }
                            }
                        }
                    }
                }
            } catch (error: CancellationException) {
                throw error
            } catch (_: SecurityException) {
                mutableState.update { it.copy(status = LanStatus.PermissionRequired, receivers = emptyList()) }
            } catch (_: Exception) {
                mutableState.update { it.copy(status = LanStatus.Failed, receivers = emptyList()) }
            }
        }
    }

    fun retry() { mode?.let(::start) }

    fun pause() {
        session?.cancel()
        mutableState.update { it.copy(status = LanStatus.Paused, receivers = emptyList()) }
    }
}
