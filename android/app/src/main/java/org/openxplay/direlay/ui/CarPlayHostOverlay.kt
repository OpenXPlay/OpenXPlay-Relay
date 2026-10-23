// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.relay.VideoStats

fun createCarPlayHostOverlay(activity: ComponentActivity, runtime: RelayRuntime): ComposeView? =
    hostOverlayUnlessPreview(runtime.settingsStore.snapshot.value?.localPreview ?: runtime.localPreview) {
        ComposeView(activity).apply {
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed)
            setContent { RelayTheme { RuntimeHostOverlay(activity, runtime) } }
        }
    }

private fun RelayRuntime.overlaySnapshot(videoStats: VideoStats): HostOverlaySnapshot? {
    val settings = settingsStore.snapshot.value ?: return null
    val selection = iphoneSelection(settings.iphoneAddress)
    val name = iphoneBonds.firstOrNull { it.address.equals(selection.address, ignoreCase = true) }?.name
    val addresses = carIpv4.toList()
    val urls = headUnitUrls()
    val failures = status.failures
    val carPlayError = carPlayDiagnostics.error
    val carlinkOn = settings.carlinkOutput || settings.bothOutputs
    val failedCarlink = carlinkOn && carlinkFailed
    val webReason = serverReason.takeIf { !settings.carlinkOutput || settings.bothOutputs }
    val screen = carPlayScreenSummary
    val window = displayPolicy.primaryDisplay?.viewport
    val carlinkDisplay = carlinkScreen
    val stats = videoStats.sample(status, settings.videoStats, carlinkOn,
        carlinkStatistics, carlinkVideoDecision, carlinkDisplay)
    return HostOverlaySnapshot(HomeSnapshot(settings, settingsStore.wireless(settings),
        permissionsReady = true, identityReady = true, phase = status.session?.phase, started = true,
        failure = failures.isNotEmpty() || failedCarlink || webReason != null || carPlayError != null,
        iphoneSelection = selection, webReason = webReason, webListening = serverState.startsWith("TLS enabled"),
        carNetworkAvailable = addresses.isNotEmpty(),
        problems = homeProblems(failures, failedCarlink, carlinkDetail, carlinkError, webReason, serverState,
            teslaFailure = teslaFailure, carlinkTrail = carlinkErrorTrail, carlinkErrorAt = carlinkErrorAt,
            carPlayError = carPlayError), stage = carPlayStage.stage,
        deviceName = bondedDevices.localName() ?: settings.carlinkDeviceName),
        name, urls, stopped = autoStartGate.paused,
        publicNameUrl = publicNameUrl(), carIpv4 = addresses, teslaUrl = teslaUrl(),
        carlinkState = carlinkStateLabel(carlinkState, carlinkFailed, carlinkPins.pending != null),
        carlinkWindow = carlinkDisplay?.let { HostOverlaySize(it.widthPx, it.heightPx) },
        canvas = screen.first.substringBefore(" · "), screenWidthMm = carPlayScreenWidthMm,
        videoCodec = status.videoCodec ?: appliedVideoDecision.codec,
        carPlayScreen = screen, viewport = window?.let { HostOverlaySize(it.width, it.height) }, videoStats = stats)
}

@Composable private fun RuntimeHostOverlay(activity: ComponentActivity, runtime: RelayRuntime) {
    val snapshot by produceState<HostOverlaySnapshot?>(null, activity, runtime) {
        val videoStats = VideoStats()
        try {
            while (true) {
                if (activity.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
                    val next = withContext(Dispatchers.IO) { runtime.overlaySnapshot(videoStats) }
                    if (activity.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
                        value = next
                        next?.takeIf { !it.home.settings.localPreview }?.let { snapshot ->
                            val presentation = presentHostOverlay(snapshot)
                            runtime.connectionLog.transition("ui", "running_page", "running_page_status",
                                statusLogDetail(presentation.headline, presentation.hint, presentation.tone,
                                    snapshot.home, activity.resources::getResourceEntryName))
                        }
                    }
                } else {
                    videoStats.clear()
                    runtime.status.countVideoBytes = false
                }
                delay(500)
            }
        } finally {
            videoStats.clear()
            runtime.status.countVideoBytes = false
        }
    }
    val scope = rememberCoroutineScope()
    var stopping by remember { mutableStateOf(false) }
    fun back() {
        // Use the host's home path, including touch release and its launcher routing to OpenXPlay Relay.
        activity.onBackPressedDispatcher.onBackPressed()
    }
    if (snapshot == null) {
        RelayPage { Heading(stringResource(R.string.host_connecting)) }
    }
    snapshot?.let { value ->
        // A saved preview change may arrive while the host is still alive.
        if (!value.home.settings.localPreview) HostOverlayScreen(presentHostOverlay(value), stopping) { action ->
            when (action) {
                HostOverlayAction.BACK -> back()
                HostOverlayAction.STOP -> if (!stopping) {
                    stopping = true
                    scope.launch {
                        try {
                            withContext(Dispatchers.IO) { runtime.sessionActions.action("stop") }
                            back()
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (failure: Exception) {
                            runtime.status.inputFailed(failure)
                        } finally { stopping = false }
                    }
                }
            }
        }
    }
}

@Composable internal fun HostOverlayScreen(value: HostOverlayPresentation, stopping: Boolean,
    onAction: (HostOverlayAction) -> Unit) {
    val motion = rememberRelayMotion()
    val clipboard = LocalClipboardManager.current
    CompositionLocalProvider(LocalRelayMotion provides motion) {
    RelayPage {
        Column(Modifier.widthIn(max = 640.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            val description = stringResource(value.headline)
            when (value.tone) {
                StatusTone.PROGRESS -> if (motion) CircularProgressIndicator(Modifier.size(48.dp))
                    else Symbol(R.drawable.symbol_sync, description, Modifier.size(48.dp))
                StatusTone.RUNNING -> CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.primary) {
                    Symbol(R.drawable.symbol_check, description, Modifier.size(48.dp))
                }
                StatusTone.ERROR -> Symbol(R.drawable.symbol_error, description, Modifier.size(48.dp))
                else -> Symbol(R.drawable.symbol_stop, description, Modifier.size(48.dp))
            }
            Heading(description)
            value.iphoneName?.let { Text(stringResource(R.string.ui_iphone_name, it), style = MaterialTheme.typography.titleMedium) }
            value.problems.forEach { problem -> key(problem) { HomeProblemRow(problem) } }
            value.hint?.let { Hint(stringResource(it, *value.hintArguments.toTypedArray())) }
            value.buttons.forEach { button ->
                OutlinedButton(onClick = { onAction(button.action) }, enabled = !stopping,
                    modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
                    Symbol(if (button.action == HostOverlayAction.BACK) R.drawable.symbol_back else R.drawable.symbol_stop,
                        stringResource(button.label), Modifier.size(24.dp))
                    Spacer(Modifier.width(12.dp))
                    Text(stringResource(button.label), Modifier.weight(1f))
                }
            }
            value.sections.forEach { section ->
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(stringResource(section.title), style = MaterialTheme.typography.titleMedium)
                        section.rows.forEach { row ->
                            DetailValue(row.label, row.valueResource?.let { stringResource(it, *row.arguments.toTypedArray()) }
                                ?: row.value, row.monospace)
                        }
                        if (section.title == R.string.host_head_unit_address) {
                            ReceiverAddresses(value.receiverUrls, { clipboard.setText(AnnotatedString(it)) }, value.teslaUrl)
                            value.browserHint?.let { Hint(stringResource(it)) }
                        }
                    }
                }
            }
            value.videoStats?.let { VideoStatisticsCard(it) }
        }
    }
    }
}
