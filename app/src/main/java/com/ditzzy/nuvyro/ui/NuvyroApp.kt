package com.ditzzy.nuvyro.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.ui.home.HomeScreen
import com.ditzzy.nuvyro.ui.setup.SetupRoute
import com.ditzzy.nuvyro.ui.setup.SetupViewModel
import com.ditzzy.nuvyro.ui.setup.SetupViewModelFactory

@Composable
fun NuvyroApp() {
    val context = LocalContext.current
    val factory = remember(context.applicationContext) { SetupViewModelFactory(context) }
    val setupViewModel: SetupViewModel = viewModel(factory = factory)
    val state by setupViewModel.state.collectAsStateWithLifecycle()

    when {
        state.loading -> Scaffold { innerPadding ->
            Column(
                modifier = Modifier.fillMaxSize().padding(innerPadding).padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically)
            ) {
                if (state.error != null) {
                    Text(stringResource(R.string.setup_error_save))
                    Button(onClick = setupViewModel::load) {
                        Text(stringResource(R.string.setup_retry))
                    }
                } else {
                    CircularProgressIndicator()
                }
            }
        }
        state.completed -> HomeScreen()
        else -> SetupRoute(state, setupViewModel)
    }
}
