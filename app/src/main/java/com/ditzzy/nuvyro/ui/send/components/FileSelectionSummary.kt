package com.ditzzy.nuvyro.ui.send.components

import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.data.send.FileSelection
import com.ditzzy.nuvyro.data.send.OutgoingFile

@Composable
internal fun FileSelectionSummary(files: List<OutgoingFile>, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val total = remember(files) { FileSelection.totalSize(files) }
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            text = pluralStringResource(R.plurals.send_file_count, files.size, files.size),
            style = MaterialTheme.typography.titleMedium
        )
        Text(
            text = if (total != null) {
                stringResource(R.string.send_total_size, Formatter.formatFileSize(context, total))
            } else {
                stringResource(R.string.send_total_unknown)
            },
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}
