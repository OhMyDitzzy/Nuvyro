package com.ditzzy.nuvyro.ui.send.receiver

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.core.net.toUri
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.ditzzy.nuvyro.R
import com.ditzzy.nuvyro.core.transfer.pairing.ScannedPairingQr
import com.ditzzy.nuvyro.ui.send.SendUiState
import com.ditzzy.nuvyro.ui.send.components.FileSelectionSummary
import android.util.Size
import com.ditzzy.nuvyro.ui.send.components.SendScaffold
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

@Composable
fun PairingQrScannerRoute(
    state: SendUiState,
    onBack: () -> Unit,
    onScanned: (ScannedPairingQr) -> Unit,
    onScanAgain: () -> Unit
) {
    val context = LocalContext.current
    var cameraGranted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var permissionDeclined by rememberSaveable { mutableStateOf(false) }
    var settingsUnavailable by remember { mutableStateOf(false) }
    var invalidQr by remember { mutableStateOf(false) }
    var cameraUnavailable by remember { mutableStateOf(false) }
    var cameraAttempt by remember { mutableIntStateOf(0) }
    val cameraAvailable = remember(context) {
        context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)
    }

    val permissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        cameraGranted = granted
        permissionDeclined = !granted
    }
    val settingsLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        cameraGranted = ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
        permissionDeclined = false
    }

    LifecycleResumeEffect(Unit) {
        cameraGranted = ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
        onPauseOrDispose { }
    }

    val requestCamera = {
        settingsUnavailable = false
        if (permissionDeclined) {
            try {
                settingsLauncher.launch(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                    data = "package:${context.packageName}".toUri()
                })
            } catch (_: ActivityNotFoundException) {
                settingsUnavailable = true
            } catch (_: SecurityException) {
                settingsUnavailable = true
            }
        } else {
            permissionLauncher.launch(Manifest.permission.CAMERA)
        }
    }

    SendScaffold(
        title = stringResource(R.string.send_pairing_scan_title),
        onBack = onBack,
        bottomBar = {
            FileSelectionSummary(state.files)
            OutlinedButton(
                onClick = onBack,
                modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)
            ) {
                Text(stringResource(R.string.send_pairing_scan_back))
            }
        }
    ) { innerPadding ->
        Box(
            modifier = Modifier.fillMaxSize().padding(innerPadding),
            contentAlignment = Alignment.TopCenter
        ) {
            Column(
                modifier = Modifier.widthIn(max = 568.dp).fillMaxSize()
                    .verticalScroll(rememberScrollState()).padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                Text(
                    stringResource(R.string.send_pairing_scan_description),
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                when {
                    state.pairingQrScanned -> PairingQrRecognized(onScanAgain)
                    !cameraAvailable -> Text(
                        stringResource(R.string.send_pairing_scan_no_camera),
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium
                    )
                    !cameraGranted -> Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(
                            stringResource(
                                if (permissionDeclined) R.string.send_pairing_scan_permission_denied
                                else R.string.send_pairing_scan_permission_hint
                            ),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        if (settingsUnavailable) {
                            Text(
                                stringResource(R.string.send_pairing_scan_settings_unavailable),
                                color = MaterialTheme.colorScheme.error
                            )
                        }
                        Button(onClick = requestCamera, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)) {
                            Text(stringResource(
                                if (permissionDeclined) R.string.send_pairing_scan_open_settings
                                else R.string.send_pairing_scan_allow_camera
                            ))
                        }
                    }
                    cameraUnavailable -> Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(
                            stringResource(R.string.send_pairing_scan_camera_failed),
                            color = MaterialTheme.colorScheme.error,
                            style = MaterialTheme.typography.bodyMedium
                        )
                        Button(onClick = { cameraUnavailable = false; cameraAttempt++ }) {
                            Text(stringResource(R.string.setup_retry))
                        }
                    }
                    else -> {
                        key(cameraAttempt) {
                            PairingQrCameraPreview(
                                onScanned = {
                                    invalidQr = false
                                    onScanned(it)
                                },
                                onInvalidQr = { invalidQr = true },
                                onCameraError = { cameraUnavailable = true }
                            )
                        }
                        Text(
                            stringResource(
                                if (invalidQr) R.string.send_pairing_scan_invalid
                                else R.string.send_pairing_scan_camera_hint
                            ),
                            style = MaterialTheme.typography.bodyMedium,
                            color = if (invalidQr) MaterialTheme.colorScheme.error
                                else MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun PairingQrRecognized(onScanAgain: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            stringResource(R.string.send_pairing_scan_recognized),
            style = MaterialTheme.typography.titleMedium
        )
        Text(
            stringResource(R.string.send_pairing_scan_not_paired),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Button(onClick = onScanAgain, modifier = Modifier.fillMaxWidth().sizeIn(minHeight = 56.dp)) {
            Text(stringResource(R.string.send_pairing_scan_again))
        }
    }
}

@Composable
private fun PairingQrCameraPreview(
    onScanned: (ScannedPairingQr) -> Unit,
    onInvalidQr: () -> Unit,
    onCameraError: () -> Unit
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val previewView = remember(context) {
        PreviewView(context).apply {
            scaleType = PreviewView.ScaleType.FILL_CENTER
            implementationMode = PreviewView.ImplementationMode.COMPATIBLE
        }
    }
    val mainExecutor = remember(context) { ContextCompat.getMainExecutor(context) }
    val currentOnScanned by rememberUpdatedState(onScanned)
    val currentOnInvalidQr by rememberUpdatedState(onInvalidQr)
    val currentOnCameraError by rememberUpdatedState(onCameraError)

    AndroidView(
        factory = { previewView },
        modifier = Modifier.fillMaxWidth().height(360.dp).background(MaterialTheme.colorScheme.surfaceVariant)
    )

    DisposableEffect(lifecycleOwner, previewView) {
        val analysisExecutor = Executors.newSingleThreadExecutor()
        val disposed = AtomicBoolean(false)
        val analyzer = PairingQrAnalyzer(
            onScanned = { scanned ->
                mainExecutor.execute {
                    if (disposed.get()) scanned.close() else currentOnScanned(scanned)
                }
            },
            onInvalidQr = { mainExecutor.execute { if (!disposed.get()) currentOnInvalidQr() } }
        )
        val preview = Preview.Builder().build().also { it.surfaceProvider = previewView.surfaceProvider }
        val resolutionSelector = ResolutionSelector.Builder()
            .setAspectRatioStrategy(AspectRatioStrategy.RATIO_4_3_FALLBACK_AUTO_STRATEGY)
            .setResolutionStrategy(
                ResolutionStrategy(Size(640, 480), ResolutionStrategy.FALLBACK_RULE_CLOSEST_LOWER_THEN_HIGHER)
            )
            .build()
        val analysis = ImageAnalysis.Builder()
            .setResolutionSelector(resolutionSelector)
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .build()
            .also { it.setAnalyzer(analysisExecutor, analyzer) }
        var provider: ProcessCameraProvider? = null
        val providerFuture = ProcessCameraProvider.getInstance(context)
        providerFuture.addListener({
            if (disposed.get()) return@addListener
            try {
                val cameraProvider = providerFuture.get()
                val selector = when {
                    cameraProvider.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA) -> CameraSelector.DEFAULT_BACK_CAMERA
                    cameraProvider.hasCamera(CameraSelector.DEFAULT_FRONT_CAMERA) -> CameraSelector.DEFAULT_FRONT_CAMERA
                    else -> error("No usable camera")
                }
                cameraProvider.bindToLifecycle(lifecycleOwner, selector, preview, analysis)
                provider = cameraProvider
            } catch (_: Exception) {
                currentOnCameraError()
            }
        }, mainExecutor)

        onDispose {
            disposed.set(true)
            analysis.clearAnalyzer()
            provider?.unbind(preview, analysis)
            analysisExecutor.shutdownNow()
        }
    }
}
