// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import android.content.Intent
import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import com.shilapi.xcertplay.AirPlayPersistence
import com.shilapi.xcertplay.orchestration.MfiTarget
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.openxplay.direlay.R
import org.openxplay.direlay.RelayActivity
import org.openxplay.direlay.path.*
import org.openxplay.direlay.relay.PathLocal
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.relay.pathInput
import org.openxplay.direlay.settings.StartupPermissions
import org.openxplay.direlay.settings.currentWifiSsid
import org.openxplay.direlay.storage.MaterialStatus

fun createCarPlayHostOverlay(activity: ComponentActivity, runtime: RelayRuntime): ComposeView? =
    hostOverlayUnlessPreview(runtime.settingsStore.snapshot.value?.localPreview ?: runtime.localPreview) {
        ComposeView(activity).apply {
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed)
            setContent { RelayTheme { RuntimeHostOverlay(activity, runtime) } }
        }
    }

/** Called on IO: Android's bond/name/material reads must never block the CarPlay surface. */
private fun RelayRuntime.overlaySnapshot(activity: ComponentActivity, wifi: String?, name: String?, version: String): HostOverlaySnapshot? {
    val settings = settingsStore.snapshot.value ?: return null
    val wireless = settingsStore.wireless(settings)
    val carlink = settings.carlinkOutput || settings.bothOutputs
    val required = StartupPermissions.required(Build.VERSION.SDK_INT, wireless, settings.wirelessNetwork.choice.mode,
        AirPlayPersistence.loadLocationReportingEnabled(activity), carlink)
    val identity = settings.mfiTarget != MfiTarget.LOCAL || identityStore.status() == MaterialStatus.Installed
    val valid = runCatching { settings.validate(wireless) }.isSuccess && (!carlink || Build.VERSION.SDK_INT >= 29)
    val input = pathInput(PathLocal(
        permissionsGranted = required.count { activity.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED },
        permissionsTotal = required.size,
        vpnConsentNeeded = needsVpnConsent(settings) && VpnService.prepare(activity) != null,
        identity = if (identity) IdentityState.READY else IdentityState.MISSING,
        identityLabel = if (!identity) null else activity.getString(when (settings.mfiTarget) {
            MfiTarget.LOCAL -> R.string.direlay_installed
            MfiTarget.REMOTE -> R.string.dev_remote
            MfiTarget.I2C -> R.string.dev_linux_i2c
            MfiTarget.USB_CH341 -> R.string.dev_usb_i2c
        }), wifiSsid = wifi, bluetoothName = name,
        setupSummary = modeRows(settings, wireless).joinToString(" · ") { activity.getString(it.value) },
        nowMillis = System.currentTimeMillis(), version = version, setupValid = valid))
    return HostOverlaySnapshot(input, headUnitUrls(), settings.localPreview,
        homeProblems(visibleHomeFailures(settings, status.failures), carlink && carlinkFailed,
            carlinkDetail, carlinkError, serverReason.takeIf { !settings.carlinkOutput || settings.bothOutputs },
            serverState, networkReadiness(settings, networkReadings), teslaFailure, carlinkErrorTrail,
            carlinkErrorAt, carPlayDiagnostics.error, dlnaFailure.takeIf { settings.dlnaOutput },
            miracastFailure.takeIf { settings.miracastOutput }))
}

@Composable private fun RuntimeHostOverlay(activity: ComponentActivity, runtime: RelayRuntime) {
    var recheck by remember { mutableIntStateOf(0) }
    val snapshot by produceState<HostOverlaySnapshot?>(null, activity, runtime) {
        var wifi: String? = null
        var name: String? = null
        var lastRefresh = Long.MIN_VALUE / 2
        var checked = -1
        val version = withContext(Dispatchers.IO) { activity.packageManager.getPackageInfo(activity.packageName, 0).versionName.orEmpty() }
        while (true) {
            if (activity.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
                val now = android.os.SystemClock.elapsedRealtime()
                val requested = recheck
                val refresh = requested != checked || now - lastRefresh >= 5_000L
                val next = withContext(Dispatchers.IO) {
                    if (refresh) {
                        wifi = currentWifiSsid(activity)
                        runtime.refreshIphoneBonds()
                        name = runtime.bondedDevices.localName()
                        runtime.autoStartSessions.refresh()
                        runtime.recheckNetworkStart()
                    }
                    runtime.overlaySnapshot(activity, wifi, name, version)
                }
                if (refresh) { lastRefresh = now; checked = requested }
                if (activity.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) value = next
                if (activity.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) && next?.localPreview == false)
                    runtime.connectionLog.transition("ui", "path", "path_line", presentHostOverlay(next).path.diagnostic)
            }
            delay(500)
        }
    }
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    val snackbar = remember { SnackbarHostState() }
    val clipboard = LocalClipboardManager.current
    fun copy(text: String) {
        clipboard.setText(AnnotatedString(text))
        scope.launch { snackbar.showSnackbar(activity.getString(R.string.settings_copied)) }
    }
    fun back() {
        // Use the host's Home path, including touch release and launcher routing to RelayActivity.
        activity.onBackPressedDispatcher.onBackPressed()
    }
    fun sessionAction(action: String, leave: Boolean = false) {
        if (busy) return
        busy = true
        scope.launch {
            try {
                withContext(Dispatchers.IO) { runtime.sessionActions.action(action) }
                if (leave) back()
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                runtime.status.inputFailed(failure)
            } finally { busy = false; recheck++ }
        }
    }
    fun stepAction(action: StepAction, step: PathStep) {
        when (action) {
            StepAction.START_NOW -> sessionAction("startNow")
            StepAction.RETRY -> sessionAction("retry")
            StepAction.COPY_PAGE_ADDRESS -> step.evidence?.let { text ->
                (text.raw ?: text.res?.let { activity.getString(it, *text.args.toTypedArray()) })?.let(::copy)
            }
            else -> runCatching {
                activity.startActivity(Intent(activity, RelayActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                    .putExtra(RelayActivity.EXTRA_STEP_ACTION, action.name))
            }.onFailure { runCatching { activity.startActivity(Intent(Settings.ACTION_SETTINGS)) } }
        }
    }
    Box(Modifier.fillMaxSize()) {
        val value = snapshot
        if (value == null) Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Box(Modifier.safeDrawingPadding(), contentAlignment = Alignment.Center) { Text(stringResource(R.string.host_connecting)) }
        } else if (!value.localPreview) HostOverlayScreen(presentHostOverlay(value), busy, ::stepAction,
            { recheck++ }, ::copy) { action -> when (action) {
                HostOverlayAction.BACK -> back()
                HostOverlayAction.STOP -> sessionAction("stop", leave = true)
                HostOverlayAction.RETRY -> sessionAction("retry")
            } }
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(12.dp))
    }
}

@Composable internal fun HostOverlayScreen(value: HostOverlayPresentation, busy: Boolean,
    onStepAction: (StepAction, PathStep) -> Unit, onRecheck: () -> Unit, onCopy: (String) -> Unit,
    onAction: (HostOverlayAction) -> Unit) {
    val motion = rememberRelayMotion()
    val haptic = LocalHapticFeedback.current
    CompositionLocalProvider(LocalRelayMotion provides motion) {
        ConnectionPathPage(value.path, value.receiverUrls, value.problems, onStepAction, onRecheck, onCopy,
            onStop = { onAction(HostOverlayAction.STOP) }, actions = {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    value.buttons.forEach { button ->
                        OutlinedButton({
                            if (button.action != HostOverlayAction.BACK) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                            onAction(button.action)
                        }, Modifier.weight(1f).heightIn(min = 56.dp), enabled = !busy,
                            contentPadding = PaddingValues(horizontal = 8.dp)) { Text(stringResource(button.label), textAlign = TextAlign.Center) }
                    }
                }
            })
    }
}
