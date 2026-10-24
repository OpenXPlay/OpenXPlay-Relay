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
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.settings.*

private data class AppliedSettings(val before: RelaySettings, val after: RelaySettings, val feedback: SettingsFeedback?)

@OptIn(ExperimentalSharedTransitionApi::class)
@Composable fun RelayApp(value: HomeContent, store: RelaySettingsStore, runtime: RelayRuntime,
    onStartStop: () -> Unit, onRestart: () -> Unit, onAutomatic: (Boolean) -> Unit, onSetup: (SetupAction) -> Unit,
    onCopy: (String) -> Unit, onReconnect: () -> Unit, onNetwork: () -> Unit,
    importMaterial: (Boolean) -> Unit, removeMaterial: (Boolean) -> Unit, importPublicName: () -> Unit = {},
    openSetup: Boolean = false, openLaunch: Boolean = false, onLater: () -> Unit = {},
    onGuideVisible: (Boolean) -> Unit = {}) {
    val context = LocalContext.current
    val motion = rememberRelayMotion()
    var entries by rememberSaveable { mutableStateOf(when {
        openSetup -> listOf("home", "setup")
        openLaunch -> listOf("home", "launch")
        else -> listOf("home")
    }) }
    var setupCompleted by remember { mutableStateOf(store.setupCompleted) }
    val snapshot by store.snapshot.collectAsState()
    val overrides by store.developerOverrides.collectAsState()
    val publicNameView by runtime.publicName.view.collectAsState()
    val publicNameStatus by runtime.publicName.status.collectAsState()
    val initial = checkNotNull(snapshot)
    val savedStates = rememberSaveableStateHolder()
    var diagnostics by remember { mutableStateOf(DeveloperView()) }
    var identityStatus by remember { mutableIntStateOf(R.string.direlay_missing) }
    var adminEnabled by remember { mutableStateOf(runtime.adminService.enabled) }
    var adminUrls by remember { mutableStateOf(runtime.adminService.urls) }
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    val snackbar = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    val navigationState = remember { SeekableTransitionState(entries) }
    val navigationTransition = rememberTransition(navigationState, label = "relay navigation")
    LaunchedEffect(entries) { navigationState.animateTo(entries) }
    fun back() { entries = RelayBackStack(entries).back().entries }
    var retained by remember { mutableStateOf(entries) }
    LaunchedEffect(entries) {
        retained.forEachIndexed { index, route ->
            if (index >= entries.size || entries[index] != route) savedStates.removeState("$index:$route")
        }
        retained = entries
    }
    fun open(route: String) {
        scope.launch {
            entries = RelayBackStack(entries).open(route).entries
        }
    }
    fun copy(text: String) {
        onCopy(text)
        scope.launch { snackbar.showSnackbar(context.getString(R.string.settings_copied)) }
    }
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
    LaunchedEffect(entries.last()) {
        if (showsIdentityStatus(entries.last())) identityStatus = withContext(Dispatchers.IO) { identityStatusKey(runtime.identityStore.status()) }
        while (isActive) {
            now = System.currentTimeMillis()
            adminEnabled = runtime.adminService.enabled
            adminUrls = runtime.adminService.urls
            if (showsDeveloperDiagnostics(entries.last())) {
                diagnostics = withContext(Dispatchers.IO) { developerView(runtime) }
                identityStatus = diagnostics.identity
            }
            delay(500)
        }
    }
    // Auto start waits while the launch choice or the guide is open, so CarPlay starts only from their buttons.
    LaunchedEffect(entries.last()) { onGuideVisible(entries.last() == "launch" || entries.last().startsWith("setup")) }
    fun completeSetup() {
        if (setupCompleted) return
        setupCompleted = true
        scope.launch { withContext(Dispatchers.IO) { store.setupCompleted = true } }
    }
    // Starting CarPlay from Home also counts as set up, and a running relay needs no launch choice.
    LaunchedEffect(value.presentation.running) {
        if (!value.presentation.running) return@LaunchedEffect
        completeSetup()
        if (entries.last() == "launch") entries = listOf("home")
    }
    val saveLock = remember { Mutex() }
    suspend fun persistChange(change: () -> Unit): AppliedSettings? = withContext(NonCancellable + Dispatchers.IO) {
        saveLock.withLock {
            runCatching {
                val appliedBefore = store.awaitSnapshot()
                val widthBefore = store.physicalWidthOverrideMm
                change()
                runtime.reloadSettings()
                val appliedAfter = checkNotNull(store.snapshot.value)
                AppliedSettings(appliedBefore, appliedAfter, settingsFeedback(appliedBefore, appliedAfter,
                    runtime.carlinkConnected, value.presentation.running, widthBefore, store.physicalWidthOverrideMm))
            }.getOrNull()
        }
    }
    suspend fun persist(after: RelaySettings): AppliedSettings? = persistChange {
        val controller = SettingsAutoSave(checkNotNull(store.snapshot.value), store::save)
        controller.apply(after, store.wireless(after))
    }
    fun feedback(applied: AppliedSettings?): Boolean {
        if (applied != null && applied.before.wirelessNetwork.choice != applied.after.wirelessNetwork.choice && value.presentation.running) {
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
    val save: suspend (RelaySettings, RelaySettings) -> Boolean = { _, after -> feedback(persist(after)) }
    val saveEditor: suspend (RelaySettings, RelaySettings, DeveloperOverrides) -> Boolean = { _, after, choices ->
        feedback(persistChange { store.save(after, choices) })
    }
    val settingsEditor = rememberSettingsEditorState(initial, settingsSessionActive(entries), store::wireless, save,
        initialDeveloper = overrides, saveDeveloper = saveEditor)
    CompositionLocalProvider(LocalRelayMotion provides motion) {
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
                            route == "home" -> HomeScreen(value, { open("settings:$it") }, onStartStop, onAutomatic, { action -> if (action == SetupAction.SHARED_WIFI) open("settings:sharedWifi") else if (action == SetupAction.SETTINGS) open(firstSettingsErrorPage(settingsEditor.errorPages(store::wireless))) else if (action == SetupAction.CHOOSE_IPHONE) open("settings:iphone") else onSetup(action) },
                                ::copy, onReconnect, onNetwork, { open("vehicle") }, shared, sharedContent, onRestart,
                                setupCompleted) { step -> open(if (step == null) "setup" else "setup:$step") }
                            route == "launch" -> LaunchScreen(value, onContinue = {
                                // Missing preparation opens the guide where it is finished, then Start.
                                if (value.presentation.canStart) {
                                    entries = listOf("home")
                                    if (!value.presentation.running) onStartStop()
                                } else open("setup:${SetupStep.PREPARE.key}")
                            }, onNewSetup = { open("setup") }) { onLater(); entries = listOf("home") }
                            route.startsWith("setup") -> SetupScreen(initial, store::wireless, value,
                                setupStep(route.substringAfter(':', "")), runtime.bondedDevices, { runtime.sharedWifiSelection.use(it) },
                                { _, after -> persist(after) != null }, onSetup, ::back) { changed ->
                                completeSetup()
                                entries = listOf("home")
                                if (!value.presentation.running) onStartStop() else if (changed) onRestart()
                            }
                            route.startsWith("settings") && route != "settings/about" && route != "settings/access/publicName" -> SettingsScreen(settingsEditor, store::wireless,
                                route, ::back, ::open, identityStatus, diagnostics, ::copy, importMaterial, removeMaterial,
                                value.currentWifi, runtime.bondedDevices, runtime = runtime,
                                accessSummary = androidx.compose.ui.res.stringResource(R.string.menu_access_summary,
                                    menuToggleSummary(adminEnabled), menuToggleSummary(publicNameView.enabled)), useSharedWifi = { runtime.sharedWifiSelection.use(it) }, adminPage = {
                                    AdminPageSettings(adminEnabled, adminUrls, { enabled ->
                                        runtime.adminService.setEnabled(enabled); adminEnabled = enabled
                                    }, ::copy, { withContext(Dispatchers.IO) { runtime.adminService.resetPassword() } })
                                })
                            route == "settings/about" -> AboutScreen(::back)
                            route == "settings/access/publicName" -> PublicNameScreen(publicNameView, publicNameStatus,
                                remember(publicNameStatus) { runtime.publicNameUrl() }, ::back,
                                { edit -> withContext(NonCancellable + Dispatchers.IO) { runCatching { runtime.publicName.save(edit) }.getOrNull() } },
                                importPublicName, runtime.publicName::renewNow,
                                { withContext(NonCancellable + Dispatchers.IO) { runCatching { runtime.publicName.remove() }.isSuccess } })
                            route == "developer/design" -> org.openxplay.direlay.ui.design.DesignGalleryScreen(::back)
                            route == "experimental" -> ExperimentalScreen(settingsEditor, ::back, store::wireless)
                            route.startsWith("developer") -> DeveloperScreen(settingsEditor, diagnostics, route, ::back, ::open, ::copy,
                                onReconnect,
                                runtime.sampleVehicleData, runtime::setSampleVehicleData, { open("vehicle") }, store::wireless, runtime)
                            route == "vehicle" || route.startsWith("snapshot:") -> {
                                val time = route.substringAfter(':', "").toLongOrNull()
                                VehicleScreen(value.vehicle, now, value.vehicle.history.firstOrNull { it.id == time },
                                    route.startsWith("snapshot:"), ::back, { open("snapshot:$it") }, {
                                        scope.launch { save(initial, initial.copy(vehicleInformation = true)) }
                                    },
                                    runtime::clearVehicleHistory, if (route == "vehicle") shared else Modifier,
                                    if (route == "vehicle") sharedContent else Modifier)
                            }
                        }
                    }
                }
            }
            CarlinkCodeSheet(value)
            SnackbarHost(snackbar, Modifier.align(androidx.compose.ui.Alignment.BottomCenter).navigationBarsPadding().padding(16.dp)) { data ->
                Snackbar(data, containerColor = MaterialTheme.colorScheme.surfaceVariant,
                    contentColor = MaterialTheme.colorScheme.onSurface, actionColor = RelayGreen)
            }
        }
    }
}
