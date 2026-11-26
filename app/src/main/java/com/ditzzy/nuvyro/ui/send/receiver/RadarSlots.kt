package com.ditzzy.nuvyro.ui.send.receiver

internal fun assignRadarSlots(ids: List<String>, previous: Map<String, Int>): Map<String, Int> {
    val current = ids.toSet()
    val slots = previous.filterKeys { it in current }.toMutableMap()
    val occupied = slots.values.toMutableSet()
    var next = 0
    for (id in ids) {
        if (id in slots) continue
        while (next in occupied) next++
        slots[id] = next
        occupied += next
    }
    return slots
}
