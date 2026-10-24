// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.*
import org.json.JSONObject
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.*

@Composable fun ViewAreasPanel(runtime: RelayRuntime) {
    var areas by remember { mutableStateOf(emptyList<ViewAreaSize>()) }
    var preference by remember { mutableStateOf<Boolean?>(null) }
    var records by remember { mutableStateOf(emptyList<WindowSizeRecord>()) }
    var exits by remember { mutableStateOf(emptyList<ReceiverDiagnostics.FullscreenExit>()) }
    var log by remember { mutableStateOf(emptyList<RelayRuntime.AreaSwitch>()) }
    LaunchedEffect(runtime) {
        while (true) {
            areas = runtime.declaredViewAreas
            preference = runtime.viewAreaPreference
            records = withContext(Dispatchers.IO) { runtime.windowRecords.snapshot() }
            log = runtime.areaSwitchLog()
            exits = runtime.status.receivers.fullscreenExits()
            delay(500)
        }
    }
    InsetGroup {
        InsetRow(R.drawable.symbol_monitor, IconHue.CAR, stringResource(R.string.car_view_areas))
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.view_areas_library_in_use, stringResource(when (preference) {
                true -> R.string.view_areas_fullscreen
                false -> R.string.view_areas_window
                null -> R.string.view_areas_all
            })))
            Text(stringResource(R.string.view_areas_declared))
            if (areas.isEmpty()) Text(stringResource(R.string.view_areas_empty))
            areas.forEachIndexed { index, area ->
                Text(stringResource(R.string.view_areas_area, index, area.width, area.height,
                    "%.3f".format(java.util.Locale.ROOT, area.width.toDouble() / area.height)))
                val libraries = area.libraries.map { stringResource(if (it == ViewAreaLibrary.FULLSCREEN)
                    R.string.view_areas_fullscreen else R.string.view_areas_window) }
                Text(stringResource(R.string.view_areas_library, if (index == 0) stringResource(R.string.view_areas_canvas)
                    else if (libraries.isEmpty()) stringResource(R.string.view_areas_all) else libraries.joinToString(" / ")))
                OutlinedButton(onClick = { runtime.showViewArea(index) }) { Text(stringResource(R.string.view_areas_show, index)) }
            }
            log.asReversed().forEach {
                Text(stringResource(if (it.accepted) R.string.view_areas_queued else R.string.view_areas_rejected,
                    snapshotTime(it.time), it.index), style = RelayTokens.mono)
            }
            SmallHeading(stringResource(R.string.view_areas_fullscreen_exits))
            if (exits.isEmpty()) Text(stringResource(R.string.view_areas_no_exits))
            exits.asReversed().forEach {
                val json = remember(it.json) { runCatching { JSONObject(it.json) }.getOrNull() }
                val display = json?.optJSONObject("display")
                val visible = display?.optJSONObject("viewport") ?: display
                val unknown = stringResource(R.string.view_areas_exit_unknown)
                val hidden = if (json == null || json.isNull("hiddenMs")) unknown
                    else stringResource(R.string.view_areas_exit_hidden_ms, json.optDouble("hiddenMs").toString())
                val size = if (visible == null) unknown else "${visible.optInt("width")} × ${visible.optInt("height")}"
                val result = stringResource(when (json?.optString("autoRestore")) {
                    "restored" -> R.string.view_areas_exit_restored
                    "denied" -> R.string.view_areas_exit_denied
                    "off" -> R.string.view_areas_exit_off
                    else -> R.string.view_areas_exit_unknown
                })
                Text("${snapshotTime(it.time)} · ${it.client}", style = RelayTokens.mono)
                Text(stringResource(R.string.view_areas_exit_details, hidden, size, result), style = RelayTokens.mono)
            }
            SmallHeading(stringResource(R.string.view_areas_records))
            TextButton(onClick = runtime::clearWindowRecords) { Text(stringResource(R.string.view_areas_clear)) }
            LazyColumn(Modifier.fillMaxWidth().height(280.dp)) {
                for (full in listOf(true, false)) {
                    item { SmallHeading(stringResource(if (full) R.string.view_areas_fullscreen else R.string.view_areas_window)) }
                    items(records.filter { it.fullscreen == full }) { record ->
                        Text(stringResource(R.string.view_areas_record, record.screenWidth, record.screenHeight,
                            "%.2f".format(java.util.Locale.ROOT, record.densityDpi / 160.0), record.width, record.height,
                            stringResource(if (record.fullscreen) R.string.view_areas_fullscreen else R.string.view_areas_window),
                            record.count, snapshotTime(record.lastSeen)), style = RelayTokens.mono)
                    }
                }
            }
        }
    }
}
