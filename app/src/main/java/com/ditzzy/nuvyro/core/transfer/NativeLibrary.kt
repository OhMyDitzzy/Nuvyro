package com.ditzzy.nuvyro.core.transfer

internal sealed interface NativeLibraryState {
    data class Ready(val abi: Int, val engineVersion: String) : NativeLibraryState
    data class Unavailable(val reason: Reason) : NativeLibraryState
    enum class Reason { MissingOrIncompatibleLibrary, AbiMismatch, InvalidResponse }
}

internal class NativeLibrary(
    private val loadBinary: () -> Unit = { System.loadLibrary("nuvyro_transfer_jni") },
    private val abiVersion: () -> Int = NativeTransferBridge::nativeAbiVersion,
    private val engineVersion: () -> String = NativeTransferBridge::nativeEngineVersion
) {
    private var cached: NativeLibraryState? = null

    @Synchronized
    fun load(): NativeLibraryState {
        cached?.let { return it }
        val result = try {
            loadBinary()
            val abi = abiVersion()
            if (abi != ABI_VERSION) {
                NativeLibraryState.Unavailable(NativeLibraryState.Reason.AbiMismatch)
            } else {
                val version = engineVersion()
                if (version.isEmpty() || version.length > 64 || !version.all { it.isLetterOrDigit() || it in ".-+" }) {
                    NativeLibraryState.Unavailable(NativeLibraryState.Reason.InvalidResponse)
                } else NativeLibraryState.Ready(abi, version)
            }
        } catch (_: LinkageError) {
            NativeLibraryState.Unavailable(NativeLibraryState.Reason.MissingOrIncompatibleLibrary)
        } catch (_: SecurityException) {
            NativeLibraryState.Unavailable(NativeLibraryState.Reason.MissingOrIncompatibleLibrary)
        } catch (_: NativeTransferException) {
            NativeLibraryState.Unavailable(NativeLibraryState.Reason.InvalidResponse)
        } catch (_: RuntimeException) {
            NativeLibraryState.Unavailable(NativeLibraryState.Reason.InvalidResponse)
        }
        cached = result
        return result
    }

    companion object { const val ABI_VERSION = 1 }
}
