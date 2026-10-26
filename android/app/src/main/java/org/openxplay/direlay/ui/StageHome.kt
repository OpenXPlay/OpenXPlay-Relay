// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.openxplay.direlay.R
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.vehicle.VehicleView

/** Home and the opaque running page share presentation, disclosure and step details. */
@Composable internal fun StageHome(path: ConnectionPath, flags: StageFlags, addresses: List<String>,
    problems: List<HomeProblem>, onStepAction: (StepAction, PathStep) -> Unit, onRecheck: () -> Unit,
    onCopy: (String) -> Unit, onStartStop: () -> Unit,
    onStop: () -> Unit = onStartStop, onBack: () -> Unit = {}, onSettings: (String) -> Unit = {},
    onGuide: () -> Unit = {}, onRestart: () -> Unit = {}, onCancelCountdown: (ArrivalEnd) -> Unit = {},
    onCountdownComplete: () -> Unit = {}, automatic: Boolean = false, onAutomatic: (Boolean) -> Unit = {},
    vehicle: VehicleView? = null, onVehicle: () -> Unit = {}, vehicleModifier: Modifier = Modifier,
    vehicleContentModifier: Modifier = Modifier) {
    val view = presentStage(path, flags)
    val pageState = rememberStagePageState()
    val haptic = LocalHapticFeedback.current
    val context = LocalContext.current
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    var expanded by rememberSaveable { mutableStateOf(emptyList<String>()) }
    var fullyExpanded by rememberSaveable { mutableStateOf(emptyList<String>()) }
    var stateKeys by rememberSaveable { mutableStateOf(emptyList<String>()) }
    var pulsed by remember { mutableStateOf<StepId?>(null) }
    var pulse by remember { mutableIntStateOf(0) }
    LaunchedEffect(pulse) {
        if (pulse > 0) { delay(700); pulsed = null }
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
    fun stepAction(action: StepAction, step: PathStep) {
        onCancelCountdown(ArrivalEnd.STEP)
        onStepAction(action, step)
    }
    val drawerKeys: List<Any> = buildList {
        if (!flags.overlay) add("mode")
        addAll(path.sections.map { it.section.name })
        if (!flags.overlay && vehicle != null) add("vehicle")
        if (!flags.overlay) add("automatic")
        add("diagnostic")
    }
    StagePage(path, view, onAction = { action ->
        when (val command = stageCommand(action, path)) {
            StageCommand.Connect -> onStartStop()
            StageCommand.Stop -> {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                onStop()
            }
            StageCommand.CancelArrival -> onCancelCountdown(ArrivalEnd.NOT_NOW)
            is StageCommand.Step -> stepAction(command.action, command.step)
            null -> Unit
        }
    }, onCopy = onCopy, onNotice = { notice ->
        when (stageNoticeTarget(notice)) {
            StageNoticeTarget.RESTART -> onRestart()
            StageNoticeTarget.CAR_SETTINGS -> onSettings("car")
            StageNoticeTarget.GUIDE -> onGuide()
        }
    }, onCountdownComplete = onCountdownComplete, state = pageState,
        modifier = Modifier.background(MaterialTheme.colorScheme.background).statusBarsPadding().displayCutoutPadding(),
        topBar = {
            if (flags.overlay) IconButton(onBack, Modifier.size(48.dp)) {
                Symbol(R.drawable.symbol_back, stringResource(R.string.ui_back))
            } else {
                Spacer(Modifier.weight(1f))
                IconButton({ onSettings("") }, Modifier.size(48.dp)) {
                    Symbol(R.drawable.symbol_settings, stringResource(R.string.direlay_settings))
                }
            }
        }, onNode = { section ->
            expanded = (expanded + section.name).distinct()
            fullyExpanded = (fullyExpanded + section.name).distinct()
            pulsed = focusForPage(path)?.takeIf { it.id.section == section }?.id
            pulse++
        }, onDrawerOpened = { onCancelCountdown(ArrivalEnd.DRAWER) }, drawerItemKeys = drawerKeys, sectionKey = { it.name }) {
        if (!flags.overlay) item("mode") {
            InsetGroup {
                InsetRow(R.drawable.symbol_settings_2, IconHue.DEVELOPER,
                    stringResource(R.string.setup2_set_up_again),
                    path.steps.firstOrNull { it.id == StepId.MODE }?.evidence?.let { stepText(it) }.orEmpty(), onGuide)
            }
        }
        path.sections.forEach { section -> item(section.section.name) {
            PathSectionCard(section, section.section.name in expanded, { open ->
                expanded = if (open) (expanded + section.section.name).distinct() else expanded - section.section.name
                fullyExpanded = if (open) (fullyExpanded + section.section.name).distinct() else fullyExpanded - section.section.name
            }, { selected = it.id.name }, { action ->
                section.steps.firstOrNull { it.action == action }?.let { stepAction(action, it) }
            }, focusedStep = pulsed ?: focusForPage(path)?.id, autoExpand = false,
                pulse = if (pulsed == null) 0 else pulse, onStepAction = ::stepAction,
                fullyExpanded = section.section.name in fullyExpanded)
        } }
        if (!flags.overlay && vehicle != null) item("vehicle") {
            Box(vehicleModifier.fillMaxWidth()) {
                InsetGroup {
                    Box(vehicleContentModifier) {
                        InsetRow(R.drawable.symbol_car, IconHue.CAR, stringResource(R.string.vehicle_title),
                            vehicleSummary(vehicle) { key, args -> context.getString(key, *args.toTypedArray()) }, onVehicle)
                    }
                }
            }
        }
        if (!flags.overlay) item("automatic") {
            InsetGroup {
                InsetToggleRow(title = stringResource(R.string.ui_automatic), checked = automatic, onChange = onAutomatic)
            }
        }
        item("diagnostic") { DiagnosticLine(path.diagnostic, onCopy, path.version, active = pageState.isExpanded) }
    }
    selected?.let { id -> path.steps.firstOrNull { it.id.name == id }?.let { step ->
        StepSheet(stepDetails(step, addresses, problems), { action -> selected = null; stepAction(action, step) },
            onRecheck, onDismiss = { selected = null })
    } }
}
