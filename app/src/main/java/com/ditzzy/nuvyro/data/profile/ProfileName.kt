package com.ditzzy.nuvyro.data.profile

object ProfileName {
    const val MaxLength = 32

    fun normalize(value: String): String = value.trim().replace(Regex("\\s+"), " ")

    fun length(value: String): Int = value.codePointCount(0, value.length)

    fun isValid(value: String): Boolean {
        val name = normalize(value)
        return name.isNotBlank() && length(name) <= MaxLength && name.none { it.isISOControl() }
    }

    fun deviceDefault(model: String): String {
        val name = normalize(model).filterNot { it.isISOControl() }.ifBlank { "Android device" }
        return name.substring(0, name.offsetByCodePoints(0, minOf(length(name), MaxLength)))
    }
}
