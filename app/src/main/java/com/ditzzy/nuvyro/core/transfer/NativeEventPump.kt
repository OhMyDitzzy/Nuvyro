package com.ditzzy.nuvyro.core.transfer

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

internal class NativeEventPump(
    private val scope: CoroutineScope,
    private val handle: () -> Long,
    private val onFailure: () -> Unit
) {
    private val lock = Any()
    private val mutablePending = MutableStateFlow<List<NativeTransferEvent>>(emptyList())
    val pending = mutablePending.asStateFlow()
    private val mutableFailure = MutableStateFlow<Int?>(null)
    val failure = mutableFailure.asStateFlow()
    private val changed = Channel<Unit>(Channel.CONFLATED)
    private var stopped = false

    fun start() {
        scope.launch {
            var lastId = NativeCommandCodec.ZERO_ID
            try {
                while (isActive) {
                    val current = handle()
                    if (current == 0L) break
                    val room = 64 - mutablePending.value.size
                    if (room == 0) { changed.receive(); continue }
                    val bytes = NativeTransferBridge.nativePoll(current, minOf(32, room), 250)
                    val events = NativeEventCodec.decode(bytes)
                    for (event in events) {
                        require(event.id > lastId)
                        lastId = event.id
                    }
                    if (events.isNotEmpty()) synchronized(lock) {
                        if (!stopped && handle() == current) {
                            mutablePending.value = mutablePending.value + events
                        }
                    }
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: NativeTransferException) {
                if (handle() != 0L) { mutableFailure.value = error.code; onFailure() }
            } catch (_: LinkageError) {
                if (handle() != 0L) { mutableFailure.value = NativeStatus.InternalFailure.code; onFailure() }
            } catch (_: Exception) {
                if (handle() != 0L) { mutableFailure.value = NativeStatus.InternalFailure.code; onFailure() }
            }
        }
    }

    fun stop() {
        synchronized(lock) {
            stopped = true
            mutablePending.value = emptyList()
            changed.trySend(Unit)
        }
    }

    fun acknowledgeThrough(eventId: String): Boolean = synchronized(lock) {
        val index = mutablePending.value.indexOfFirst { it.id == eventId }
        if (index < 0) false else {
            mutablePending.value = mutablePending.value.drop(index + 1)
            changed.trySend(Unit)
            true
        }
    }
}
