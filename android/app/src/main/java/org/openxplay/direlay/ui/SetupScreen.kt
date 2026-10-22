// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17
package org.openxplay.direlay.ui

import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.bluetooth.BondedDevices
import org.openxplay.direlay.settings.*

/**
 * The guided setup: how the car shows CarPlay, how the iPhone connects, the wireless network, the
 * one-time preparation, then Start. Choices are saved as they are made, like Settings, so the
 * preparation step already reflects them; Settings keep the advanced options.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun SetupScreen(initial: RelaySettings, wireless: (RelaySettings) -> Boolean, home: HomeContent,
    startAt: SetupStep?, bondedDevices: BondedDevices, useSharedWifi: (String) -> Unit,
    persist: suspend (RelaySettings, RelaySettings) -> Boolean, onSetup: (SetupAction) -> Unit,
    close: () -> Unit, finish: (changed: Boolean) -> Unit) {
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
    var attempted by rememberSaveable(stepKey) { mutableStateOf(false) }
    var nextRequests by remember { mutableIntStateOf(0) }
    val compact = compactSetup(LocalConfiguration.current.screenHeightDp)
    val buttonHeight = if (compact) 48.dp else 56.dp
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
    BackHandler(index > 0) { previous() }
    val ready = home.presentation.canStart || running
    val nextEnabled = !saving && when (step) {
        SetupStep.PREPARE -> ready
        SetupStep.START -> running || (home.presentation.canStart && !home.busy)
        else -> true
    }
    val motion = LocalRelayMotion.current
    Scaffold(topBar = {
        Column {
            TopAppBar(expandedHeight = 64.dp * LocalDensity.current.fontScale.coerceAtLeast(1f), title = {
                Column {
                    Text(stringResource(R.string.setup_title), style = MaterialTheme.typography.titleLarge)
                    Text(stringResource(R.string.setup_progress, index + 1, steps.size),
                        style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }, navigationIcon = {
                IconButton(onClick = ::previous) {
                    Symbol(R.drawable.symbol_back, stringResource(if (index > 0) R.string.ui_back else R.string.setup_close))
                }
            })
            LinearProgressIndicator(progress = { (index + 1f) / steps.size }, Modifier.fillMaxWidth(),
                color = RelayGreen, trackColor = MaterialTheme.colorScheme.surfaceVariant)
        }
    }, bottomBar = {
        Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 3.dp) {
            Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 16.dp, vertical = if (compact) 8.dp else 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally) {
                Column(Modifier.widthIn(max = 640.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (!compact && step == SetupStep.PREPARE && !ready) Hint(stringResource(R.string.setup_prepare_blocked))
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        if (index > 0) OutlinedButton(onClick = ::previous, Modifier.heightIn(min = buttonHeight)) {
                            Text(stringResource(R.string.ui_back))
                        }
                        Button(onClick = ::next, enabled = nextEnabled, modifier = Modifier.weight(1f).heightIn(min = buttonHeight),
                            colors = ButtonDefaults.buttonColors(containerColor = RelayGreen)) {
                            if (step == SetupStep.START && !running) {
                                Symbol(R.drawable.symbol_play, null, Modifier.size(24.dp))
                                Spacer(Modifier.width(8.dp))
                            }
                            Text(stringResource(when {
                                step != SetupStep.START -> R.string.setup_next
                                !running -> R.string.setup_start
                                changed -> R.string.setup_apply_restart
                                else -> R.string.setup_done
                            }))
                        }
                    }
                }
            }
        }
    }) { padding ->
        AnimatedContent(index, Modifier.fillMaxSize().padding(padding), transitionSpec = {
            val duration = if (motion) 250 else 0
            val forward = targetState > initialState
            (slideInHorizontally(tween(duration)) { if (forward) it / 6 else -it / 6 } + fadeIn(tween(duration))) togetherWith
                (slideOutHorizontally(tween(duration)) { if (forward) -it / 6 else it / 6 } + fadeOut(tween(duration)))
        }, label = "setup step") { shown ->
            val current = steps.getOrElse(shown) { step }
            Box(Modifier.fillMaxSize().imePadding().verticalScroll(rememberScrollState()), contentAlignment = Alignment.TopCenter) {
                Column(Modifier.widthIn(max = 640.dp).fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    when (current) {
                        SetupStep.OUTPUT -> OutputStep(draft) { draft = chooseOutput(draft, it, isWireless) }
                        SetupStep.CONNECTION -> ConnectionStep(draft, isWireless) {
                            val mode = if (it) ConnectionMode.WIRELESS else ConnectionMode.USB
                            if (needsUsbConfirmation(draft.connectionMode, mode, isWireless)) confirmUsb = true
                            else draft = chooseConnection(draft, it)
                        }
                        SetupStep.NETWORK -> NetworkStep(draft, if (attempted) errors else emptyMap(), home.currentWifi, useSharedWifi, nextRequests) { draft = it }
                        SetupStep.PREPARE -> PrepareStep(draft, isWireless, home, bondedDevices, { draft = draft.copy(iphoneAddress = it) }) { action ->
                            when (action) {
                                SetupAction.SHARED_WIFI, SetupAction.SETTINGS -> go(if (isWireless) SetupStep.NETWORK else SetupStep.OUTPUT)
                                else -> onSetup(action)
                            }
                        }
                        SetupStep.START -> StartStep(draft, isWireless, running, ::go) { draft = it }
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
}

/** The chosen mode, one row per choice; with [open], each row opens the guide step that changes it. */
@Composable fun ModeSummary(rows: List<ModeRow>, open: ((SetupStep) -> Unit)? = null) {
    rows.forEachIndexed { position, row ->
        if (position > 0) HorizontalDivider()
        SettingsRow(stringResource(row.title), stringResource(row.value), row.icon, open?.let { { it(row.step) } })
    }
}

@Composable private fun StepHeader(title: Int, hint: Int) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Heading(stringResource(title))
        Hint(stringResource(hint))
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable private fun ChoiceCard(title: String, description: String, icon: Int, selected: Boolean,
    badges: List<String> = emptyList(), enabled: Boolean = true, note: String? = null,
    warningBadge: Boolean = false,
    extra: @Composable (() -> Unit)? = null, select: () -> Unit) {
    val border = if (selected) RelayGreen else MaterialTheme.colorScheme.outline
    Card(Modifier.fillMaxWidth().alpha(if (enabled) 1f else .5f)
        .selectable(selected, enabled = enabled, role = Role.RadioButton, onClick = select),
        shape = RoundedCornerShape(16.dp), border = BorderStroke(if (selected) 2.dp else 1.dp, border),
        colors = CardDefaults.cardColors(containerColor = if (selected) RelayGreen.copy(alpha = .08f) else MaterialTheme.colorScheme.surface)) {
        Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Symbol(icon, null, Modifier.padding(top = 2.dp).size(28.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(title, Modifier.align(Alignment.CenterVertically), style = MaterialTheme.typography.titleMedium)
                    badges.forEach { ChoiceBadge(it, Modifier.align(Alignment.CenterVertically), warning = warningBadge) }
                }
                Text(description, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                note?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = Color(0xFFFFB547)) }
                extra?.invoke()
            }
            RadioButton(selected, onClick = null, enabled = enabled)
        }
    }
}

@Composable private fun ChoiceBadge(text: String, modifier: Modifier = Modifier, warning: Boolean = false) {
    val color = if (warning) Color(0xFFFFB547) else RelayGreen
    Surface(modifier, shape = RoundedCornerShape(8.dp), color = color.copy(alpha = .15f), contentColor = color) {
        Text(text, Modifier.padding(horizontal = 8.dp, vertical = 2.dp), style = MaterialTheme.typography.labelMedium)
    }
}

@Composable private fun OutputStep(draft: RelaySettings, choose: (Boolean) -> Unit) {
    StepHeader(R.string.setup_output_title, R.string.setup_output_hint)
    val carlinkSupported = Build.VERSION.SDK_INT >= 29
    Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ChoiceCard(stringResource(R.string.ui_browser), stringResource(R.string.setup_browser_hint), R.drawable.symbol_link,
            !draft.carlinkOutput && !draft.bothOutputs, listOf(stringResource(R.string.setup_recommended))) { choose(false) }
        ChoiceCard(stringResource(R.string.ui_carlink), stringResource(R.string.setup_carlink_hint), R.drawable.symbol_car,
            draft.carlinkOutput && !draft.bothOutputs, listOf(stringResource(R.string.setup_experimental)), carlinkSupported,
            if (carlinkSupported) null else stringResource(R.string.setup_carlink_android10),
            extra = { NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.WIFI) }) { choose(true) }
    }
    Hint(stringResource(R.string.setup_browser_check_tip))
}

@Composable private fun ConnectionStep(draft: RelaySettings, wireless: Boolean, choose: (Boolean) -> Unit) {
    StepHeader(R.string.setup_connection_title, R.string.setup_connection_hint)
    val recommended = recommendedConnection(draft)
    val badge = listOf(stringResource(R.string.setup_recommended))
    Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ChoiceCard(stringResource(R.string.direlay_wireless), stringResource(R.string.setup_wireless_hint), R.drawable.symbol_wifi,
            wireless, if (recommended == ConnectionMode.WIRELESS) badge else emptyList()) { choose(true) }
        ChoiceCard(stringResource(R.string.ui_usb), stringResource(R.string.setup_usb_hint), R.drawable.symbol_phone,
            !wireless, listOf(stringResource(R.string.setup_not_recommended)), warningBadge = true) { choose(false) }
    }
    if (CarlinkConnectionPolicy.enabled(draft)) Hint(stringResource(R.string.setup_carlink_hotspot_tip))
}

@Composable private fun NetworkStep(draft: RelaySettings, errors: Map<String, Int>, currentWifi: String?,
    useSharedWifi: (String) -> Unit, nextRequests: Int, change: (RelaySettings) -> Unit) {
    val fields = remember { BringIntoViewRequester() }
    var revealRequests by remember { mutableIntStateOf(0) }
    StepHeader(R.string.setup_network_title, R.string.setup_network_hint)
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
            }, selected == choice, if (choice == recommended) listOf(stringResource(R.string.setup_recommended)) else emptyList(),
                allowed, if (allowed) null else stringResource(R.string.carlink_wifi_direct_constraint),
                extra = {
                    NetworkDiagram(choice, selected == choice)
                    if (choice == WirelessNetworkChoice.PHONE_HOTSPOT)
                        NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.HOTSPOT)
                }) {
                change(draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(choice = choice)))
                if (choice == WirelessNetworkChoice.PHONE_HOTSPOT) revealRequests++
            }
        }
    }
    if (selected != WirelessNetworkChoice.WIFI_DIRECT) RelayCard {
        NetworkFields(draft.wirelessNetwork, selected, errors, currentWifi, false, useSharedWifi, credentialsModifier = Modifier.bringIntoViewRequester(fields)) {
            change(draft.copy(wirelessNetwork = it))
        }
    }
}

@Composable private fun PrepareStep(draft: RelaySettings, wireless: Boolean, home: HomeContent, bondedDevices: BondedDevices,
    chooseIphone: (String?) -> Unit, act: (SetupAction) -> Unit) {
    StepHeader(R.string.setup_prepare_title, R.string.setup_prepare_hint)
    val checks = setupChecks(home.presentation, wireless, draft.wirelessNetwork.choice)
    // Re-keying the picker opens its sheet, which keeps its own open state.
    var chooseRequests by rememberSaveable { mutableIntStateOf(0) }
    RelayCard {
        checks.forEachIndexed { position, check ->
            if (position > 0) HorizontalDivider()
            CheckRow(check, home.deviceName, !home.busy) { action ->
                if (action == SetupAction.CHOOSE_IPHONE) chooseRequests++ else act(action)
            }
        }
    }
    if (wireless) key(chooseRequests) {
        IphonePickerSetting(draft.iphoneAddress, bondedDevices, chooseRequests > 0, chooseIphone)
    }
    if (home.presentation.canStart) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol(R.drawable.symbol_check, null, Modifier.size(20.dp))
        Text(stringResource(R.string.setup_prepare_ready), color = RelayGreen)
    }
}

@Composable private fun CheckRow(check: SetupCheck, deviceName: String, enabled: Boolean, act: (SetupAction) -> Unit) {
    val done = check.state == SetupCheckState.DONE
    Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Surface(Modifier.size(32.dp), shape = CircleShape, color = if (done) RelayGreen else Color.Transparent,
            border = if (done) null else BorderStroke(2.dp, MaterialTheme.colorScheme.onSurfaceVariant)) {
            Box(contentAlignment = Alignment.Center) {
                when (check.state) {
                    SetupCheckState.DONE -> Symbol(R.drawable.symbol_check, null, Modifier.size(20.dp))
                    SetupCheckState.CHECKING -> CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                    SetupCheckState.TO_DO -> Unit
                }
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(stringResource(check.title), style = MaterialTheme.typography.bodyLarge)
            when (check.state) {
                SetupCheckState.DONE -> Hint(stringResource(R.string.setup_check_done))
                SetupCheckState.CHECKING -> Hint(stringResource(R.string.direlay_checking_identity))
                SetupCheckState.TO_DO -> {
                    check.detail?.let { Hint(stringResource(it)) }
                    if (check.action == SetupAction.IPHONE) Hint(stringResource(R.string.iphone_pair_hint, deviceName))
                    if (check.action != null && check.actionLabel != null) FilledTonalButton(onClick = { act(check.action) },
                        enabled = enabled, modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(check.actionLabel)) }
                }
            }
        }
    }
}

@Composable private fun StartStep(draft: RelaySettings, wireless: Boolean, running: Boolean, go: (SetupStep) -> Unit,
    change: (RelaySettings) -> Unit) {
    StepHeader(if (running) R.string.setup_start_running_title else R.string.setup_start_title, R.string.setup_start_hint)
    RelayCard { ModeSummary(modeRows(draft, wireless), go) }
    RelayCard {
        Text(stringResource(R.string.setup_after_start), style = MaterialTheme.typography.titleMedium)
        listOf(if (wireless) R.string.setup_after_wireless else R.string.setup_after_usb,
            if (draft.carlinkOutput) R.string.setup_after_carlink else R.string.setup_after_browser).forEachIndexed { position, line ->
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Surface(Modifier.size(24.dp), shape = CircleShape, color = MaterialTheme.colorScheme.surfaceVariant) {
                    Box(contentAlignment = Alignment.Center) { Text("${position + 1}", style = MaterialTheme.typography.labelMedium) }
                }
                Text(stringResource(line), Modifier.weight(1f))
            }
        }
    }
    RelayCard {
        SettingsRow(stringResource(R.string.ui_automatic), stringResource(R.string.settings_automatic_hint), R.drawable.symbol_sync,
            trailing = { LabeledSwitch(stringResource(R.string.ui_automatic), draft.autoStartWhenReady) { change(draft.copy(autoStartWhenReady = it)) } })
        SettingsRow(stringResource(R.string.ui_boot), stringResource(R.string.direlay_boot_hint), R.drawable.symbol_play,
            trailing = { LabeledSwitch(stringResource(R.string.ui_boot), draft.autoStartOnBoot) { change(draft.copy(autoStartOnBoot = it)) } })
    }
}
