package com.ditzzy.nuvyro.ui.send.receiver.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.ui.send.receiver.ReceiverItem
import com.ditzzy.nuvyro.ui.send.receiver.assignRadarSlots
import kotlin.math.roundToInt

@Composable
internal fun ReceiverRadar(
    receivers: List<ReceiverItem>,
    selectedId: String?,
    scanning: Boolean,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    val contacts = remember(receivers) { receivers.distinctBy { it.id } }
    val ids = remember(contacts) { contacts.map { it.id } }
    var previousSlots by remember { mutableStateOf(emptyMap<String, Int>()) }
    val slots = remember(ids, previousSlots) { assignRadarSlots(ids, previousSlots) }
    SideEffect { if (previousSlots != slots) previousSlots = slots }

    BoxWithConstraints(modifier = modifier.fillMaxWidth()) {
        val perPage = if (maxWidth < 260.dp || LocalDensity.current.fontScale > 1.5f) 1 else 3
        val pages = slots.values.map { it / perPage }.distinct().sorted().ifEmpty { listOf(0) }
        var pageIndex by rememberSaveable { mutableIntStateOf(0) }
        val currentPageIndex = pageIndex.coerceIn(0, pages.lastIndex)
        SideEffect { if (pageIndex != currentPageIndex) pageIndex = currentPageIndex }
        val visible = contacts.filter { slots.getValue(it.id) / perPage == pages[currentPageIndex] }

        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(modifier = Modifier.fillMaxWidth().aspectRatio(1f)) {
                RadarBackground(scanning, Modifier.fillMaxSize())
                Layout(
                    modifier = Modifier.fillMaxSize(),
                    content = {
                        for (receiver in visible) {
                            key(receiver.id) {
                                RadarContact(
                                    receiver = receiver,
                                    selected = receiver.id == selectedId,
                                    onClick = { onSelect(receiver.id) }
                                )
                            }
                        }
                    }
                ) { measurables, constraints ->
                    val width = constraints.maxWidth
                    val height = constraints.maxHeight
                    val contactWidth = (width * if (perPage == 1) 0.65f else 0.36f).roundToInt()
                    val placeables = measurables.map {
                        it.measure(Constraints(maxWidth = contactWidth))
                    }
                    layout(width, height) {
                        for ((index, placeable) in placeables.withIndex()) {
                            val slot = slots.getValue(visible[index].id) % perPage
                            val point = if (perPage == 1) Offset(0.5f, 0.48f) else radarPoints[slot]
                            val x = (width * point.x - placeable.width / 2f).roundToInt()
                            val y = (height * point.y - placeable.height / 2f).roundToInt()
                            placeable.place(
                                x.coerceIn(0, (width - placeable.width).coerceAtLeast(0)),
                                y.coerceIn(0, (height - placeable.height).coerceAtLeast(0))
                            )
                        }
                    }
                }
            }
            if (pages.size > 1) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    TextButton(
                        onClick = { pageIndex = currentPageIndex - 1 },
                        enabled = currentPageIndex > 0,
                        modifier = Modifier.weight(1f)
                    ) {
                        Text(stringResource(R.string.receiver_previous_page))
                    }
                    Text(
                        text = stringResource(R.string.receiver_radar_page, currentPageIndex + 1, pages.size),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    TextButton(
                        onClick = { pageIndex = currentPageIndex + 1 },
                        enabled = currentPageIndex < pages.lastIndex,
                        modifier = Modifier.weight(1f)
                    ) {
                        Text(stringResource(R.string.receiver_next_page))
                    }
                }
            }
        }
    }
}

private val radarPoints = listOf(Offset(0.5f, 0.2f), Offset(0.23f, 0.68f), Offset(0.77f, 0.68f))
