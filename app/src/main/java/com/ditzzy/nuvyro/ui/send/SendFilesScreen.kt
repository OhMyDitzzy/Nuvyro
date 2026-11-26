package com.ditzzy.nuvyro.ui.send

import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.ui.send.components.FileSelectionSummary
import com.ditzzy.nuvyro.ui.send.components.SelectedFileItem
import com.ditzzy.nuvyro.ui.send.components.SendScaffold

@Composable
fun SendFilesScreen(
    state: SendUiState,
    onChooseFiles: () -> Unit,
    onRemoveFile: (Uri) -> Unit,
    onContinue: () -> Unit,
    onBack: () -> Unit
) {
    SendScaffold(
        title = stringResource(R.string.send_files_title),
        onBack = onBack,
        bottomBar = {
            FileSelectionSummary(state.files)
            Button(
                onClick = onContinue,
                enabled = state.files.isNotEmpty() && !state.busy,
                modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
            ) {
                Text(stringResource(R.string.setup_continue))
            }
        }
    ) { innerPadding ->
        Box(
            modifier = Modifier.fillMaxSize().padding(innerPadding),
            contentAlignment = Alignment.TopCenter
        ) {
            LazyColumn(
                modifier = Modifier.widthIn(max = 568.dp).fillMaxSize(),
                contentPadding = PaddingValues(24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                item {
                    Text(
                        text = stringResource(R.string.send_files_description),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                item {
                    FilledTonalButton(
                        onClick = onChooseFiles,
                        enabled = !state.busy,
                        modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
                    ) {
                        Text(stringResource(
                            if (state.files.isEmpty()) R.string.send_choose_files
                            else R.string.send_add_files
                        ))
                    }
                }
                if (state.loading) {
                    item {
                        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                            Text(
                                text = stringResource(R.string.send_reading_files),
                                style = MaterialTheme.typography.bodyMedium,
                                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
                            )
                        }
                    }
                }
                if (state.pickerUnavailable || state.unavailableCount > 0) {
                    item {
                        Text(
                            text = if (state.pickerUnavailable) {
                                stringResource(R.string.send_error_picker)
                            } else {
                                pluralStringResource(
                                    R.plurals.send_files_unavailable,
                                    state.unavailableCount, state.unavailableCount
                                )
                            },
                            color = MaterialTheme.colorScheme.error,
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
                        )
                    }
                }
                if (state.files.isEmpty() && !state.loading) {
                    item { EmptySelection() }
                }
                items(state.files, key = { it.uri.toString() }) { file ->
                    SelectedFileItem(file, enabled = !state.busy, onRemove = { onRemoveFile(file.uri) })
                }
            }
        }
    }
}

@Composable
private fun EmptySelection() {
    Column(
        modifier = Modifier.fillMaxWidth().padding(vertical = 24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Surface(
            color = MaterialTheme.colorScheme.primaryContainer,
            contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
            shape = MaterialTheme.shapes.extraLarge,
            modifier = Modifier.size(88.dp)
        ) {
            Box(contentAlignment = Alignment.Center) {
                Icon(
                    painter = painterResource(R.drawable.ic_file),
                    contentDescription = null,
                    modifier = Modifier.size(40.dp)
                )
            }
        }
        Text(
            text = stringResource(R.string.send_empty_title),
            style = MaterialTheme.typography.titleLarge
        )
        Text(
            text = stringResource(R.string.send_empty_description),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}
