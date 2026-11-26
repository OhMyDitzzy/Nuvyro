package com.ditzzy.nuvyro.ui.send.receiver.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.unit.dp

@Composable
internal fun RadarBackground(scanning: Boolean, modifier: Modifier = Modifier) {
    val angle = if (scanning) {
        val transition = rememberInfiniteTransition(label = "Radar sweep")
        transition.animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(tween(4_000, easing = LinearEasing), RepeatMode.Restart),
            label = "Sweep rotation"
        )
    } else rememberUpdatedState(0f)
    val primary = MaterialTheme.colorScheme.primary
    val surface = MaterialTheme.colorScheme.surfaceContainerLow
    Canvas(modifier) {
        val radius = size.minDimension * 0.47f
        drawCircle(color = surface, radius = radius)
        for (ring in 1..4) {
            drawCircle(
                color = primary.copy(alpha = if (ring == 4) 0.28f else 0.14f),
                radius = radius * ring / 4,
                style = Stroke(width = 1.dp.toPx())
            )
        }
        val dash = PathEffect.dashPathEffect(floatArrayOf(3.dp.toPx(), 6.dp.toPx()))
        drawLine(
            color = primary.copy(alpha = 0.12f),
            start = Offset(center.x - radius, center.y),
            end = Offset(center.x + radius, center.y),
            strokeWidth = 1.dp.toPx(), pathEffect = dash
        )
        drawLine(
            color = primary.copy(alpha = 0.12f),
            start = Offset(center.x, center.y - radius),
            end = Offset(center.x, center.y + radius),
            strokeWidth = 1.dp.toPx(), pathEffect = dash
        )
        if (scanning) {
            rotate(angle.value) {
                drawCircle(
                    brush = Brush.sweepGradient(
                        0f to Color.Transparent,
                        0.78f to Color.Transparent,
                        1f to primary.copy(alpha = 0.28f),
                        center = center
                    ),
                    radius = radius
                )
                drawLine(
                    color = primary.copy(alpha = 0.65f),
                    start = center,
                    end = Offset(center.x + radius, center.y),
                    strokeWidth = 1.5.dp.toPx()
                )
            }
        }
        drawCircle(color = primary.copy(alpha = 0.45f), radius = 3.dp.toPx())
    }
}
