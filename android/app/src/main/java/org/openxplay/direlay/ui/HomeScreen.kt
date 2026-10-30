// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.openxplay.direlay.R
import org.openxplay.direlay.presentation.PendingPin
import org.openxplay.direlay.clink.PinSheetState
import org.openxplay.direlay.network.NetworkReadings
import org.openxplay.direlay.path.*
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.vehicle.*

data class HomeContent(val presentation: HomePresentation, val addresses: List<String>,
    val deviceName: String, val automatic: Boolean, val carlink: Boolean,
    val pending: PendingPin?, val busy: Boolean = false, val vehicle: VehicleView = VehicleView(),
    val pinRejected: Int = 0, val pairingSuccess: Int = 0, val displayMismatch: Boolean = false,
    val currentWifi: String? = null, val networkReadings: NetworkReadings = NetworkReadings(),
    val pathInput: PathInput? = null, val path: ConnectionPath = presentPath(pathInput ?: PathInput()), val stepProblems: List<HomeProblem> = emptyList(),
    val highPerformance: Boolean = true)

@Composable fun HomeScreen(value: HomeContent, onSettings: (String) -> Unit, onStartStop: () -> Unit,
    onAutomatic: (Boolean) -> Unit, onCopy: (String) -> Unit, onStepAction: (StepAction, PathStep) -> Unit,
    onRecheck: () -> Unit, onVehicle: () -> Unit, vehicleModifier: Modifier = Modifier,
    vehicleContentModifier: Modifier = Modifier, onRestart: () -> Unit = {},
    setupCompleted: Boolean = true, carRequired: Boolean = false, onGuide: () -> Unit = {},
    countdown: Boolean = false, onCancelCountdown: (ArrivalEnd) -> Unit = {}, onCountdownComplete: () -> Unit = {}) {
    BackHandler(countdown) { onCancelCountdown(ArrivalEnd.BACK) }
    StageHome(value.path, StageFlags(countdown = countdown, busy = value.busy,
        displayChanged = value.displayMismatch && value.path.running, carIncomplete = carRequired,
        setupCompleted = setupCompleted), value.addresses, value.stepProblems, onStepAction, onRecheck, onCopy,
        onStartStop, onSettings = onSettings, onGuide = onGuide, onRestart = onRestart,
        onCancelCountdown = onCancelCountdown, onCountdownComplete = onCountdownComplete,
        automatic = value.automatic, onAutomatic = onAutomatic,
        vehicle = value.vehicle.takeIf { value.carlink || it.sample }, onVehicle = onVehicle,
        vehicleModifier = vehicleModifier, vehicleContentModifier = vehicleContentModifier, highPerformance = value.highPerformance)
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
        val machine = remember(request) { PinSheetState(request.confirmation, request.respond) }
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
        RelaySheet({ request.cancel(); machine.cancel() }) {
            Column(Modifier.fillMaxWidth().imePadding(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
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
