package com.ditzzy.nuvyro.ui.home

import android.graphics.Bitmap
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.ui.profile.components.ProfileAvatar
import kotlinx.coroutines.launch

@Composable
fun HomeScreen(
    modifier: Modifier = Modifier,
    profileName: String = "",
    profilePhoto: Bitmap? = null,
    onProfileClick: () -> Unit = {},
    onSendClick: () -> Unit = {}
) {
    val snackbarHostState = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    val receiveMessage = stringResource(R.string.home_receive_unavailable)
    val showMessage: (String) -> Unit = { message ->
        scope.launch {
            snackbarHostState.currentSnackbarData?.dismiss()
            snackbarHostState.showSnackbar(message)
        }
    }

    Scaffold(
        modifier = modifier,
        snackbarHost = { SnackbarHost(snackbarHostState) }
    ) { innerPadding ->
        HomeContent(
            profileName = profileName,
            profilePhoto = profilePhoto,
            onProfileClick = onProfileClick,
            onSendClick = onSendClick,
            onReceiveClick = { showMessage(receiveMessage) },
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
        )
    }
}

@Composable
private fun HomeContent(
    profileName: String,
    profilePhoto: Bitmap?,
    onProfileClick: () -> Unit,
    onSendClick: () -> Unit,
    onReceiveClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 24.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Column(modifier = Modifier.widthIn(max = 520.dp).fillMaxWidth()) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Surface(
                    modifier = Modifier.size(48.dp),
                    shape = RoundedCornerShape(18.dp),
                    color = MaterialTheme.colorScheme.primaryContainer,
                    contentColor = MaterialTheme.colorScheme.onPrimaryContainer
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(
                            painter = painterResource(R.drawable.ic_transfer),
                            contentDescription = null,
                            modifier = Modifier.size(26.dp)
                        )
                    }
                }
                Text(
                    text = stringResource(R.string.app_name),
                    modifier = Modifier.weight(1f),
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.SemiBold
                )
                val profileLabel = stringResource(R.string.profile_open)
                IconButton(
                    onClick = onProfileClick,
                    modifier = Modifier.size(48.dp).semantics { contentDescription = profileLabel }
                ) {
                    ProfileAvatar(profileName, profilePhoto, Modifier.size(40.dp))
                }
            }

            Spacer(modifier = Modifier.size(40.dp))

            Text(
                text = stringResource(R.string.home_headline),
                style = MaterialTheme.typography.displaySmall,
                fontWeight = FontWeight.SemiBold
            )
            Text(
                text = stringResource(R.string.home_description),
                modifier = Modifier.padding(top = 12.dp),
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )

            Spacer(modifier = Modifier.size(32.dp))

            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                HomeAction(
                    title = R.string.home_send,
                    description = R.string.home_send_description,
                    icon = R.drawable.ic_send,
                    colors = ButtonDefaults.buttonColors(),
                    onClick = onSendClick
                )
                HomeAction(
                    title = R.string.home_receive,
                    description = R.string.home_receive_description,
                    icon = R.drawable.ic_receive,
                    colors = ButtonDefaults.filledTonalButtonColors(),
                    onClick = onReceiveClick
                )
            }
        }
    }
}

@Composable
private fun HomeAction(
    @StringRes title: Int,
    @StringRes description: Int,
    @DrawableRes icon: Int,
    colors: ButtonColors,
    onClick: () -> Unit
) {
    Button(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 112.dp),
        shape = RoundedCornerShape(32.dp),
        colors = colors,
        contentPadding = PaddingValues(24.dp)
    ) {
        Icon(
            painter = painterResource(icon),
            contentDescription = null,
            modifier = Modifier.size(32.dp)
        )
        Column(
            modifier = Modifier.weight(1f).padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp)
        ) {
            Text(
                text = stringResource(title),
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.SemiBold
            )
            Text(
                text = stringResource(description),
                style = MaterialTheme.typography.bodyMedium
            )
        }
        Icon(
            painter = painterResource(R.drawable.ic_arrow_forward),
            contentDescription = null,
            modifier = Modifier.size(20.dp)
        )
    }
}
