package com.ditzzy.nuvyro.ui.send.receiver.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.ui.profile.components.ProfileAvatar
import com.ditzzy.nuvyro.ui.send.receiver.ReceiverItem

@Composable
internal fun RadarContact(receiver: ReceiverItem, selected: Boolean, onClick: () -> Unit) {
    val label = stringResource(R.string.receiver_select, receiver.name)
    Surface(
        onClick = onClick,
        selected = selected,
        modifier = Modifier.fillMaxWidth().semantics { contentDescription = label },
        shape = MaterialTheme.shapes.large,
        color = Color.Transparent
    ) {
        Column(
            modifier = Modifier.padding(4.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(4.dp)
        ) {
            Surface(
                modifier = Modifier.size(56.dp),
                shape = CircleShape,
                color = MaterialTheme.colorScheme.surface,
                border = BorderStroke(
                    if (selected) 3.dp else 1.dp,
                    if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant
                )
            ) {
                ProfileAvatar(
                    name = receiver.name,
                    photo = receiver.photo,
                    modifier = Modifier.fillMaxSize().padding(4.dp)
                )
            }
            Text(
                text = receiver.name,
                modifier = Modifier.background(
                    MaterialTheme.colorScheme.surface.copy(alpha = 0.95f),
                    MaterialTheme.shapes.small
                ).padding(horizontal = 6.dp, vertical = 2.dp),
                style = MaterialTheme.typography.labelLarge,
                color = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis
            )
        }
    }
}
