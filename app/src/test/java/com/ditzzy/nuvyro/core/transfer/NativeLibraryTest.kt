package com.ditzzy.nuvyro.core.transfer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NativeLibraryTest {
    @Test
    fun missingLibraryIsReportedAndCached() {
        var attempts = 0
        val loader = NativeLibrary(
            loadBinary = { attempts++; throw UnsatisfiedLinkError("test") },
            abiVersion = { error("Must not probe a missing library") },
            engineVersion = { error("Must not probe a missing library") }
        )
        val expected = NativeLibraryState.Unavailable(NativeLibraryState.Reason.MissingOrIncompatibleLibrary)
        assertEquals(expected, loader.load())
        assertEquals(expected, loader.load())
        assertEquals(1, attempts)
    }

    @Test
    fun abiMismatchStopsBeforeVersionProbe() {
        val loader = NativeLibrary({}, { 99 }, { error("Must not use a mismatched ABI") })
        assertEquals(NativeLibraryState.Unavailable(NativeLibraryState.Reason.AbiMismatch), loader.load())
    }

    @Test
    fun matchingVersionIsAvailableAndInvalidVersionIsRejected() {
        assertEquals(NativeLibraryState.Ready(1, "0.1.0"), NativeLibrary({}, { 1 }, { "0.1.0" }).load())
        assertTrue(NativeLibrary({}, { 1 }, { "" }).load() is NativeLibraryState.Unavailable)
    }
}
