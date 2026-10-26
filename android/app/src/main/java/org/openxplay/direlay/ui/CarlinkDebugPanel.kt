// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18

package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R

@Composable fun CarlinkDebugPanel(view: org.openxplay.direlay.presentation.CarlinkDebugView, enabled: Boolean,
    clearTrace: () -> Unit, capture: (Boolean) -> Unit, share: (java.io.File) -> Unit) {
    val lines = view.lines
    val capturing = view.capturing
    val bytes = view.bytes
    val dropped = view.dropped
    val file = view.file
    val failure = view.failure
    var filter by remember { mutableStateOf(0) }
    InsetGroup {
        InsetRow(R.drawable.symbol_activity, IconHue.DEVELOPER, stringResource(R.string.carlink_debug))
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            val types = listOf(null, "Rtsp", "Frame", "Media", "Link", "Error")
            Options(listOf(R.string.carlink_trace_all, R.string.carlink_trace_rtsp, R.string.carlink_trace_frames,
                R.string.carlink_trace_media, R.string.carlink_trace_link, R.string.carlink_trace_errors).map { stringResource(it) }, filter) { filter = it }
            Text(stringResource(R.string.carlink_trace_counters, lines.size, dropped, bytes))
            lines.lastOrNull { it.type == "Media" }?.let { Text(it.json, style = RelayTokens.mono) }
            failure?.let { Text(stringResource(R.string.carlink_trace_failure, it)) }
            OutlinedButton(enabled = file != null, onClick = { file?.let(share) }) { Text(stringResource(R.string.carlink_trace_share)) }
            TextButton(onClick = clearTrace) { Text(stringResource(R.string.carlink_trace_clear)) }
            OutlinedButton(enabled = enabled || capturing, onClick = { capture(!capturing) }) {
                Text(stringResource(if (capturing) R.string.carlink_capture_stop else R.string.carlink_capture_start))
            }
            LazyColumn(Modifier.fillMaxWidth().height(280.dp)) {
                items(lines.asReversed().filter { types[filter] == null || it.type == types[filter] }) {
                    Text(it.json, style = RelayTokens.mono)
                }
            }
        }
    }
}
