package com.ditzzy.nuvyro.ui.send.receiver.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
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
    Box(modifier.drawWithCache {
        val center = Offset(size.width / 2f, size.height / 2f)
        val radius = size.minDimension * 0.47f
        val ringStroke = Stroke(width = 1.dp.toPx())
        val dash = PathEffect.dashPathEffect(floatArrayOf(3.dp.toPx(), 6.dp.toPx()))
        val sweepHead = Offset(center.x + radius, center.y)
        val trailStart = 0.58f
        val trailDegrees = 360f * (1f - trailStart)
        val afterglow = Brush.sweepGradient(
            0f to primary.copy(alpha = 0f),
            trailStart to primary.copy(alpha = 0f),
            0.64f to primary.copy(alpha = 0.008f),
            0.72f to primary.copy(alpha = 0.025f),
            0.80f to primary.copy(alpha = 0.06f),
            0.88f to primary.copy(alpha = 0.12f),
            0.94f to primary.copy(alpha = 0.22f),
            0.98f to primary.copy(alpha = 0.34f),
            1f to primary.copy(alpha = 0.46f),
            center = center
        )
        val beam = Brush.linearGradient(
            0f to primary.copy(alpha = 0.3f),
            0.2f to primary.copy(alpha = 0.8f),
            1f to primary,
            start = center,
            end = sweepHead
        )
        val headGlowRadius = 12.dp.toPx()
        val headGlow = Brush.radialGradient(
            0f to primary.copy(alpha = 0.42f),
            0.4f to primary.copy(alpha = 0.16f),
            1f to primary.copy(alpha = 0f),
            center = sweepHead,
            radius = headGlowRadius
        )
        val rimStroke = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round)
        val haloWidths = floatArrayOf(16.dp.toPx(), 9.dp.toPx(), 4.dp.toPx())
        val haloAlphas = floatArrayOf(0.025f, 0.06f, 0.14f)
        val beamWidth = 1.5.dp.toPx()

        onDrawBehind {
            drawCircle(color = surface, radius = radius)
            for (ring in 1..4) {
                drawCircle(
                    color = primary.copy(alpha = if (ring == 4) 0.28f else 0.14f),
                    radius = radius * ring / 4,
                    style = ringStroke
                )
            }
            drawLine(
                color = primary.copy(alpha = 0.12f),
                start = Offset(center.x - radius, center.y),
                end = sweepHead,
                strokeWidth = ringStroke.width, pathEffect = dash
            )
            drawLine(
                color = primary.copy(alpha = 0.12f),
                start = Offset(center.x, center.y - radius),
                end = Offset(center.x, center.y + radius),
                strokeWidth = ringStroke.width, pathEffect = dash
            )
            if (scanning) {
                rotate(angle.value) {
                    // Clockwise motion leaves the fading sector on the negative-angle side.
                    drawCircle(brush = afterglow, radius = radius)
                    drawArc(
                        brush = afterglow,
                        startAngle = -trailDegrees,
                        sweepAngle = trailDegrees,
                        useCenter = false,
                        topLeft = Offset(center.x - radius, center.y - radius),
                        size = Size(radius * 2f, radius * 2f),
                        style = rimStroke
                    )
                    for (index in haloWidths.indices) {
                        drawLine(
                            color = primary.copy(alpha = haloAlphas[index]),
                            start = center,
                            end = sweepHead,
                            strokeWidth = haloWidths[index],
                            cap = StrokeCap.Round
                        )
                    }
                    drawLine(
                        brush = beam,
                        start = center,
                        end = sweepHead,
                        strokeWidth = beamWidth,
                        cap = StrokeCap.Round
                    )
                    drawCircle(
                        brush = headGlow,
                        center = sweepHead,
                        radius = headGlowRadius
                    )
                }
            }
            drawCircle(color = primary.copy(alpha = 0.7f), radius = 3.dp.toPx())
        }
    })
}
