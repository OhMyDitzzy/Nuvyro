package com.ditzzy.nuvyro.ui.receive

import android.graphics.Bitmap
import android.os.SystemClock
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverPairingQr
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionFailure
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionPhase
import com.ditzzy.nuvyro.core.transfer.receiver.ReceiverSessionState
import java.util.concurrent.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

@Composable
internal fun ReceiverPairingSection(state: ReceiverSessionState, onRetry: () -> Unit) {
    val invitation = state.invitation
    if (state.phase == ReceiverSessionPhase.Listening && invitation != null) {
        key(invitation) { ActivePairingQr(invitation, onRetry) }
        return
    }

    val failed = state.phase == ReceiverSessionPhase.Stopped &&
        state.failure != null && state.failure != ReceiverSessionFailure.Closed
    val noNetwork = state.failure == ReceiverSessionFailure.NetworkUnavailable
    Surface(
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surfaceContainer,
        modifier = Modifier.fillMaxWidth()
    ) {
        Row(
            modifier = Modifier.padding(20.dp),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            if (!failed) CircularProgressIndicator(modifier = Modifier.size(28.dp), strokeWidth = 3.dp)
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.weight(1f)) {
                Text(
                    stringResource(
                        when {
                            noNetwork -> R.string.receive_session_no_network
                            failed -> R.string.receive_session_failed
                            else -> R.string.receive_session_starting
                        }
                    ),
                    style = MaterialTheme.typography.titleMedium
                )
                Text(
                    stringResource(
                        when {
                            noNetwork -> R.string.receive_session_no_network_hint
                            failed -> R.string.receive_session_failed_hint
                            else -> R.string.receive_session_starting_hint
                        }
                    ),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                if (failed) {
                    Button(onClick = onRetry, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 48.dp)) {
                        Text(stringResource(R.string.receive_session_retry))
                    }
                }
            }
        }
    }
}

@Composable
private fun ActivePairingQr(invitation: ReceiverPairingQr, onRetry: () -> Unit) {
    val bitmapOwner = remember(invitation) { ReceiverPairingQrBitmap() }
    var renderFailed by remember(invitation) { mutableStateOf(false) }
    val qrBitmap by produceState<Bitmap?>(null, invitation) {
        val payload = invitation.copyForRendering()
        if (payload == null) {
            renderFailed = true
            return@produceState
        }
        try {
            value = bitmapOwner.render(payload)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            renderFailed = true
        } finally {
            payload.fill(0)
        }
    }
    DisposableEffect(bitmapOwner) {
        onDispose { bitmapOwner.close() }
    }
    val initialRemaining = ((invitation.expiresAtElapsedRealtime - SystemClock.elapsedRealtime()).coerceAtLeast(0) + 999) / 1000
    val remainingSeconds by produceState(initialRemaining, invitation) {
        while (isActive && invitation.isAvailable()) {
            val remaining = (invitation.expiresAtElapsedRealtime - SystemClock.elapsedRealtime()).coerceAtLeast(0)
            value = (remaining + 999) / 1000
            if (remaining == 0L) break
            delay(minOf(remaining, 1000L))
        }
        value = 0
    }
    LaunchedEffect(qrBitmap, remainingSeconds == 0L) {
        if (remainingSeconds == 0L) bitmapOwner.erase()
    }

    Surface(
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.secondaryContainer,
        modifier = Modifier.fillMaxWidth()
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Text(stringResource(R.string.receive_pairing_qr_title), style = MaterialTheme.typography.titleLarge)
            if (remainingSeconds == 0L) {
                Text(stringResource(R.string.receive_pairing_qr_expired))
            } else qrBitmap?.let { bitmap ->
                Image(
                    bitmap = bitmap.asImageBitmap(),
                    filterQuality = FilterQuality.None,
                    contentDescription = stringResource(R.string.receive_pairing_qr_description),
                    modifier = Modifier
                        .size(240.dp)
                        .background(Color.White, RoundedCornerShape(16.dp))
                        .padding(12.dp)
                )
            } ?: if (renderFailed) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(
                        stringResource(R.string.receive_pairing_qr_render_failed),
                        style = MaterialTheme.typography.bodyMedium
                    )
                    Button(onClick = onRetry, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 48.dp)) {
                        Text(stringResource(R.string.receive_session_retry))
                    }
                }
            } else CircularProgressIndicator(modifier = Modifier.size(28.dp), strokeWidth = 3.dp)
            Text(
                stringResource(R.string.receive_pairing_qr_expiry, formatRemaining(remainingSeconds)),
                style = MaterialTheme.typography.titleSmall
            )
            Text(
                stringResource(R.string.receive_pairing_qr_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}

private fun formatRemaining(seconds: Long): String {
    val minutes = seconds / 60
    val remainder = seconds % 60
    return "%1$02d:%2$02d".format(minutes, remainder)
}
