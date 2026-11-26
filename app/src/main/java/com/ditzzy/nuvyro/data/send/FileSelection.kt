package com.ditzzy.nuvyro.data.send

object FileSelection {
    fun merge(current: List<OutgoingFile>, added: List<OutgoingFile>): List<OutgoingFile> =
        (current + added).associateBy { it.uri }.values.toList()

    fun totalSize(files: List<OutgoingFile>): Long? {
        var total = 0L
        for (file in files) {
            val size = file.size?.takeIf { it >= 0 } ?: return null
            if (size > Long.MAX_VALUE - total) return null
            total += size
        }
        return total
    }
}
