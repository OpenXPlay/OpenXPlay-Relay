// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.ui

import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.*
import org.openxplay.direlay.R
import org.openxplay.direlay.bluetooth.BondedDevices
import org.openxplay.direlay.path.*
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.design.*

/**
 * The guided setup: how the car shows CarPlay, how the iPhone connects, the wireless network, the
 * one-time preparation, then Start. Choices are saved as they are made, like Settings, so the
 * preparation step already reflects them; Settings keep the advanced options.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun SetupScreen(initial: RelaySettings, wireless: (RelaySettings) -> Boolean, home: HomeContent,
    startAt: SetupStep?, bondedDevices: BondedDevices, useSharedWifi: (String) -> Unit,
    persist: suspend (RelaySettings, RelaySettings) -> Boolean, onSetup: (SetupAction) -> Unit,
    close: () -> Unit, onStepAction: (StepAction, PathStep) -> Unit = { _, _ -> }, finish: (changed: Boolean) -> Unit) {
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
    var stepKey by rememberSaveable { mutableStateOf((startAt ?: SetupStep.OUTPUT).key) }
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
            else -> if (setupStepErrors(step, errors).isEmpty() && !saving && !saveFailed) go(steps[index + 1])
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
    val nextEnabled = !saving && when (step) {
        SetupStep.PREPARE -> ready
        SetupStep.START -> running || (home.presentation.canStart && !home.busy)
        else -> true
    }
    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = {
        TopAppBar(expandedHeight = if (compact) 52.dp else 72.dp, title = {
            SetupProgressHeader(index, steps.size, compact)
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
                    if (!compact && step == SetupStep.PREPARE && !ready) Hint(stringResource(R.string.setup_prepare_blocked))
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        if (index > 0) TextButton(onClick = ::previous, Modifier.heightIn(min = 56.dp)) {
                            Text(stringResource(R.string.ui_back), textAlign = TextAlign.Center)
                        }
                        Box(Modifier.weight(1f)) {
                            PrimaryPill(running = false, enabled = nextEnabled, label = stringResource(when {
                                step != SetupStep.START -> R.string.setup_next
                                !running -> R.string.setup_start
                                changed -> R.string.setup_apply_restart
                                else -> R.string.setup_done
                            }), onClick = ::next, icon = if (step == SetupStep.START) R.drawable.symbol_power else null)
                        }
                    }
                }
            }
        }
    }) { padding ->
        stepTransition.AnimatedContent(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding).imePadding(), transitionSpec = {
            relayPageTransition(targetState > initialState, motion)
        }) { shown ->
            val current = steps.getOrElse(shown) { step }
            SetupPage(compact, title = stringResource(when (current) {
                SetupStep.OUTPUT -> R.string.setup2_output_title
                SetupStep.CONNECTION -> R.string.setup2_connection_title
                SetupStep.NETWORK -> R.string.setup2_network_title
                SetupStep.PREPARE -> R.string.setup_prepare_title
                SetupStep.START -> if (running) R.string.setup_start_running_title else R.string.setup_start_title
            }), subtitle = stringResource(when (current) {
                SetupStep.OUTPUT -> R.string.setup2_output_subtitle
                SetupStep.CONNECTION -> R.string.setup2_connection_subtitle
                SetupStep.NETWORK -> R.string.setup2_network_subtitle
                SetupStep.PREPARE -> R.string.setup2_prepare_subtitle
                SetupStep.START -> R.string.setup2_start_subtitle
            }), illustration = { if (compact) SetupIllustration(draft, isWireless) }) {
                when (current) {
                    SetupStep.OUTPUT -> OutputStep(draft) { choice, kind -> draft = chooseOutput(draft, choice, kind, isWireless) }
                    SetupStep.CONNECTION -> ConnectionStep(draft, isWireless) {
                        val mode = if (it) ConnectionMode.WIRELESS else ConnectionMode.USB
                        if (needsUsbConfirmation(draft.connectionMode, mode, isWireless)) confirmUsb = true
                        else draft = chooseConnection(draft, it)
                    }
                    SetupStep.NETWORK -> NetworkStep(draft, compact, if (attempted) errors else emptyMap(), home.currentWifi, useSharedWifi, nextRequests) { draft = it }
                    SetupStep.PREPARE -> PrepareStep(draft, isWireless, home, bondedDevices, onStepAction, { draft = draft.copy(iphoneAddress = it) }) { action ->
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
                    setupStepErrors(current, errors).filterKeys { it !in shownByFields }.values.firstOrNull()?.let {
                        Text(stringResource(it), color = MaterialTheme.colorScheme.error)
                    }
                }
                if (compact && current == SetupStep.PREPARE && !ready) Hint(stringResource(R.string.setup_prepare_blocked))
                if (saveFailed) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
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

@Composable private fun SetupProgressHeader(index: Int, count: Int, compact: Boolean) {
    val progress = stringResource(R.string.setup_progress, index + 1, count)
    Column(Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = progress },
        verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            repeat(count) { position ->
                val width by animateDpAsState(if (position == index) 48.dp else 24.dp,
                    relaySpatial(LocalRelayMotion.current), label = "setup progress width")
                val color by animateColorAsState(if (position <= index) MaterialTheme.colorScheme.primaryContainer
                    else MaterialTheme.colorScheme.surfaceContainerHigh, relayEffects(LocalRelayMotion.current), label = "setup progress color")
                Surface(Modifier.width(width).heightIn(min = 24.dp), shape = RelayTokens.pill, color = color,
                    contentColor = if (position <= index) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant) {
                    Box(contentAlignment = Alignment.Center) { Text("${position + 1}", style = RelayTokens.caption) }
                }
            }
        }
        if (!compact) Text(progress, style = RelayTokens.caption, color = MaterialTheme.colorScheme.onSurfaceVariant)
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
        Text(title, Modifier.semantics { heading() }, style = RelayTokens.largeTitle)
        Text(subtitle, style = RelayTokens.footnote, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable private fun SetupIllustration(draft: RelaySettings, wireless: Boolean) {
    if (wireless) NetworkDiagram(draft.wirelessNetwork.choice, true)
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

@OptIn(ExperimentalLayoutApi::class)
@Composable private fun ChoiceCard(title: String, description: String, icon: Int, hue: IconHue, selected: Boolean,
    badges: List<String> = emptyList(), enabled: Boolean = true, note: String? = null,
    warningBadge: Boolean = false,
    extra: @Composable (() -> Unit)? = null, select: () -> Unit) {
    InsetGroup {
        Column(Modifier.fillMaxWidth().alpha(if (enabled) 1f else .5f)
            .background(if (selected) MaterialTheme.colorScheme.primary.copy(alpha = .08f) else MaterialTheme.colorScheme.surface)
            .selectable(selected, enabled = enabled, role = Role.RadioButton, onClick = select)) {
            Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 16.dp, vertical = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                IconTile(icon, hue)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(title, style = RelayTokens.headline)
                    if (badges.isNotEmpty()) FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        badges.forEach { ChoiceBadge(it, warning = warningBadge) }
                    }
                }
                RadioButton(selected, onClick = null, enabled = enabled)
            }
            Column(Modifier.padding(start = 60.dp, end = 16.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(description, style = RelayTokens.callout, color = MaterialTheme.colorScheme.onSurfaceVariant)
                note?.let { Text(it, style = RelayTokens.footnote, color = LocalStatusColors.current.action) }
                extra?.invoke()
            }
        }
    }
}

@Composable private fun ChoiceBadge(text: String, modifier: Modifier = Modifier, warning: Boolean = false) {
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
            selected == OutputChoice.BROWSER, listOf(stringResource(R.string.setup_recommended))) { choose(OutputChoice.BROWSER, kind) }
        ChoiceCard(stringResource(R.string.ui_carlink), stringResource(R.string.setup_carlink_hint), R.drawable.symbol_car_front, IconHue.CAR,
            selected == OutputChoice.CARLINK, listOf(stringResource(R.string.setup_experimental)), carlinkSupported,
            if (carlinkSupported) null else stringResource(R.string.setup_carlink_android10), warningBadge = true,
            extra = { NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.WIFI) }) { choose(OutputChoice.CARLINK, kind) }
        ChoiceCard(stringResource(R.string.wireless_display), stringResource(R.string.setup_wireless_display_hint), R.drawable.symbol_cast, IconHue.CAR,
            selected == OutputChoice.WIRELESS, listOf(stringResource(R.string.setup_picture_only)), warningBadge = true,
            extra = if (selected == OutputChoice.WIRELESS) ({
                Column(Modifier.selectableGroup()) {
                    WirelessDisplayKind.entries.forEach { option ->
                        Row(Modifier.fillMaxWidth().heightIn(min = 48.dp)
                            .selectable(kind == option, role = Role.RadioButton, onClick = { choose(OutputChoice.WIRELESS, option) })
                            .padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Text(if (option == WirelessDisplayKind.DLNA) "DLNA" else "Miracast", style = RelayTokens.headline)
                                Hint(stringResource(if (option == WirelessDisplayKind.DLNA) R.string.setup_dlna_hint else R.string.setup_miracast_hint))
                            }
                            RadioButton(kind == option, onClick = null)
                        }
                    }
                }
                if (kind == WirelessDisplayKind.MIRACAST) NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.WIFI)
            }) else null) { choose(OutputChoice.WIRELESS, kind) }
    }
    if (selected == OutputChoice.BROWSER || selected == null) Hint(stringResource(R.string.setup_browser_check_tip))
}

@Composable private fun ConnectionStep(draft: RelaySettings, wireless: Boolean, choose: (Boolean) -> Unit) {
    val recommended = recommendedConnection(draft)
    val badge = listOf(stringResource(R.string.setup_recommended))
    Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ChoiceCard(stringResource(R.string.direlay_wireless), stringResource(R.string.setup_wireless_hint), R.drawable.symbol_bluetooth, IconHue.BLUETOOTH,
            wireless, if (recommended == ConnectionMode.WIRELESS) badge else emptyList()) { choose(true) }
        ChoiceCard(stringResource(R.string.ui_usb), stringResource(R.string.setup_usb_hint), R.drawable.symbol_usb, IconHue.IPHONE,
            !wireless, listOf(stringResource(R.string.setup_not_recommended)), warningBadge = true) { choose(false) }
    }
    if (CarlinkConnectionPolicy.enabled(draft)) Hint(stringResource(R.string.setup_carlink_hotspot_tip))
}

@Composable private fun NetworkStep(draft: RelaySettings, compact: Boolean, errors: Map<String, Int>, currentWifi: String?,
    useSharedWifi: (String) -> Unit, nextRequests: Int, change: (RelaySettings) -> Unit) {
    val fields = remember { BringIntoViewRequester() }
    var revealRequests by remember { mutableIntStateOf(0) }
    val recommended = recommendedNetwork(draft)
    val selected = draft.wirelessNetwork.choice
    LaunchedEffect(selected, revealRequests, nextRequests) {
        if (revealHotspotFields(draft.wirelessNetwork, errors)) {
            withFrameNanos { }
            fields.bringIntoView()
        }
    }
    Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        WirelessNetworkChoice.entries.forEach { choice ->
            val allowed = choice != WirelessNetworkChoice.WIFI_DIRECT || CarlinkConnectionPolicy.wifiDirectAllowed(draft)
            ChoiceCard(stringResource(setupNetworkLabel(choice)), stringResource(networkOption(choice).supporting), when (choice) {
                WirelessNetworkChoice.WIFI_DIRECT -> R.drawable.symbol_link
                WirelessNetworkChoice.PHONE_HOTSPOT -> R.drawable.symbol_hotspot
                WirelessNetworkChoice.EXISTING_WIFI -> R.drawable.symbol_router
            }, IconHue.NETWORK, selected == choice, if (choice == recommended) listOf(stringResource(R.string.setup_recommended)) else emptyList(),
                allowed, if (allowed) null else stringResource(if (draft.miracastOutput && !draft.carlinkOutput)
                    R.string.miracast_wifi_direct_constraint else R.string.carlink_wifi_direct_constraint),
                extra = {
                    if (!compact) NetworkDiagram(choice, selected == choice)
                    if (choice == WirelessNetworkChoice.PHONE_HOTSPOT)
                        NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.HOTSPOT)
                }) {
                change(draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(choice = choice)))
                if (choice == WirelessNetworkChoice.PHONE_HOTSPOT) revealRequests++
            }
        }
    }
    if (selected != WirelessNetworkChoice.WIFI_DIRECT) InsetGroup {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            NetworkFields(draft.wirelessNetwork, selected, errors, currentWifi, false, useSharedWifi, credentialsModifier = Modifier.bringIntoViewRequester(fields)) {
                change(draft.copy(wirelessNetwork = it))
            }
        }
    }
}

@Composable private fun PrepareStep(draft: RelaySettings, wireless: Boolean, home: HomeContent, bondedDevices: BondedDevices,
    onStepAction: (StepAction, PathStep) -> Unit, chooseIphone: (String?) -> Unit, act: (SetupAction) -> Unit) {
    val path = remember(home.pathInput) { home.pathInput?.let { presentPath(it) } }
    var inspected by rememberSaveable { mutableStateOf<String?>(null) }
    val shownStep = path?.steps?.firstOrNull { it.id.name == inspected }
    if (shownStep != null) StepSheet(shownStep, onAction = {
        inspected = null
        onStepAction(it, shownStep)
    }, onRecheck = { inspected = null }, onDismiss = { inspected = null })
    // Re-keying the picker opens its sheet, which keeps its own open state.
    var chooseRequests by rememberSaveable { mutableIntStateOf(0) }
    if (path != null) {
        setupPrepareSections(path).forEach { section ->
            InsetGroup(header = stringResource(if (section.section == PathSection.RELAY) R.string.path_section_relay else R.string.path_section_iphone)) {
                section.steps.forEachIndexed { position, item ->
                    if (position > 0) InsetDivider()
                    StepRow(item, onClick = { inspected = item.id.name }, onAction = { onStepAction(it, item) },
                        focused = path.focus?.id == item.id)
                }
            }
        }
    } else {
        // The first runtime snapshot may still be loading; retain the existing preparation actions.
        InsetGroup {
            setupChecks(home.presentation, wireless, draft.wirelessNetwork.choice).forEachIndexed { position, check ->
                if (position > 0) InsetDivider()
                CheckRow(check, home.deviceName, !home.busy) { action ->
                    if (action == SetupAction.CHOOSE_IPHONE) chooseRequests++ else act(action)
                }
            }
        }
    }
    if (wireless) key(chooseRequests) {
        IphonePickerSetting(draft.iphoneAddress, bondedDevices, chooseRequests > 0, chooseIphone)
    }
    if (home.presentation.canStart) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol(R.drawable.symbol_check, null, Modifier.size(20.dp))
        Text(stringResource(R.string.setup_prepare_ready), style = RelayTokens.callout, color = MaterialTheme.colorScheme.primary)
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
    InsetGroup { ModeSummary(modeRows(draft, wireless), go) }
    InsetGroup(header = stringResource(R.string.setup_after_start)) {
        afterStartLines(draft, wireless).forEachIndexed { position, line ->
            if (position > 0) InsetDivider()
            Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Surface(Modifier.size(24.dp), shape = CircleShape, color = MaterialTheme.colorScheme.primaryContainer,
                    contentColor = MaterialTheme.colorScheme.onPrimaryContainer) {
                    Box(contentAlignment = Alignment.Center) { Text("${position + 1}", style = RelayTokens.caption) }
                }
                Text(stringResource(line), Modifier.weight(1f), style = RelayTokens.callout)
            }
        }
    }
    InsetGroup(footer = stringResource(R.string.settings_automatic_hint) + "\n" + stringResource(R.string.direlay_boot_hint)) {
        InsetToggleRow(R.drawable.symbol_sync, IconHue.DEVELOPER, stringResource(R.string.ui_automatic), draft.autoStartWhenReady,
            { change(draft.copy(autoStartWhenReady = it)) })
        InsetDivider()
        InsetToggleRow(R.drawable.symbol_power, IconHue.DEVELOPER, stringResource(R.string.ui_boot), draft.autoStartOnBoot,
            { change(draft.copy(autoStartOnBoot = it)) })
    }
}
