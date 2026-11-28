package com.ditzzy.nuvyro.core.transfer

import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverCommandCodec
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverNativePort
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverRouteSnapshot
import java.util.concurrent.atomic.AtomicLong
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal sealed interface NativeEngineOpenResult {
    data class Ready(val engine: NativeTransferEngine) : NativeEngineOpenResult
    data class Unavailable(val library: NativeLibraryState.Unavailable) : NativeEngineOpenResult
    data class Failed(val status: NativeStatus) : NativeEngineOpenResult
}

internal class NativeTransferEngine private constructor(initialHandle: Long) : ReceiverNativePort {
    private val handle = AtomicLong(initialHandle)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val closed = CompletableDeferred<Unit>()
    private val pump = NativeEventPump(scope, handle::get, ::close)
    override val pendingEvents get() = pump.pending
    override val failure get() = pump.failure

    init { pump.start() }

    override suspend fun startLocalSession(requestId: String): NativeStatus = submit { NativeCommandCodec.startSession(requestId) }
    override suspend fun receiverRoutes(generation: Long, sessionId: String, routeGeneration: Long,
        routes: ReceiverRouteSnapshot, update: Boolean): NativeStatus =
        submit { ReceiverCommandCodec.encode(generation, sessionId, routeGeneration, routes, update) }
    override suspend fun createInvitation(generation: Long, sessionId: String, routeGeneration: Long): ByteArray {
        val acquired = AtomicReference<ByteArray?>(null)
        try {
            val payload = withContext(Dispatchers.IO) {
                val current = handle.get()
                if (current == 0L) throw NativeTransferException(NativeStatus.InvalidHandle.code)
                NativeTransferBridge.nativeCreateInvitation(current, generation,
                    ReceiverCommandCodec.sessionIdBytes(sessionId), routeGeneration).also(acquired::set)
            }
            acquired.set(null)
            return payload
        } finally {
            // Cancellation can arrive after JNI allocates bytes, before the dispatcher returns.
            acquired.getAndSet(null)?.fill(0)
        }
    }
    override suspend fun clearInvitation(generation: Long, sessionId: String, routeGeneration: Long): NativeStatus = withContext(Dispatchers.IO) {
        val current = handle.get()
        if (current == 0L) NativeStatus.InvalidHandle
        else NativeStatus.fromCode(NativeTransferBridge.nativeClearInvitation(current, generation,
            ReceiverCommandCodec.sessionIdBytes(sessionId), routeGeneration))
    }
    override suspend fun closeLocalSession(generation: Long, sessionId: String): NativeStatus =
        submit { NativeCommandCodec.closeSession(generation, sessionId) }

    private suspend fun submit(command: () -> ByteArray): NativeStatus = withContext(Dispatchers.IO) {
        val current = handle.get()
        if (current == 0L) NativeStatus.InvalidHandle
        else NativeStatus.fromCode(NativeTransferBridge.nativeSubmit(current, command()))
    }

    override fun acknowledgeThrough(eventId: String): Boolean = pump.acknowledgeThrough(eventId)

    override fun close() {
        val current = handle.getAndSet(0L)
        if (current == 0L) return
        pump.stop()
        scope.launch(NonCancellable) {
            try { NativeTransferBridge.nativeDestroyEngine(current) }
            catch (_: LinkageError) { }
            catch (_: RuntimeException) { }
            finally { scope.cancel(); closed.complete(Unit) }
        }
    }

    override suspend fun awaitClosed() { closed.await() }

    companion object {
        private val library = NativeLibrary()

        suspend fun open(): NativeEngineOpenResult {
            val acquired = AtomicReference<NativeTransferEngine?>(null)
            try {
                return withContext(Dispatchers.IO) {
                    val loaded = library.load()
                    if (loaded is NativeLibraryState.Unavailable) return@withContext NativeEngineOpenResult.Unavailable(loaded)
                    val handle = NativeTransferBridge.nativeCreateEngine(NativeLibrary.ABI_VERSION)
                    if (handle <= 0L) return@withContext NativeEngineOpenResult.Failed(NativeStatus.InternalFailure)
                    val engine = try { NativeTransferEngine(handle) }
                    catch (error: Throwable) { runCatching { NativeTransferBridge.nativeDestroyEngine(handle) }; throw error }
                    acquired.set(engine)
                    NativeEngineOpenResult.Ready(engine)
                }
            } catch (error: CancellationException) {
                acquired.get()?.close()
                throw error
            } catch (error: NativeTransferException) {
                acquired.get()?.close()
                return NativeEngineOpenResult.Failed(NativeStatus.entries.firstOrNull { it.code == error.code } ?: NativeStatus.InternalFailure)
            } catch (_: LinkageError) {
                acquired.get()?.close()
                return NativeEngineOpenResult.Failed(NativeStatus.InternalFailure)
            } catch (_: Exception) {
                acquired.get()?.close()
                return NativeEngineOpenResult.Failed(NativeStatus.InternalFailure)
            }
        }
    }
}
