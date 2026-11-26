package com.ditzzy.nuvyro.ui.hotspot

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.ui.send.components.SendScaffold

@Composable
internal fun ConnectionGuideLayout(
    onBack: () -> Unit,
    bottomBar: @Composable () -> Unit,
    content: @Composable ColumnScope.() -> Unit
) {
    SendScaffold(stringResource(R.string.connection_title), onBack, bottomBar) { padding ->
        Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            Column(
                modifier = Modifier.widthIn(max = 568.dp).fillMaxSize()
                    .verticalScroll(rememberScrollState()).padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
                content = content
            )
        }
    }
}
