// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.BorderStroke
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.vehicle.*
import java.text.DateFormat
import java.util.Date

fun vehicleLabel(key: String): Int = when (key) {
    "gps" -> R.string.vehicle_gps; "lights" -> R.string.vehicle_lights
    "gear" -> R.string.vehicle_gear; "fuel" -> R.string.vehicle_fuel
    "gyroscope" -> R.string.vehicle_gyroscope; "acceleration" -> R.string.vehicle_acceleration
    "light" -> R.string.vehicle_light; "daynight" -> R.string.vehicle_daynight
    "latitude" -> R.string.vehicle_latitude; "longitude" -> R.string.vehicle_longitude
    "altitude" -> R.string.vehicle_altitude; "speed" -> R.string.vehicle_speed
    "heading" -> R.string.vehicle_heading; "antenna" -> R.string.vehicle_antenna
    "pdop" -> R.string.vehicle_pdop; "used" -> R.string.vehicle_used; "visible" -> R.string.vehicle_visible
    "timestamp" -> R.string.vehicle_timestamp; "low" -> R.string.vehicle_low; "high" -> R.string.vehicle_high
    "position" -> R.string.vehicle_position; "reverse" -> R.string.vehicle_reverse; "brake" -> R.string.vehicle_brake
    "max" -> R.string.vehicle_max; "min" -> R.string.vehicle_min; "current" -> R.string.vehicle_current
    "type" -> R.string.vehicle_type; "x" -> R.string.vehicle_x; "y" -> R.string.vehicle_y; "z" -> R.string.vehicle_z
    else -> R.string.vehicle_mode
}
@Composable private fun rawValue(key: String, value: String): String = when {
    value == "true" -> stringResource(R.string.vehicle_on)
    value == "false" -> stringResource(R.string.vehicle_off_value)
    value == "DAY" -> stringResource(R.string.vehicle_day)
    value == "NIGHT" -> stringResource(R.string.vehicle_night)
    value == "SINGLE" -> stringResource(R.string.vehicle_single_axis)
    value == "THREE_AXIS" -> stringResource(R.string.vehicle_three_axes)
    value == "UNKNOWN" -> stringResource(R.string.vehicle_unknown)
    else -> value
}
fun snapshotTime(time: Long): String = DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.MEDIUM).format(Date(time))

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun VehicleScreen(view: VehicleView, now: Long, snapshot: VehicleSnapshot?, isSnapshot: Boolean,
    back: () -> Unit, open: (Long) -> Unit, enable: () -> Unit, clear: () -> Unit,
    modifier: Modifier = Modifier, contentModifier: Modifier = Modifier) {
    var confirm by remember { mutableStateOf(false) }
    val reports = if (isSnapshot) snapshot?.reports.orEmpty() else view.reports
    val state = vehicleScreenState(view, isSnapshot, snapshot)
    Box(modifier) {
        LargeTitleScaffold(stringResource(if (isSnapshot) R.string.vehicle_snapshot else R.string.vehicle_title), back) {
            Column(contentModifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (state != VehicleScreenState.DATA) InsetGroup(footer = stringResource(R.string.vehicle_privacy)) {
                    InsetRow(R.drawable.symbol_car, IconHue.CAR, stringResource(when (state) {
                        VehicleScreenState.OFF -> R.string.vehicle_off
                        VehicleScreenState.WAITING -> R.string.vehicle_waiting
                        else -> R.string.vehicle_snapshot_missing
                    }))
                    if (state == VehicleScreenState.OFF) {
                        InsetDivider()
                        InsetRow(R.drawable.symbol_power, IconHue.CAR, stringResource(R.string.vehicle_enable), onClick = enable)
                    }
                }
                if (state == VehicleScreenState.DATA) {
                    InsetGroup(footer = stringResource(R.string.vehicle_units)) {
                        InsetRow(R.drawable.symbol_car, IconHue.CAR, stringResource(R.string.vehicle_title),
                            if (isSnapshot && snapshot != null) snapshotTime(snapshot.time)
                            else if (view.sample) stringResource(R.string.vehicle_sample) else null)
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            val glance = vehicleGlance(reports)
                            val at = if (isSnapshot) snapshot?.time ?: now else now
                            LargeVehicleValue(stringResource(R.string.vehicle_speed), glance[0])
                            LargeVehicleValue(stringResource(R.string.vehicle_gear_position), rawValue("gear", glance[1]))
                            reports.filter { "speed" in it.fields || it.type == "gear" }.maxByOrNull { it.receivedAt }?.let { ReportAge(it, at) }
                        }
                    }
                    VehicleSignals(reports.filter { it.type in listOf("lights", "daynight") },
                        if (isSnapshot) snapshot?.time ?: now else now)
                    reports.filter { it.type !in listOf("gear", "lights", "daynight") }.forEach { report ->
                        key(report.type) {
                            val at = if (isSnapshot) snapshot?.time ?: now else now
                            InsetGroup(footer = stringResource(R.string.vehicle_updated, (at - report.receivedAt).coerceAtLeast(0) / 1000)) {
                                InsetRow(R.drawable.symbol_car, IconHue.CAR, stringResource(vehicleLabel(report.type)))
                                report.fields.forEach { (key, value) ->
                                    InsetDivider()
                                    InsetRow(title = stringResource(vehicleLabel(key)), value = rawValue(key, value))
                                }
                            }
                        }
                    }
                }
                if (view.storageFailed) Text(stringResource(R.string.vehicle_storage_error), color = MaterialTheme.colorScheme.error)
                if (!isSnapshot && ((state == VehicleScreenState.DATA && !view.sample) || view.history.isNotEmpty() || view.storageFailed)) {
                    InsetGroup(header = stringResource(R.string.vehicle_history), footer = stringResource(R.string.vehicle_history_hint)) {
                        if (view.history.isNotEmpty() || view.storageFailed) {
                            InsetRow(R.drawable.symbol_car, IconHue.CAR, stringResource(R.string.vehicle_clear), onClick = { confirm = true })
                        } else InsetRow(R.drawable.symbol_car, IconHue.CAR, stringResource(R.string.vehicle_history_empty))
                        view.history.forEach { item ->
                            key(item.id) {
                                val glance = vehicleGlance(item.reports)
                                InsetDivider()
                                InsetRow(R.drawable.symbol_car, IconHue.CAR, snapshotTime(item.time),
                                    stringResource(R.string.vehicle_glance, glance[0], glance[1], glance[2]), onClick = { open(item.id) })
                            }
                        }
                    }
                }
            }
        }
    }
    if (confirm) AlertDialog(onDismissRequest = { confirm = false }, title = { Text(stringResource(R.string.vehicle_clear_question)) },
        text = { Text(stringResource(R.string.vehicle_clear_hint)) },
        confirmButton = { TextButton(onClick = { clear(); confirm = false }) { Text(stringResource(R.string.vehicle_clear)) } },
        dismissButton = { TextButton(onClick = { confirm = false }) { Text(stringResource(android.R.string.cancel)) } })
}

@Composable private fun LargeVehicleValue(label: String, value: String) {
    Column {
        Hint(label)
        Text(value, fontSize = 40.sp, fontWeight = FontWeight.SemiBold, fontFamily = FontFamily.Monospace)
    }
}
@Composable private fun ReportAge(report: VehicleReport, now: Long) {
    Hint(stringResource(R.string.vehicle_updated, (now - report.receivedAt).coerceAtLeast(0) / 1000))
}
@OptIn(ExperimentalLayoutApi::class)
@Composable private fun VehicleSignals(reports: List<VehicleReport>, now: Long) {
    if (reports.isEmpty()) return
    InsetGroup {
        reports.forEach { report ->
            InsetRow(R.drawable.symbol_car, IconHue.CAR, stringResource(vehicleLabel(report.type)))
            Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    report.fields.forEach { (key, value) ->
                        val on = report.type == "lights" && value == "true"
                        Surface(shape = MaterialTheme.shapes.small,
                            color = if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surface,
                            contentColor = if (on) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                            border = if (on) null else BorderStroke(1.dp, MaterialTheme.colorScheme.outline)) {
                            Text(if (report.type == "daynight") rawValue(key, value)
                                else stringResource(R.string.vehicle_signal, stringResource(vehicleLabel(key)), rawValue(key, value)), Modifier.padding(horizontal = 12.dp, vertical = 8.dp))
                        }
                    }
                }
            }
        }
        Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
            reports.maxByOrNull { it.receivedAt }?.let { ReportAge(it, now) }
        }
    }
}
