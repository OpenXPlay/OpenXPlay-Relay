// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.os.Build
import android.view.HapticFeedbackConstants
import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import org.openxplay.direlay.R
import org.openxplay.direlay.clink.DialogPinProvider
import org.openxplay.direlay.clink.PinSheetState
import org.openxplay.direlay.network.NetworkReadings
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.vehicle.*

data class HomeContent(val presentation: HomePresentation, val addresses: List<String>,
    val deviceName: String, val automatic: Boolean, val carlink: Boolean,
    val pending: DialogPinProvider.Request?, val busy: Boolean = false, val vehicle: VehicleView = VehicleView(),
    val pinRejected: Int = 0, val pairingSuccess: Int = 0, val displayMismatch: Boolean = false,
    val currentWifi: String? = null, val networkReadings: NetworkReadings = NetworkReadings(),
    val pathInput: PathInput? = null, val stepProblems: List<HomeProblem> = emptyList())

@Composable fun MotionCard(visible: Boolean = true, content: @Composable ColumnScope.() -> Unit) {
    val ms = if (LocalRelayMotion.current) 220 else 0
    val visibility = remember { MutableTransitionState(false) }
    visibility.targetState = visible
    AnimatedVisibility(visibility, enter = fadeIn(tween(ms)) + expandVertically(tween(ms), expandFrom = Alignment.Top),
        exit = fadeOut(tween(ms)) + shrinkVertically(tween(ms), shrinkTowards = Alignment.Top)) { RelayCard(content) }
}

@Composable fun HomeScreen(value: HomeContent, onSettings: (String) -> Unit, onStartStop: () -> Unit,
    onAutomatic: (Boolean) -> Unit, onCopy: (String) -> Unit, onStepAction: (StepAction, PathStep) -> Unit,
    onRecheck: () -> Unit, onVehicle: () -> Unit, vehicleModifier: Modifier = Modifier,
    vehicleContentModifier: Modifier = Modifier, onRestart: () -> Unit = {},
    setupCompleted: Boolean = true, carRequired: Boolean = false, onGuide: () -> Unit = {}) {
    val path = presentPath(value.pathInput ?: PathInput())
    ConnectionPathPage(path, value.addresses, value.stepProblems, onStepAction, onRecheck, onCopy,
        onStop = onStartStop, headerActions = { IconButton({ onSettings("") }) {
            Symbol(R.drawable.symbol_settings, stringResource(R.string.direlay_settings))
        } }, actions = { reveal ->
            val missing = missingStartStep(path)
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                // The disabled pill still directs a tap to the preparation the user must finish.
                Box(Modifier.weight(1f)) {
                    PrimaryPill(path.running, path.running || !value.busy && path.canStart,
                        stringResource(if (path.running) R.string.design_stop else R.string.home2_connect)) {
                        if (path.running || path.canStart) onStartStop() else missing?.let(reveal)
                    }
                    if (!value.busy && missing != null) Box(Modifier.matchParentSize().clickable(
                        role = Role.Button, onClickLabel = stringResource(R.string.home2_connect)) { reveal(missing) })
                }
                AutoSwitch(value.automatic, onAutomatic)
            }
        }, afterFocus = { HomeCarRow(carRequired) { onSettings("car") } }, afterSections = {
            if (value.displayMismatch && path.running) item("display") {
                RelayCard {
                    Text(stringResource(R.string.display_changed), style = RelayTokens.footnote)
                    TextButton(onRestart) { Text(stringResource(R.string.display_reconnect)) }
                }
            }
            if (value.carlink || value.vehicle.sample) item("vehicle") {
                VehicleTile(value, onVehicle, vehicleModifier, vehicleContentModifier)
            }
            if (!setupCompleted) item("guide") {
                TextButton(onGuide, Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                    Text(stringResource(R.string.home2_setup), textAlign = TextAlign.Center)
                }
            }
        })
}

/** Shared layout keeps Home and the opaque host overlay on the same path and disclosure policy. */
@Composable internal fun ConnectionPathPage(path: ConnectionPath, addresses: List<String>, problems: List<HomeProblem>,
    onStepAction: (StepAction, PathStep) -> Unit, onRecheck: () -> Unit, onCopy: (String) -> Unit,
    onStop: () -> Unit, headerActions: @Composable RowScope.() -> Unit = {},
    actions: @Composable ((PathStep) -> Unit) -> Unit,
    afterFocus: @Composable () -> Unit = {},
    afterSections: androidx.compose.foundation.lazy.LazyListScope.() -> Unit = {}) {
    val scroll = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val view = LocalView.current
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    var expanded by rememberSaveable { mutableStateOf(listOf<String>()) }
    var fullyExpanded by rememberSaveable { mutableStateOf(listOf<String>()) }
    var stateKeys by rememberSaveable { mutableStateOf(listOf<String>()) }
    var pulsed by remember { mutableStateOf<StepId?>(null) }
    var pulse by remember { mutableIntStateOf(0) }
    LaunchedEffect(pulse) {
        if (pulse > 0) { kotlinx.coroutines.delay(700); pulsed = null }
    }
    val keys = path.sections.map { section -> section.steps.joinToString { "${it.id}:${it.state}" } }
    LaunchedEffect(keys, path.running) {
        path.sections.forEachIndexed { index, section ->
            if (stateKeys.getOrNull(index) != keys[index]) {
                expanded = expanded - section.section.name
                fullyExpanded = fullyExpanded - section.section.name
                if (expandPathSection(section, path.running)) expanded = expanded + section.section.name
            }
        }
        stateKeys = keys
    }
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val compact = homeWindow(maxWidth.value.toInt(), maxHeight.value.toInt()).inlineActions
            fun reveal(section: PathSection, step: PathStep? = null) {
                expanded = (expanded + section.name).distinct()
                if (step == null) fullyExpanded = (fullyExpanded + section.name).distinct()
                if (step != null) {
                    pulsed = step.id; pulse++
                    view.performHapticFeedback(if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.REJECT
                        else HapticFeedbackConstants.LONG_PRESS)
                }
                val index = path.sections.indexOfFirst { it.section == section }
                if (index >= 0) scope.launch { scroll.animateScrollToItem(index + if (compact) 0 else 2) }
            }
            val revealStep: (PathStep) -> Unit = { reveal(it.id.section, it) }
            val focusStep = focusForPage(path)
            val focus: @Composable () -> Unit = {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    path.live?.let { LiveCard(it, path, onStop, compact = true) }
                        ?: focusStep?.let { step -> FocusCard(step, { onStepAction(it, step) }, onRecheck,
                            { selected = step.id.name }, compact = true) }
                    afterFocus()
                }
            }
            val sections: androidx.compose.foundation.lazy.LazyListScope.() -> Unit = {
                path.sections.forEach { section -> item(section.section.name) {
                    PathSectionCard(section, section.section.name in expanded, { open ->
                        expanded = if (open) (expanded + section.section.name).distinct() else expanded - section.section.name
                        fullyExpanded = if (open) (fullyExpanded + section.section.name).distinct() else fullyExpanded - section.section.name
                    }, { selected = it.id.name }, { action ->
                        section.steps.firstOrNull { it.action == action }?.let { onStepAction(action, it) }
                    }, focusedStep = pulsed ?: focusStep?.id, autoExpand = false, pulse = if (pulsed == null) 0 else pulse,
                        onStepAction = onStepAction, fullyExpanded = section.section.name in fullyExpanded)
                } }
                afterSections()
                item("diagnostic") {
                    DiagnosticLine(path.diagnostic, onCopy)
                }
            }
            Column(Modifier.fillMaxSize().safeDrawingPadding()) {
                Row(Modifier.fillMaxWidth().heightIn(min = if (compact) 44.dp else 52.dp).padding(horizontal = 12.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.relay_brand_first), style = RelayTokens.headline, maxLines = 1)
                    Box(Modifier.weight(1f), contentAlignment = Alignment.CenterEnd) { PathStatusCapsule(path) }
                    headerActions()
                }
                if (compact) Row(Modifier.weight(1f).padding(horizontal = 12.dp, vertical = 2.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        PathMap(path, compact = true) { reveal(it) }
                        focus()
                        actions(revealStep)
                    }
                    LazyColumn(Modifier.weight(1.25f).fillMaxHeight(), state = scroll,
                        verticalArrangement = Arrangement.spacedBy(4.dp), content = sections)
                } else {
                    LazyColumn(Modifier.weight(1f).padding(horizontal = 12.dp), state = scroll,
                        contentPadding = PaddingValues(vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        item("map") { PathMap(path) { reveal(it) } }
                        item("focus") { focus() }
                        sections()
                    }
                    Surface(color = MaterialTheme.colorScheme.surface.copy(alpha = .92f)) {
                        Box(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp)) { actions(revealStep) }
                    }
                }
            }
        }
    }
    selected?.let { id -> path.steps.firstOrNull { it.id.name == id }?.let { step ->
        StepSheet(stepDetails(step, addresses, problems), { action -> selected = null; onStepAction(action, step) },
            onRecheck, onDismiss = { selected = null })
    } }
}

@Composable internal fun PathStatusCapsule(path: ConnectionPath) {
    val text = when {
        path.live != null -> stringResource(R.string.home2_live,
            sessionTime(rememberElapsedSeconds(path.live.sinceMillis)))
        focusForPage(path)?.action == StepAction.START_NOW -> stringResource(R.string.home2_waiting_car)
        path.tone == PathTone.READY -> stringResource(R.string.design_ready)
        path.focus != null -> stringResource(path.focus.title)
        else -> stringResource(R.string.design_idle)
    }
    StatusCapsule(path.tone, text)
}

@Composable private fun VehicleTile(value: HomeContent, open: () -> Unit, modifier: Modifier, contentModifier: Modifier) {
    val context = LocalContext.current
    Card(onClick = open, modifier = modifier.fillMaxWidth()) {
        Row(contentModifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol(R.drawable.symbol_car, null, Modifier.size(20.dp))
            Text(stringResource(R.string.vehicle_title), style = RelayTokens.footnote)
            Text(vehicleSummary(value.vehicle) { key, args -> context.getString(key, *args.toTypedArray()) },
                Modifier.weight(1f), style = RelayTokens.caption,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            Symbol(R.drawable.symbol_chevron, null, Modifier.size(16.dp))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun CarlinkCodeSheet(value: HomeContent) {
    val haptic = LocalHapticFeedback.current
    var deliveredSuccess by remember { mutableIntStateOf(value.pairingSuccess) }
    LaunchedEffect(value.pairingSuccess) {
        if (value.pairingSuccess > deliveredSuccess) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
        deliveredSuccess = value.pairingSuccess
    }
    value.pending?.let { request ->
        val machine = remember(request) { PinSheetState(request.confirmation, request::respond) }
        var digits by remember(request) { mutableStateOf("") }
        val shake = remember { Animatable(0f) }
        val motion = LocalRelayMotion.current
        LaunchedEffect(value.pinRejected) {
            if (request.incorrect && motion) {
                kotlinx.coroutines.delay(200)
                shake.animateTo(0f, keyframes {
                durationMillis = 300; -12f at 50; 12f at 100; -8f at 150; 8f at 200; 0f at 300
                })
            }
        }
        ModalBottomSheet(onDismissRequest = { request.cancel(); machine.cancel() },
            sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
            Column(Modifier.fillMaxWidth().imePadding().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Heading(stringResource(if (request.confirmation) R.string.direlay_carlink_confirm else R.string.ui_pin_title))
                if (request.incorrect) Text(stringResource(R.string.vehicle_wrong_code), color = MaterialTheme.colorScheme.error)
                if (request.confirmation) Text(stringResource(R.string.direlay_carlink_confirm_hint, request.headUnitId))
                else OutlinedTextField(digits, { machine.edit(it); digits = machine.digits },
                    Modifier.fillMaxWidth().offset(x = shake.value.dp), singleLine = true,
                    textStyle = LocalTextStyle.current.copy(fontSize = 32.sp, fontFamily = FontFamily.Monospace),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                    label = { Text(stringResource(R.string.direlay_carlink_pin)) })
                if (machine.finished) Hint(stringResource(R.string.ui_connecting))
                Button(onClick = { machine.submit(); digits = "" }, enabled = machine.canSubmit || (!machine.finished && digits.length == 6),
                    modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) { Text(stringResource(R.string.ui_connect)) }
                TextButton(onClick = { request.cancel(); machine.cancel() }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(android.R.string.cancel)) }
            }
        }
    }
}

val LocalNetworkReadings = compositionLocalOf { NetworkReadings() }

/** Static reminders belong only under choices whose platform state cannot be read. */
@Composable internal fun NetworkChoiceReminder(requirement: org.openxplay.direlay.network.NetworkRequirement) {
    networkChoiceReminder(requirement, LocalNetworkReadings.current)?.let { Hint(stringResource(it)) }
}
