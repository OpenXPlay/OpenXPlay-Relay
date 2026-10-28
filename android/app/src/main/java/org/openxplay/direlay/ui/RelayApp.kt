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
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.*
import org.openxplay.direlay.R
import org.openxplay.direlay.presentation.*
import org.openxplay.direlay.path.StepAction
import org.openxplay.direlay.path.PathStep
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.design.*

@OptIn(ExperimentalSharedTransitionApi::class)
@Composable fun RelayApp(value: HomeContent, sources: ScreenSources, actions: ScreenActions, support: SupportActions,
    shareTrace: (java.io.File) -> Unit,
    onStartStop: () -> Unit, onRestart: () -> Unit, onAutomatic: (Boolean) -> Unit, onSetup: (SetupAction) -> Unit,
    onCopy: (String) -> Unit, onReconnect: () -> Unit,
    onStepAction: (StepAction, PathStep) -> Unit, onRecheck: () -> Unit,
    stepNavigation: StepAction? = null, onStepNavigationHandled: () -> Unit = {},
    homeDestination: String? = null, onHomeDestinationHandled: () -> Unit = {},
    importMaterial: (Boolean) -> Unit, removeMaterial: (Boolean) -> Unit, importPublicName: () -> Unit = {},
    openSetup: Boolean = false, openLaunch: Boolean = false, onLater: () -> Unit = {}, surfaceBlocked: Boolean = false,
    onGuideVisible: (Boolean) -> Unit = {}) {
    val running = value.presentation.running
    val context = LocalContext.current
    val motion by sources.motion.screenState()
    var entries by rememberSaveable(stateSaver = relayEntriesSaver) { mutableStateOf(when {
        openSetup && sources.settings.value?.setupCompleted == false -> listOf("home", "setup")
        else -> listOf("home")
    }) }
    val common by sources.settings.screenState()
    val settings = common ?: return
    var setupCompleted by rememberSaveable { mutableStateOf(settings.setupCompleted) }
    val initial = settings.settings
    val overrides = settings.overrides
    var countdown by rememberSaveable {
        mutableStateOf(openLaunch && settings.setupCompleted && entries.last() == "home" &&
            homeDestination == null && value.automatic && !value.path.running)
    }
    fun endArrival(reason: ArrivalEnd) {
        if (!countdown) return
        if (reason.pauseAutomatic) onLater()
        countdown = false
    }
    val showCountdown = arrivalCountdown(countdown, value.automatic, value.path, value.busy)
    // Readiness checks may finish after the first Home snapshot or after rotation.
    LaunchedEffect(value.automatic, running) {
        if (countdown && running) endArrival(ArrivalEnd.COMPLETE)
        else if (countdown && !value.automatic) endArrival(ArrivalEnd.UNAVAILABLE)
    }
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
            val key = relayPageKey(route)
            if (index >= entries.size || relayPageKey(entries[index]) != key) savedStates.removeState("$index:$key")
        }
        retained = entries
    }
    val open: (String) -> Unit = { route ->
        endArrival(if (route.startsWith("setup")) ArrivalEnd.GUIDE else ArrivalEnd.SETTINGS)
        entries = RelayBackStack(entries).open(route).entries
    }
    LaunchedEffect(homeDestination) {
        homeDestination?.let { open(it); onHomeDestinationHandled() }
    }
    val copy: (String) -> Unit = remember(onCopy, scope, snackbar, context) { { text ->
        onCopy(text)
        scope.launch { snackbar.showSnackbar(context.getString(R.string.settings_copied)) }
        Unit
    } }
    BackHandler(entries.size > 1 && Build.VERSION.SDK_INT < 34) { back() }
    PredictiveBackHandler(entries.size > 1 && Build.VERSION.SDK_INT >= 34) { events ->
        val origin = entries
        val previous = RelayBackStack(origin).back().entries
        try {
            events.collect { if (motion) navigationState.seekTo(it.progress, previous) }
            entries = commitRelayBack(origin, entries)
            if (entries != previous) navigationState.animateTo(entries)
        } catch (_: CancellationException) {
            // Scrub back to the original screen; cancellation never changes the saved stack.
            withContext(NonCancellable) { navigationState.animateTo(entries) }
        }
    }
    // The activity's existing auto-start gate covers the arrival ring and every guide route.
    LaunchedEffect(countdown, entries.last()) { onGuideVisible(countdown || entries.last().startsWith("setup")) }
    fun completeSetup() {
        if (setupCompleted) return
        setupCompleted = true
        actions.completeSetup()
    }
    // Starting CarPlay from Home also counts as set up.
    LaunchedEffect(running) {
        if (!running) return@LaunchedEffect
        completeSetup()
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
                    if (feedback.action == R.string.vehicle_reconnect_action) onReconnect() else {
                        if (applied.before.carlinkPictureDefault != applied.after.carlinkPictureDefault)
                            actions.clearCarlinkPictureTrial(false)
                        onRestart()
                    }
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
        if (stepNavigation == StepAction.CHOOSE_RENDERER) { endArrival(ArrivalEnd.STEP); rendererPicker = true }
        val destination = stepNavigationDestination(stepNavigation, firstSettingsErrorPage(settingsEditor.errorPages(wireless)))
        if (destination != null) open(destination)
        if (stepNavigation != null) onStepNavigationHandled()
    }
    fun stepAction(action: StepAction, step: PathStep) {
        endArrival(ArrivalEnd.STEP)
        onStepAction(action, step)
        if (action == StepAction.COPY_PAGE_ADDRESS) scope.launch { snackbar.showSnackbar(context.getString(R.string.settings_copied)) }
    }
    val sheet = relayAppSheet(rendererPicker, value.pending != null)
    val surfaceHost = remember { mutableStateOf(RelaySurfaceQueue()) }
    CompositionLocalProvider(LocalRelaySurfaceHost provides surfaceHost, LocalRelaySurfaceActive provides !surfaceBlocked,
        LocalOpenRendererPicker provides { rendererPicker = true },
        LocalRelayMotion provides motion, LocalUpdateChecker provides support.updates, LocalAppVersion provides settings.version,
        LocalAppEnvironment provides settings.environment) {
        Box(Modifier.fillMaxSize()) {
            SharedTransitionLayout {
                navigationTransition.AnimatedContent(modifier = Modifier.fillMaxSize(), contentKey = { "${it.lastIndex}:${relayPageKey(it.last())}" }, transitionSpec = {
                    relayPageTransition(targetState.size > initialState.size, motion)
                }) { stack ->
                    val route = stack.last()
                    val shared = if (motion) Modifier.sharedBounds(rememberSharedContentState("vehicle"),
                        animatedVisibilityScope = this, enter = EnterTransition.None,
                        exit = ExitTransition.None, boundsTransform = { _, _ -> tween(RelayPageDurationMillis, easing = RelayPageEasing) }) else Modifier
                    val sharedContent = if (motion) Modifier.animateEnterExit(
                        enter = fadeIn(tween(80, delayMillis = RelayPageDurationMillis - 80)), exit = fadeOut(tween(80))) else Modifier
                    savedStates.SaveableStateProvider("${stack.lastIndex}:${relayPageKey(route)}") {
                        CompositionLocalProvider(LocalRelaySurfaceActive provides (!surfaceBlocked && stack == entries && sheet == null)) {
                            when {
                                route == "home" -> HomeScreen(value, { open("settings:$it") }, {
                                    endArrival(ArrivalEnd.CONNECT)
                                    onStartStop()
                                }, onAutomatic, copy, ::stepAction, onRecheck, { open("vehicle") }, shared, sharedContent, onRestart,
                                    setupCompleted, carRequired = homeCarRowVisible(initial.car), onGuide = { open("setup") },
                                    countdown = showCountdown, onCancelCountdown = ::endArrival,
                                    onCountdownComplete = {
                                        if (arrivalCountdown(countdown, value.automatic, value.path, value.busy)) {
                                            endArrival(ArrivalEnd.COMPLETE)
                                            if (!value.path.running) onStartStop()
                                        }
                                    })
                                route.startsWith("setup") -> SetupScreen(initial, wireless, value,
                                    setupStep(route.substringAfter(':', "")), actions.readIphones, actions.useSharedWifi,
                                    { _, after -> actions.save(after) != null }, onSetup, back, carModels = settings.models, onStepAction = ::stepAction,
                                    onStepChanged = { step ->
                                        if (relayPageKey(entries.last()) == "setup") entries = RelayBackStack(entries).open("setup:${step.key}").entries
                                    }) { changed ->
                                    completeSetup()
                                    entries = RelayBackStack(entries).open("home").entries
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
            }
            if (sheet == RelayAppSheet.RENDERER) DlnaReceiverPicker(settingsEditor.draft.value.dlnaRenderer, actions.searchRenderers,
                dismiss = { rendererPicker = false }) {
                val editing = settingsSessionActive(entries)
                val after = rendererSelection(initial, settingsEditor.draft.value, editing, DlnaRendererChoice(it.udn, it.friendlyName))
                if (editing) settingsEditor.draft.value = after else scope.launch { feedback(actions.save(after)) }
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

internal val relayEntriesSaver = listSaver<List<String>, String>(save = { it }, restore = { RelayBackStack(it).entries })
