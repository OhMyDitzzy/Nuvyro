package com.ditzzy.nuvyro.core.transfer

internal class NativeTransferException(val code: Int) : RuntimeException("Native transfer operation failed")

internal enum class NativeStatus(val code: Int) {
    Accepted(0), InvalidArgument(1), InvalidHandle(2), AbiMismatch(3),
    InvalidState(4), QueueFull(5), ResourceLimit(6), InternalFailure(7);

    companion object {
        fun fromCode(code: Int): NativeStatus = entries.firstOrNull { it.code == code }
            ?: throw NativeTransferException(InternalFailure.code)
    }
}
