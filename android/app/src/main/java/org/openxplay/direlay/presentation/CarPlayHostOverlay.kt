// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import android.content.Intent
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import kotlinx.coroutines.*
import org.openxplay.direlay.R
import org.openxplay.direlay.RelayActivity
import org.openxplay.direlay.path.*
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.ui.*

fun createCarPlayHostOverlay(activity: ComponentActivity, runtime: RelayRuntime): ComposeView? =
    hostOverlayUnlessPreview(runtime.settingsStore.snapshot.value?.localPreview ?: runtime.localPreview) {
        ComposeView(activity).apply {
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed)
            setContent { RelayTheme { RuntimeHostOverlay(activity, runtime) } }
        }
    }

@Composable private fun RuntimeHostOverlay(activity: ComponentActivity, runtime: RelayRuntime) {
    val model = remember(activity) {
        androidx.lifecycle.ViewModelProvider(activity, RelayViewModel.Factory(activity.applicationContext, runtime))[RelayViewModel::class.java]
    }
    val snapshot by model.overlay.screenState()
    val motion by model.motion.screenState()
    DisposableEffect(activity, model) {
        val observer = androidx.lifecycle.LifecycleEventObserver { _, _ ->
            model.hostFlags.value = model.hostFlags.value.copy(resumed = activity.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED))
        }
        activity.lifecycle.addObserver(observer)
        onDispose { activity.lifecycle.removeObserver(observer) }
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
                model.sessionAction(action)
                if (leave) back()
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Exception) {
                model.action { runtime.status.inputFailed(failure) }
            } finally { busy = false; model.refresh() }
        }
    }
    fun openHome(destination: String) {
        activity.startActivity(Intent(activity, RelayActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(RelayActivity.EXTRA_HOME_DESTINATION, destination))
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
    CompositionLocalProvider(LocalRelayMotion provides motion) {
        Box(Modifier.fillMaxSize()) {
            val value = snapshot
            if (value == null) Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                Box(Modifier.safeDrawingPadding(), contentAlignment = Alignment.Center) { Text(stringResource(R.string.host_connecting)) }
            } else if (!value.localPreview) HostOverlayScreen(presentHostOverlay(value), busy, ::stepAction,
                { model.refresh() }, ::copy, onStartStop = { sessionAction("start") }, onRestart = { sessionAction("restart") },
                onSettings = { openHome("settings:$it") }, onGuide = { openHome("setup") }) { action -> when (action) {
                    HostOverlayAction.BACK -> back()
                    HostOverlayAction.STOP -> sessionAction("stop", leave = true)
                } }
            SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(12.dp))
        }
    }
}
