package com.ditzzy.nuvyro.ui.hotspot

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ditzzy.nuvyro.core.hotspot.LocalHotspot
import com.ditzzy.nuvyro.core.lan.LanAdvertiser
import com.ditzzy.nuvyro.data.hotspot.HotspotState
import com.ditzzy.nuvyro.data.hotspot.HotspotStatus
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

class HotspotViewModel(private val hotspot: LocalHotspot, private val advertiser: LanAdvertiser) : ViewModel() {
    private val mutableState = MutableStateFlow(HotspotState())
    val state = mutableState.asStateFlow()
    private var session: Job? = null
    private val generation = AtomicInteger()

    fun create() {
        if (state.value.status == HotspotStatus.Starting || state.value.status == HotspotStatus.Active) return
        val attempt = generation.incrementAndGet()
        val previous = session
        previous?.cancel()
        mutableState.value = HotspotState(status = HotspotStatus.Starting)
        session = viewModelScope.launch {
            previous?.cancelAndJoin()
            try {
                hotspot.create().collectLatest { credentials ->
                    advertiser.serveHotspot {
                        mutableState.update {
                            if (generation.get() == attempt) HotspotState(HotspotStatus.Active, credentials) else it
                        }
                    }
                }
            } catch (error: CancellationException) {
                throw error
            } catch (_: Exception) {
                mutableState.update {
                    if (generation.get() == attempt) HotspotState(status = HotspotStatus.Failed) else it
                }
            }
        }
    }

    fun stop() {
        endSession(HotspotStatus.Idle)
    }

    fun ownsActiveReservation(): Boolean = state.value.status == HotspotStatus.Active

    fun pause() {
        if (state.value.status == HotspotStatus.Starting || state.value.status == HotspotStatus.Active) {
            endSession(HotspotStatus.Paused)
        }
    }

    private fun endSession(status: HotspotStatus) {
        generation.incrementAndGet()
        session?.cancel()
        mutableState.value = HotspotState(status = status)
    }
}
