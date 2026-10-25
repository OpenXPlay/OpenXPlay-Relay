// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.*
import org.openxplay.direlay.R
import org.openxplay.direlay.presentation.*
import org.openxplay.direlay.path.StepAction
import org.openxplay.direlay.path.PathStep
import org.openxplay.direlay.settings.*

@OptIn(ExperimentalSharedTransitionApi::class)
@Composable fun RelayApp(value: HomeContent, sources: ScreenSources, actions: ScreenActions, support: SupportActions,
    shareTrace: (java.io.File) -> Unit,
    onStartStop: () -> Unit, onRestart: () -> Unit, onAutomatic: (Boolean) -> Unit, onSetup: (SetupAction) -> Unit,
    onCopy: (String) -> Unit, onReconnect: () -> Unit,
    onStepAction: (StepAction, PathStep) -> Unit, onRecheck: () -> Unit,
    stepNavigation: StepAction? = null, onStepNavigationHandled: () -> Unit = {},
    importMaterial: (Boolean) -> Unit, removeMaterial: (Boolean) -> Unit, importPublicName: () -> Unit = {},
    openSetup: Boolean = false, openLaunch: Boolean = false, onLater: () -> Unit = {},
    onGuideVisible: (Boolean) -> Unit = {}) {
    val running = value.presentation.running
    val context = LocalContext.current
    val motion by sources.motion.screenState()
    var entries by rememberSaveable { mutableStateOf(when {
        openSetup && sources.settings.value?.setupCompleted == false -> listOf("home", "setup")
        openLaunch -> listOf("home", "launch")
        else -> listOf("home")
    }) }
    val common by sources.settings.screenState()
    val settings = common ?: return
    var setupCompleted by remember { mutableStateOf(settings.setupCompleted) }
    val initial = settings.settings
    val overrides = settings.overrides
    val wireless: (RelaySettings) -> Boolean = remember(settings.lastWireless) {
        { it.connectionMode.wireless(settings.lastWireless) }
    }
    val savedStates = rememberSaveableStateHolder()
    val snackbar = remember { SnackbarHostState() }
    var rendererPicker by rememberSaveable { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val navigationState = remember { SeekableTransitionState(entries) }
    val navigationTransition = rememberTransition(navigationState, label = "relay navigation")
    LaunchedEffect(entries) { navigationState.animateTo(entries) }
    val back: () -> Unit = remember { { entries = RelayBackStack(entries).back().entries } }
    var retained by remember { mutableStateOf(entries) }
    LaunchedEffect(entries) {
        retained.forEachIndexed { index, route ->
            if (index >= entries.size || entries[index] != route) savedStates.removeState("$index:$route")
        }
        retained = entries
    }
    val open: (String) -> Unit = remember(scope) { { route ->
        scope.launch { entries = RelayBackStack(entries).open(route).entries }
        Unit
    } }
    val copy: (String) -> Unit = remember(onCopy, scope, snackbar, context) { { text ->
        onCopy(text)
        scope.launch { snackbar.showSnackbar(context.getString(R.string.settings_copied)) }
        Unit
    } }
    BackHandler(entries.size > 1 && Build.VERSION.SDK_INT < 34) { back() }
    PredictiveBackHandler(entries.size > 1 && Build.VERSION.SDK_INT >= 34) { events ->
        val previous = RelayBackStack(entries).back().entries
        try {
            events.collect { if (motion) navigationState.seekTo(it.progress, previous) }
            entries = previous
        } catch (_: CancellationException) {
            // Scrub back to the original screen; cancellation never changes the saved stack.
            withContext(NonCancellable) { navigationState.animateTo(entries) }
        }
    }
    // Auto start waits while the launch choice or the guide is open, so CarPlay starts only from their buttons.
    LaunchedEffect(entries.last()) { onGuideVisible(entries.last() == "launch" || entries.last().startsWith("setup")) }
    fun completeSetup() {
        if (setupCompleted) return
        setupCompleted = true
        actions.completeSetup()
    }
    // Starting CarPlay from Home also counts as set up, and a running relay needs no launch choice.
    LaunchedEffect(running) {
        if (!running) return@LaunchedEffect
        completeSetup()
        if (entries.last() == "launch") entries = listOf("home")
    }
    fun feedback(applied: AppliedSettings?): Boolean {
        if (applied != null && applied.before.wirelessNetwork.choice != applied.after.wirelessNetwork.choice && running) {
            onRestart()
        } else if (applied != null) {
            val feedback = applied.feedback
            if (feedback != null) scope.launch {
                snackbar.currentSnackbarData?.dismiss()
                val result = snackbar.showSnackbar(context.getString(feedback.message),
                    feedback.action?.let { context.getString(it) }, duration = SnackbarDuration.Long)
                if (result == SnackbarResult.ActionPerformed) {
                    if (feedback.action == R.string.vehicle_reconnect_action) onReconnect() else onRestart()
                }
            }
        }
        return applied != null
    }
    val save: suspend (RelaySettings, RelaySettings) -> Boolean = { _, after -> feedback(actions.save(after)) }
    val saveEditor: suspend (RelaySettings, RelaySettings, DeveloperOverrides) -> Boolean = { _, after, choices ->
        feedback(actions.saveDeveloper(after, choices))
    }
    val settingsEditor = rememberSettingsEditorState(initial, settingsSessionActive(entries), wireless, save,
        initialDeveloper = overrides, saveDeveloper = saveEditor)
    LaunchedEffect(settingsEditor.draft.value) { actions.draft(settingsEditor.draft.value) }
    LaunchedEffect(stepNavigation) {
        val destination = when (stepNavigation) {
            StepAction.FIX_SETTINGS -> firstSettingsErrorPage(settingsEditor.errorPages(wireless))
            StepAction.CHOOSE_IPHONE -> "settings:iphone"
            StepAction.CHOOSE_SHARED_WIFI -> "settings:sharedWifi"
            StepAction.CHOOSE_RENDERER -> { rendererPicker = true; "settings:output" }
            else -> null
        }
        if (destination != null) { open(destination); onStepNavigationHandled() }
    }
    fun stepAction(action: StepAction, step: PathStep) {
        onStepAction(action, step)
        if (action == StepAction.COPY_PAGE_ADDRESS) scope.launch { snackbar.showSnackbar(context.getString(R.string.settings_copied)) }
    }
    CompositionLocalProvider(LocalRelayMotion provides motion, LocalUpdateChecker provides support.updates, LocalAppVersion provides settings.version,
        LocalAppEnvironment provides settings.environment) {
        Box(Modifier.fillMaxSize()) {
            SharedTransitionLayout {
                navigationTransition.AnimatedContent(modifier = Modifier.fillMaxSize(), contentKey = { "${it.lastIndex}:${it.last()}" }, transitionSpec = {
                    relayPageTransition(targetState.size > initialState.size, motion)
                }) { stack ->
                    val route = stack.last()
                    val shared = if (motion) Modifier.sharedBounds(rememberSharedContentState("vehicle"),
                        animatedVisibilityScope = this, enter = EnterTransition.None,
                        exit = ExitTransition.None, boundsTransform = { _, _ -> tween(RelayPageDurationMillis, easing = RelayPageEasing) }) else Modifier
                    val sharedContent = if (motion) Modifier.animateEnterExit(
                        enter = fadeIn(tween(80, delayMillis = RelayPageDurationMillis - 80)), exit = fadeOut(tween(80))) else Modifier
                    savedStates.SaveableStateProvider("${stack.lastIndex}:$route") {
                        when {
                            route == "home" -> HomeScreen(value, { open("settings:$it") }, onStartStop, onAutomatic,
                                copy, ::stepAction, onRecheck, { open("vehicle") }, shared, sharedContent, onRestart,
                                setupCompleted, carRequired = homeCarRowVisible(initial.car)) { open("setup") }
                            route == "launch" -> LaunchScreen(value, onContinue = {
                                // Missing preparation opens the guide where it is finished, then Start.
                                if (value.presentation.canStart) {
                                    entries = listOf("home")
                                    if (!running) onStartStop()
                                } else open("setup:${SetupStep.PREPARE.key}")
                            }, onNewSetup = { open("setup") }) { onLater(); entries = listOf("home") }
                            route.startsWith("setup") -> SetupScreen(initial, wireless, value,
                                setupStep(route.substringAfter(':', "")), actions.readIphones, actions.useSharedWifi,
                                { _, after -> actions.save(after) != null }, onSetup, back, carModels = settings.models, onStepAction = ::stepAction) { changed ->
                                completeSetup()
                                entries = listOf("home")
                                if (!running) onStartStop() else if (changed) onRestart()
                            }
                            route.startsWith("settings") && route != "settings/about" && route != "settings/access/publicName" -> {
                                SettingsScreen(settingsEditor, wireless, route, back, open, copy, importMaterial, removeMaterial,
                                    value.currentWifi, actions.readIphones, actions.useSharedWifi, sources, actions, support,
                                    running = running)
                            }
                            route == "settings/about" -> AboutScreen(back, support.updates)
                            route == "settings/access/publicName" -> {
                                val name by sources.name.screenState()
                                PublicNameScreen(name.view, name.status, name.url, back,
                                    actions.saveName, importPublicName, actions.renewName, actions.removeName)
                            }
                            route == "developer/design" -> org.openxplay.direlay.ui.design.DesignGalleryScreen(back)
                            route == "experimental" -> ExperimentalScreen(settingsEditor, back, wireless)
                            route.startsWith("developer") -> {
                                val diagnostics = if (showsDeveloperDiagnostics(route)) sources.developer.screenState().value else DeveloperView()
                                DeveloperScreen(settingsEditor, diagnostics, route, back, open, copy,
                                onReconnect,
                                settings.sampleVehicle, actions.sampleVehicle, { open("vehicle") }, wireless, sources, actions, shareTrace)
                            }
                            route == "vehicle" || route.startsWith("snapshot:") -> {
                                val time = route.substringAfter(':', "").toLongOrNull()
                                VehiclePage(value.vehicle, value.vehicle.history.firstOrNull { it.id == time },
                                    route.startsWith("snapshot:"), back, { open("snapshot:$it") }, {
                                        scope.launch { save(initial, initial.copy(vehicleInformation = true)) }
                                    },
                                    actions.clearVehicleHistory, if (route == "vehicle") shared else Modifier,
                                    if (route == "vehicle") sharedContent else Modifier)
                            }
                        }
                    }
                }
            }
            if (rendererPicker) DlnaReceiverPicker(settingsEditor.draft.value.dlnaRenderer, actions.searchRenderers,
                dismiss = { rendererPicker = false }) {
                settingsEditor.draft.value = settingsEditor.draft.value.copy(dlnaRenderer = DlnaRendererChoice(it.udn, it.friendlyName))
                rendererPicker = false
            }
            CarlinkCodeSheet(value)
            SnackbarHost(snackbar, Modifier.align(androidx.compose.ui.Alignment.BottomCenter).navigationBarsPadding().padding(16.dp)) { data ->
                Snackbar(data, containerColor = MaterialTheme.colorScheme.surfaceVariant,
                    contentColor = MaterialTheme.colorScheme.onSurface, actionColor = RelayGreen)
            }
        }
    }
}
