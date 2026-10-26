// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.ui

import android.os.Build
import android.view.HapticFeedbackConstants
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.*
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.relay.CarModels
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.design.*

/**
 * The guided setup: the car, how it shows CarPlay, how the iPhone connects, the wireless network, the
 * one-time preparation, then Start. Choices are saved as they are made, like Settings, so the
 * preparation step already reflects them; Settings keep the advanced options.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun SetupScreen(initial: RelaySettings, wireless: (RelaySettings) -> Boolean, home: HomeContent,
    startAt: SetupStep?, bondedDevices: suspend () -> org.openxplay.direlay.presentation.IphonePickerView, useSharedWifi: (String) -> Unit,
    persist: suspend (RelaySettings, RelaySettings) -> Boolean, onSetup: (SetupAction) -> Unit,
    close: () -> Unit, carModels: CarModels? = null, onStepAction: (StepAction, PathStep) -> Unit = { _, _ -> }, finish: (changed: Boolean) -> Unit) {
    val entry by rememberSaveable(stateSaver = relaySettingsSaver(initial)) { mutableStateOf(initial) }
    var draft by rememberSaveable(stateSaver = relaySettingsSaver(initial)) { mutableStateOf(setupDraft(initial, wireless(initial))) }
    var lastSaved by rememberSaveable(stateSaver = relaySettingsSaver(initial)) { mutableStateOf(initial) }
    var saveFailed by remember { mutableStateOf(false) }
    var confirmUsb by remember { mutableStateOf(false) }
    if (confirmUsb) UsbConnectionDialog(confirm = {
        draft = chooseConnection(draft, wireless = false)
        confirmUsb = false
    }, dismiss = { confirmUsb = false })
    LaunchedEffect(initial) {
        if (draft == lastSaved) { draft = setupDraft(initial, wireless(initial)); lastSaved = initial }
    }
    LaunchedEffect(draft) {
        if (draft != lastSaved && settingsSaveErrors(draft, wireless(draft)).isEmpty()) {
            val candidate = draft
            if (persist(lastSaved, candidate)) { lastSaved = candidate; saveFailed = false } else saveFailed = true
        }
    }
    val isWireless = wireless(draft)
    val saving = draft != lastSaved && settingsSaveErrors(draft, isWireless).isEmpty()
    val steps = setupSteps(isWireless)
    var stepKey by rememberSaveable { mutableStateOf((startAt ?: SetupStep.CAR).key) }
    // The network page disappears with USB; land on the choice that removed it.
    val step = setupStep(stepKey)?.takeIf { it in steps } ?: SetupStep.CONNECTION
    val index = steps.indexOf(step)
    val motion = LocalRelayMotion.current
    val stepState = remember { SeekableTransitionState(index) }
    val stepTransition = rememberTransition(stepState, label = "setup step")
    LaunchedEffect(index, motion) { if (motion) stepState.animateTo(index) else stepState.snapTo(index) }
    var attempted by rememberSaveable(stepKey) { mutableStateOf(false) }
    var nextRequests by remember { mutableIntStateOf(0) }
    val compact = compactSetup(LocalConfiguration.current.screenHeightDp)
    val errors = settingsErrors(draft, isWireless)
    val running = home.presentation.running
    val changed = settingsNotice(setupDraft(entry, wireless(entry)), draft, carlinkConnected = false) != SettingsNotice.SILENT
    fun go(target: SetupStep) { stepKey = target.key }
    fun previous() { if (index > 0) go(steps[index - 1]) else close() }
    fun next() {
        attempted = true; nextRequests++
        when (step) {
            SetupStep.START -> finish(changed)
            SetupStep.PREPARE -> go(SetupStep.START)
            else -> if (setupStepErrors(step, errors, draft.car).isEmpty() && !saving && !saveFailed) go(steps[index + 1])
        }
    }
    BackHandler(index > 0 && Build.VERSION.SDK_INT < 34) { previous() }
    PredictiveBackHandler(index > 0 && Build.VERSION.SDK_INT >= 34) { events ->
        val previousIndex = index - 1
        try {
            events.collect { if (motion) stepState.seekTo(it.progress, previousIndex) }
            previous()
        } catch (_: CancellationException) {
            withContext(NonCancellable) { if (motion) stepState.animateTo(index) else stepState.snapTo(index) }
        }
    }
    val ready = home.presentation.canStart || running
    val preparePath = home.path
    val prepareChecks = setupChecks(home.presentation, isWireless, draft.wirelessNetwork.choice)
    val prepareComplete = setupPrepareComplete(preparePath, prepareChecks)
    var prepareRequests by remember(stepKey) { mutableIntStateOf(0) }
    val view = LocalView.current
    val nextScale = remember { Animatable(1f) }
    var wasPrepareComplete by remember(stepKey) { mutableStateOf(false) }
    LaunchedEffect(step, prepareComplete, motion) {
        val pulse = step == SetupStep.PREPARE && prepareComplete && !wasPrepareComplete
        wasPrepareComplete = step == SetupStep.PREPARE && prepareComplete
        nextScale.snapTo(1f)
        if (pulse && motion) {
            nextScale.snapTo(.97f)
            nextScale.animateTo(1f, relaySpatial(true))
        }
    }
    val nextEnabled = !saving && when (step) {
        SetupStep.CAR -> draft.car.complete
        SetupStep.PREPARE -> ready
        SetupStep.START -> running || (home.presentation.canStart && !home.busy)
        else -> true
    }
    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = {
        TopAppBar(expandedHeight = if (compact) 52.dp else 72.dp, title = {
            SetupProgressHeader(index, steps.size)
        }, navigationIcon = {
            IconButton(onClick = ::previous) {
                Symbol(R.drawable.symbol_back, stringResource(if (index > 0) R.string.ui_back else R.string.setup_close))
            }
        }, colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background))
    }, bottomBar = {
        Surface(color = MaterialTheme.colorScheme.surface.copy(alpha = .96f)) {
            Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 16.dp, vertical = if (compact) 8.dp else 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally) {
                Column(Modifier.widthIn(max = 720.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    AnimatedVisibility(step == SetupStep.PREPARE && !ready && prepareRequests > 0,
                        enter = relayDisclosureEnter(motion), exit = relayDisclosureExit(motion)) {
                        Box(Modifier.semantics { liveRegion = LiveRegionMode.Polite }) {
                            Hint(stringResource(R.string.setup_prepare_blocked))
                        }
                    }
                    Box(Modifier.graphicsLayer {
                        scaleX = if (motion) nextScale.value else 1f; scaleY = scaleX
                    }) {
                        val label = stringResource(when {
                            step != SetupStep.START -> R.string.setup_next
                            !running -> R.string.setup_start
                            changed -> R.string.setup_apply_restart
                            else -> R.string.setup_done
                        })
                        val reveal = step == SetupStep.PREPARE && !ready && !nextEnabled
                        Box(if (reveal) Modifier.clearAndSetSemantics {} else Modifier) {
                            PrimaryPill(running = false, enabled = nextEnabled, label = label,
                                onClick = ::next, icon = if (step == SetupStep.START) R.drawable.symbol_power else null)
                        }
                        if (reveal) Box(Modifier.matchParentSize().semantics { contentDescription = label }
                            .clickable(role = Role.Button, onClickLabel = label) {
                                prepareRequests++
                                if (Build.VERSION.SDK_INT >= 30) view.performHapticFeedback(HapticFeedbackConstants.REJECT)
                            })
                    }
                }
            }
        }
    }) { padding ->
        stepTransition.AnimatedContent(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding).imePadding(), transitionSpec = {
            relayPageTransition(targetState > initialState, motion)
        }) { shown ->
            val current = steps.getOrElse(shown) { step }
            SetupPageEntrance {
                SetupPage(compact, title = stringResource(when (current) {
                    SetupStep.CAR -> R.string.setup_car_title
                    SetupStep.OUTPUT -> R.string.setup2_output_title
                    SetupStep.CONNECTION -> R.string.setup2_connection_title
                    SetupStep.NETWORK -> R.string.setup2_network_title
                    SetupStep.PREPARE -> R.string.setup_prepare_title
                    SetupStep.START -> if (running) R.string.setup_start_running_title else R.string.setup_start_title
                }), subtitle = stringResource(when (current) {
                    SetupStep.CAR -> R.string.car_choose_hint
                    SetupStep.OUTPUT -> R.string.setup2_output_subtitle
                    SetupStep.CONNECTION -> R.string.setup2_connection_subtitle
                    SetupStep.NETWORK -> R.string.setup2_network_subtitle
                    SetupStep.PREPARE -> R.string.setup2_prepare_subtitle
                    SetupStep.START -> R.string.setup2_start_subtitle
                }), illustration = { if (compact && current != SetupStep.CAR) SetupIllustration(draft, isWireless) }) {
                    when (current) {
                        SetupStep.CAR -> CarPicker(draft.car, carModels, LocalConfiguration.current.locales[0]) { draft = draft.copy(car = it) }
                        SetupStep.OUTPUT -> OutputStep(draft) { choice, kind -> draft = chooseOutput(draft, choice, kind, isWireless) }
                        SetupStep.CONNECTION -> ConnectionStep(draft, isWireless) {
                            val mode = if (it) ConnectionMode.WIRELESS else ConnectionMode.USB
                            if (needsUsbConfirmation(draft.connectionMode, mode, isWireless)) confirmUsb = true
                            else draft = chooseConnection(draft, it)
                        }
                        SetupStep.NETWORK -> NetworkStep(draft, compact, if (attempted) errors else emptyMap(), home.currentWifi, useSharedWifi, nextRequests) { draft = it }
                        SetupStep.PREPARE -> PrepareStep(draft, isWireless, home, preparePath, prepareChecks, prepareComplete,
                            prepareRequests, bondedDevices, onStepAction, { draft = draft.copy(iphoneAddress = it) }) { action ->
                            when (action) {
                                SetupAction.SHARED_WIFI, SetupAction.SETTINGS -> go(if (isWireless) SetupStep.NETWORK else SetupStep.OUTPUT)
                                else -> onSetup(action)
                            }
                        }
                        SetupStep.START -> StartStep(draft, isWireless, ::go) { draft = it }
                    }
                    if (attempted && current != SetupStep.PREPARE && current != SetupStep.START) {
                        // Hotspot fields show their own errors; anything else is reported here.
                        val shownByFields = if (current == SetupStep.NETWORK && draft.wirelessNetwork.choice == WirelessNetworkChoice.PHONE_HOTSPOT)
                            setOf("ssid", "password") else emptySet()
                        setupStepErrors(current, errors, draft.car).filterKeys { it !in shownByFields }.values.firstOrNull()?.let {
                            Text(stringResource(it), color = MaterialTheme.colorScheme.error)
                        }
                    }
                    if (saveFailed) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
}

/** The chosen mode, one row per choice; with [open], each row opens the guide step that changes it. */
@Composable fun ModeSummary(rows: List<ModeRow>, open: ((SetupStep) -> Unit)? = null) {
    rows.forEachIndexed { position, row ->
        if (position > 0) InsetDivider()
        InsetRow(row.icon, when (row.step) {
            SetupStep.OUTPUT -> IconHue.CAR
            SetupStep.CONNECTION -> IconHue.IPHONE
            else -> IconHue.NETWORK
        }, stringResource(row.title), stringResource(row.value), open?.let { { it(row.step) } })
    }
}

@Composable private fun SetupProgressHeader(index: Int, count: Int) {
    val progress = stringResource(R.string.setup_progress, index + 1, count)
    val motion = LocalRelayMotion.current
    Row(Modifier.widthIn(max = 200.dp).fillMaxWidth().clearAndSetSemantics { contentDescription = progress },
        horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        repeat(count) { position ->
            val track = MaterialTheme.colorScheme.outlineVariant
            val color by animateColorAsState(when {
                position < index -> MaterialTheme.colorScheme.primary.copy(alpha = .5f)
                position == index -> MaterialTheme.colorScheme.primary
                else -> track
            }, relayEffects(motion), label = "setup progress color")
            val fill = remember(index) { Animatable(if (position == index && motion) 0f else 1f) }
            LaunchedEffect(index, motion) { fill.animateTo(1f, relaySpatial(motion)) }
            Box(Modifier.weight(1f).height(4.dp).clip(RelayTokens.pill).drawWithContent {
                drawContent()
                if (position == index) drawRoundRect(track, cornerRadius = CornerRadius(size.height / 2))
                val fraction = if (motion) fill.value.coerceIn(0f, 1f) else 1f
                drawRoundRect(color, size = size.copy(width = size.width * fraction),
                    cornerRadius = CornerRadius(size.height / 2))
            })
        }
    }
}

/** Both columns scroll within the scaffold's padding, including above the pinned action and keyboard. */
@Composable internal fun SetupPage(compact: Boolean, title: String, subtitle: String,
    illustration: @Composable () -> Unit = {}, content: @Composable ColumnScope.() -> Unit) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        if (compact) Row(Modifier.widthIn(max = 1040.dp).fillMaxSize().padding(16.dp),
            horizontalArrangement = Arrangement.spacedBy(24.dp)) {
            Column(Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp)) {
                SetupTitle(title, subtitle)
                illustration()
            }
            Column(Modifier.weight(1.25f).fillMaxHeight().verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(16.dp), content = content)
        } else Column(Modifier.widthIn(max = 720.dp).fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp)) {
            SetupTitle(title, subtitle)
            illustration()
            content()
        }
    }
}

@Composable private fun SetupTitle(title: String, subtitle: String) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(title, Modifier.semantics { heading(); liveRegion = LiveRegionMode.Polite }, style = RelayTokens.largeTitle)
        Text(subtitle, style = RelayTokens.footnote, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable private fun SetupIllustration(draft: RelaySettings, wireless: Boolean) {
    if (wireless) SetupNetworkDiagram(draft.wirelessNetwork.choice)
    else Row(Modifier.fillMaxWidth().padding(vertical = 24.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        listOf(R.drawable.symbol_smartphone to R.string.network_iphone,
            R.drawable.symbol_usb to R.string.network_this_phone,
            R.drawable.symbol_car_front to R.string.network_car).forEachIndexed { position, (icon, label) ->
            if (position > 0) Box(Modifier.weight(1f).height(2.dp).background(MaterialTheme.colorScheme.outlineVariant, RelayTokens.pill))
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                IconTile(icon, if (position == 2) IconHue.CAR else IconHue.IPHONE)
                Text(stringResource(label), style = RelayTokens.caption)
            }
        }
    }
}

@Composable private fun SetupNetworkDiagram(choice: WirelessNetworkChoice) {
    val motion = LocalRelayMotion.current
    SetupEntrance(0) {
        AnimatedContent(choice, transitionSpec = {
            if (motion) (fadeIn(relayEffects(true)) + scaleIn(relaySpatial(true), initialScale = .96f)) togetherWith
                fadeOut(relayEffects(true))
            else EnterTransition.None togetherWith ExitTransition.None
        }, label = "setup network") { NetworkDiagram(it, true) }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable private fun ChoiceCard(title: String, description: String, icon: Int, hue: IconHue, selected: Boolean,
    tag: SetupChoiceTag? = null, enabled: Boolean = true, note: String? = null, order: Int = 0,
    extra: @Composable (() -> Unit)? = null, select: () -> Unit) {
    val motion = LocalRelayMotion.current
    SetupEntrance(order) {
        InsetGroup {
            Column(Modifier.fillMaxWidth().alpha(if (enabled) 1f else .5f)
                .background(if (selected) MaterialTheme.colorScheme.primary.copy(alpha = .08f) else MaterialTheme.colorScheme.surface)
                .selectable(selected, enabled = enabled, role = Role.RadioButton, onClick = select)) {
                Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 16.dp, vertical = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    IconTile(icon, hue)
                    FlowRow(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                        Text(title, style = RelayTokens.headline)
                        tag?.let { ChoiceBadge(stringResource(it.label), warning = it.warning) }
                    }
                    RadioButton(selected, onClick = null, enabled = enabled, modifier = Modifier.size(24.dp))
                }
                AnimatedVisibility(selected, enter = relayDisclosureEnter(motion), exit = relayDisclosureExit(motion)) {
                    Column(Modifier.padding(start = 60.dp, end = 16.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(description, style = RelayTokens.callout, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        note?.let { Text(it, style = RelayTokens.footnote, color = LocalStatusColors.current.action) }
                        extra?.invoke()
                    }
                }
            }
        }
    }
}

@Composable fun ChoiceBadge(text: String, modifier: Modifier = Modifier, warning: Boolean = false) {
    val color = if (warning) LocalStatusColors.current.action else MaterialTheme.colorScheme.primary
    Surface(modifier, shape = RelayTokens.chip, color = color.copy(alpha = .14f), contentColor = color) {
        Text(text, Modifier.padding(horizontal = 8.dp, vertical = 2.dp), style = RelayTokens.caption)
    }
}

@Composable private fun OutputStep(draft: RelaySettings, choose: (OutputChoice, WirelessDisplayKind) -> Unit) {
    val carlinkSupported = Build.VERSION.SDK_INT >= 29
    val selected = OutputSelection.choice(draft.outputs)
    val kind = OutputSelection.wirelessKind(draft.outputs)
    if (selected == null) Hint(stringResource(R.string.setup_multiple_outputs))
    Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ChoiceCard(stringResource(R.string.ui_browser), stringResource(R.string.setup_browser_hint), R.drawable.symbol_monitor, IconHue.CAR,
            selected == OutputChoice.BROWSER, setupOutputTag(OutputChoice.BROWSER),
            extra = { Hint(stringResource(R.string.setup_browser_check_tip)) }) { choose(OutputChoice.BROWSER, kind) }
        ChoiceCard(stringResource(R.string.ui_carlink), stringResource(R.string.setup_carlink_hint), R.drawable.symbol_car_front, IconHue.CAR,
            selected == OutputChoice.CARLINK, setupOutputTag(OutputChoice.CARLINK), carlinkSupported,
            if (carlinkSupported) null else stringResource(R.string.setup_carlink_android10), order = 1,
            extra = { NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.WIFI) }) { choose(OutputChoice.CARLINK, kind) }
        ChoiceCard(stringResource(R.string.wireless_display), stringResource(R.string.setup_wireless_display_hint), R.drawable.symbol_cast, IconHue.CAR,
            selected == OutputChoice.WIRELESS, setupOutputTag(OutputChoice.WIRELESS), outputOpen(OutputChoice.WIRELESS), order = 2,
            extra = {
                Column(Modifier.selectableGroup()) {
                    WirelessDisplayKind.entries.forEach { option ->
                        Row(Modifier.fillMaxWidth().heightIn(min = 48.dp)
                            .selectable(kind == option, enabled = outputOpen(OutputChoice.WIRELESS), role = Role.RadioButton,
                                onClick = { choose(OutputChoice.WIRELESS, option) })
                            .padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Text(if (option == WirelessDisplayKind.DLNA) "DLNA" else "Miracast", style = RelayTokens.headline)
                                Hint(stringResource(if (option == WirelessDisplayKind.DLNA) R.string.setup_dlna_hint else R.string.setup_miracast_hint))
                            }
                            RadioButton(kind == option, onClick = null, enabled = outputOpen(OutputChoice.WIRELESS))
                        }
                    }
                }
                if (kind == WirelessDisplayKind.MIRACAST) NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.WIFI)
            }) { choose(OutputChoice.WIRELESS, kind) }
    }
}

@Composable private fun ConnectionStep(draft: RelaySettings, wireless: Boolean, choose: (Boolean) -> Unit) {
    Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ChoiceCard(stringResource(R.string.direlay_wireless), stringResource(R.string.setup_wireless_hint), R.drawable.symbol_bluetooth, IconHue.BLUETOOTH,
            wireless, setupConnectionTag(draft, true), extra = {
                if (CarlinkConnectionPolicy.enabled(draft)) Hint(stringResource(R.string.setup_carlink_hotspot_tip))
            }) { choose(true) }
        ChoiceCard(stringResource(R.string.ui_usb), stringResource(R.string.setup_usb_hint), R.drawable.symbol_usb, IconHue.IPHONE,
            !wireless, setupConnectionTag(draft, false), order = 1) { choose(false) }
    }
}

@Composable private fun NetworkStep(draft: RelaySettings, compact: Boolean, errors: Map<String, Int>, currentWifi: String?,
    useSharedWifi: (String) -> Unit, nextRequests: Int, change: (RelaySettings) -> Unit) {
    val fields = remember { BringIntoViewRequester() }
    var revealRequests by remember { mutableIntStateOf(0) }
    val selected = draft.wirelessNetwork.choice
    LaunchedEffect(selected, revealRequests, nextRequests) {
        if (revealHotspotFields(draft.wirelessNetwork, errors)) {
            withFrameNanos { }
            fields.bringIntoView()
        }
    }
    if (!compact) SetupNetworkDiagram(selected)
    Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        WirelessNetworkChoice.entries.forEachIndexed { index, choice ->
            val allowed = choice != WirelessNetworkChoice.WIFI_DIRECT || CarlinkConnectionPolicy.wifiDirectAllowed(draft)
            ChoiceCard(stringResource(setupNetworkLabel(choice)), stringResource(networkOption(choice).supporting), when (choice) {
                WirelessNetworkChoice.WIFI_DIRECT -> R.drawable.symbol_link
                WirelessNetworkChoice.PHONE_HOTSPOT -> R.drawable.symbol_hotspot
                WirelessNetworkChoice.EXISTING_WIFI -> R.drawable.symbol_car_front
            }, IconHue.NETWORK, selected == choice, setupNetworkTag(draft, choice),
                allowed, if (allowed) null else stringResource(if (draft.miracastOutput && !draft.carlinkOutput)
                    R.string.miracast_wifi_direct_constraint else R.string.carlink_wifi_direct_constraint),
                order = index + 1,
                extra = {
                    if (choice == WirelessNetworkChoice.PHONE_HOTSPOT)
                        NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.HOTSPOT)
                }) {
                change(draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(choice = choice)))
                if (choice == WirelessNetworkChoice.PHONE_HOTSPOT) revealRequests++
            }
        }
    }
    if (selected != WirelessNetworkChoice.WIFI_DIRECT) SetupEntrance(4) {
        InsetGroup {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                NetworkFields(draft.wirelessNetwork, selected, errors, currentWifi, false, useSharedWifi,
                    credentialsModifier = Modifier.bringIntoViewRequester(fields), compactHints = true) {
                    change(draft.copy(wirelessNetwork = it))
                }
            }
        }
    }
}

@Composable private fun PrepareStep(draft: RelaySettings, wireless: Boolean, home: HomeContent,
    path: ConnectionPath?, checks: List<SetupCheck>, complete: Boolean, pulse: Int, bondedDevices: suspend () -> org.openxplay.direlay.presentation.IphonePickerView,
    onStepAction: (StepAction, PathStep) -> Unit, chooseIphone: (String?) -> Unit, act: (SetupAction) -> Unit) {
    val motion = LocalRelayMotion.current
    var inspected by rememberSaveable { mutableStateOf<String?>(null) }
    val shownStep = path?.steps?.firstOrNull { it.id.name == inspected }
    if (shownStep != null) StepSheet(shownStep, onAction = {
        inspected = null
        onStepAction(it, shownStep)
    }, onRecheck = { inspected = null }, onDismiss = { inspected = null })
    // Re-keying the picker opens its sheet, which keeps its own open state.
    var chooseRequests by rememberSaveable { mutableIntStateOf(0) }
    AnimatedVisibility(!complete, enter = relayDisclosureEnter(motion), exit = relayDisclosureExit(motion)) {
        Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            if (path != null) {
                val sections = setupPrepareSections(path)
                val firstOpen = setupPrepareSteps(sections.flatMap { it.steps }).open.firstOrNull()?.id
                sections.forEachIndexed { index, section -> key(section.section) {
                    val steps = setupPrepareSteps(section.steps)
                    val stateKey = section.steps.joinToString { "${it.id}:${it.state}" }
                    var showDone by rememberSaveable(stateKey) { mutableStateOf(false) }
                    SetupEntrance(index) {
                        InsetGroup(header = stringResource(if (section.section == PathSection.RELAY)
                            R.string.path_section_relay else R.string.path_section_iphone)) {
                            Column(Modifier.animateContentSize(relaySpatial(motion))) {
                                if (steps.done.isNotEmpty()) FoldedPrepareLine(steps.done.map { it.title }, showDone) { showDone = !showDone }
                                val visible = if (showDone) section.steps else steps.open
                                visible.forEachIndexed { position, item -> key(item.id) {
                                    if (position > 0 || steps.done.isNotEmpty()) InsetDivider()
                                    PreparePulse(pulse, item.id == firstOpen) {
                                        StepRow(item, onClick = { inspected = item.id.name }, onAction = { onStepAction(it, item) },
                                            focused = item.id == firstOpen)
                                    }
                                } }
                            }
                        }
                    }
                } }
            } else {
                // The first runtime snapshot may still be loading; retain the existing preparation actions.
                val steps = setupPrepareChecks(checks)
                var showDone by rememberSaveable(checks) { mutableStateOf(false) }
                SetupEntrance(0) {
                    InsetGroup {
                        Column(Modifier.animateContentSize(relaySpatial(motion))) {
                            if (steps.done.isNotEmpty()) FoldedPrepareLine(steps.done.map { it.title }, showDone) { showDone = !showDone }
                            val visible = if (showDone) checks else steps.open
                            visible.forEachIndexed { position, check -> key(check.title) {
                                if (position > 0 || steps.done.isNotEmpty()) InsetDivider()
                                PreparePulse(pulse, check == steps.open.firstOrNull()) {
                                    CheckRow(check, home.deviceName, !home.busy) { action ->
                                        if (action == SetupAction.CHOOSE_IPHONE) chooseRequests++ else act(action)
                                    }
                                }
                            } }
                        }
                    }
                }
            }
        }
    }
    AnimatedVisibility(complete, enter = relayDisclosureEnter(motion) + scaleIn(relaySpatial(motion), initialScale = .9f),
        exit = relayDisclosureExit(motion)) {
        SetupEntrance(0) {
            var glyph by remember { mutableStateOf(if (motion) StepState.BLOCKED else StepState.OK) }
            LaunchedEffect(Unit) { glyph = StepState.OK }
            Column(Modifier.fillMaxWidth().padding(vertical = 16.dp), horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp)) {
                StatusGlyph(glyph, 64.dp)
                Text(stringResource(R.string.setup_prepare_ready), Modifier.semantics { liveRegion = LiveRegionMode.Polite }, style = RelayTokens.headline,
                    color = MaterialTheme.colorScheme.primary, textAlign = TextAlign.Center)
            }
        }
    }
    if (wireless) SetupEntrance(2) { key(chooseRequests) {
        IphonePickerSetting(draft.iphoneAddress, bondedDevices, chooseRequests > 0, chooseIphone)
    } }
}

@Composable private fun FoldedPrepareLine(titles: List<Int>, expanded: Boolean, toggle: () -> Unit) {
    val state = stringResource(if (expanded) R.string.menu_expanded else R.string.menu_collapsed)
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).semantics { stateDescription = state }
        .clickable(role = Role.Button, onClick = toggle).padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        StatusGlyph(StepState.OK, 24.dp, announce = false)
        val done = stringResource(R.string.design_done_count, titles.size)
        val names = titles.map { stringResource(it) }.joinToString(" · ")
        val doneStyle = RelayTokens.footnote.toSpanStyle().copy(color = MaterialTheme.colorScheme.onSurface)
        SegmentedText(buildAnnotatedString {
            withStyle(doneStyle) { append(done) }
            append(" · "); append(names)
        }, modifier = Modifier.weight(1f), style = RelayTokens.caption, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable private fun CheckRow(check: SetupCheck, deviceName: String, enabled: Boolean, act: (SetupAction) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        StatusGlyph(when (check.state) {
            SetupCheckState.DONE -> StepState.OK
            SetupCheckState.CHECKING -> StepState.WORKING
            SetupCheckState.TO_DO -> StepState.ACTION
        })
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(stringResource(check.title), style = RelayTokens.headline)
            when (check.state) {
                SetupCheckState.DONE -> Hint(stringResource(R.string.setup_check_done))
                SetupCheckState.CHECKING -> Hint(stringResource(R.string.direlay_checking_identity))
                SetupCheckState.TO_DO -> {
                    check.detail?.let { Hint(stringResource(it)) }
                    if (check.action == SetupAction.IPHONE) Hint(stringResource(R.string.iphone_pair_hint, deviceName))
                    if (check.action != null && check.actionLabel != null) FilledTonalButton(onClick = { act(check.action) },
                        enabled = enabled, modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(check.actionLabel), textAlign = TextAlign.Center) }
                }
            }
        }
    }
}

@Composable private fun StartStep(draft: RelaySettings, wireless: Boolean, go: (SetupStep) -> Unit,
    change: (RelaySettings) -> Unit) {
    SetupEntrance(0) { InsetGroup { ModeSummary(modeRows(draft, wireless), go) } }
    SetupEntrance(1) {
        InsetGroup(footer = stringResource(R.string.settings_automatic_hint)) {
            InsetToggleRow(R.drawable.symbol_sync, IconHue.DEVELOPER, stringResource(R.string.ui_automatic), draft.autoStartWhenReady,
                { change(draft.copy(autoStartWhenReady = it)) })
            InsetDivider()
            InsetToggleRow(R.drawable.symbol_power, IconHue.DEVELOPER, stringResource(R.string.ui_boot), draft.autoStartOnBoot,
                { change(draft.copy(autoStartOnBoot = it)) })
        }
    }
}
