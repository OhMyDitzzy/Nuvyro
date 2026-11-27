package com.ditzzy.nuvyro.core.transfer

internal object NativeTransferBridge {
    @JvmStatic external fun nativeAbiVersion(): Int
    @JvmStatic external fun nativeEngineVersion(): String
    @JvmStatic external fun nativeCreateEngine(expectedAbi: Int): Long
    @JvmStatic external fun nativeDestroyEngine(engine: Long): Int
    @JvmStatic external fun nativeSubmit(engine: Long, command: ByteArray): Int
    @JvmStatic external fun nativePoll(engine: Long, maxEvents: Int, timeoutMs: Int): ByteArray
    @JvmStatic external fun nativeAttachSourceFd(engine: Long, generation: Long, transferId: ByteArray, fileId: ByteArray, fd: Int): Int
    @JvmStatic external fun nativeAttachDestinationFd(engine: Long, generation: Long, transferId: ByteArray, fileId: ByteArray, fd: Int): Int
}
