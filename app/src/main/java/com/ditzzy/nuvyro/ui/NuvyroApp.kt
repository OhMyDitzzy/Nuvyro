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
import com.ditzzy.nuvyro.ui.lan.LanViewModel
import com.ditzzy.nuvyro.ui.lan.LanViewModelFactory
import com.ditzzy.nuvyro.ui.profile.ProfileRoute
import com.ditzzy.nuvyro.ui.profile.ProfileViewModel
import com.ditzzy.nuvyro.ui.profile.ProfileViewModelFactory
import com.ditzzy.nuvyro.ui.receive.ReceiveRoute
import com.ditzzy.nuvyro.ui.send.SendRoute
import com.ditzzy.nuvyro.ui.send.SendViewModel
import com.ditzzy.nuvyro.ui.send.SendViewModelFactory
import com.ditzzy.nuvyro.ui.setup.SetupRoute
import com.ditzzy.nuvyro.ui.setup.SetupViewModel
import com.ditzzy.nuvyro.ui.setup.SetupViewModelFactory

@Composable
fun NuvyroApp() {
    val context = LocalContext.current
    val factory = remember(context.applicationContext) { SetupViewModelFactory(context) }
    val setupViewModel: SetupViewModel = viewModel(factory = factory)
    val state by setupViewModel.state.collectAsStateWithLifecycle()
    val profileFactory = remember(context.applicationContext) { ProfileViewModelFactory(context) }
    val profileViewModel: ProfileViewModel = viewModel(factory = profileFactory)
    val profileState by profileViewModel.state.collectAsStateWithLifecycle()
    val sendFactory = remember(context.applicationContext) { SendViewModelFactory(context) }
    val sendViewModel: SendViewModel = viewModel(factory = sendFactory)
    val sendState by sendViewModel.state.collectAsStateWithLifecycle()
    val lanFactory = remember(context.applicationContext) { LanViewModelFactory(context) }
    val lanViewModel: LanViewModel = viewModel(factory = lanFactory)
    val lanState by lanViewModel.state.collectAsStateWithLifecycle()

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
        state.completed -> when {
            profileState.editing -> ProfileRoute(profileState, profileViewModel)
            lanState.receiving -> ReceiveRoute(
                lanState, profileState.profile?.displayName.orEmpty(), profileState.savedPhoto, lanViewModel
            )
            sendState.active -> SendRoute(sendState, sendViewModel, lanState, lanViewModel)
            else -> HomeScreen(
                profileName = profileState.profile?.displayName.orEmpty(),
                profilePhoto = profileState.savedPhoto,
                onProfileClick = profileViewModel::beginEditing,
                onSendClick = sendViewModel::open,
                onReceiveClick = lanViewModel::openReceive
            )
        }
        else -> SetupRoute(state, setupViewModel, profileState, profileViewModel)
    }
}
