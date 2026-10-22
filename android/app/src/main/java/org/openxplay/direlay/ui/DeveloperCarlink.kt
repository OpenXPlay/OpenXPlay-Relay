// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.RelayRuntime

@Composable
fun DeveloperCarlink(state: SettingsEditorState, debug: RelayRuntime?) {
    var draft by state.draft
    var width by remember(debug) { mutableIntStateOf(debug?.settingsStore?.physicalWidthOverrideMm ?: 0) }
    CollapsibleCard("debug", stringResource(R.string.carlink_debug), menuToggleSummary(draft.carlinkTrace), initiallyExpanded = true) {
        Toggle(stringResource(R.string.carlink_trace_enabled), draft.carlinkTrace, { draft = draft.copy(carlinkTrace = it) })
        if (debug != null) CarlinkDebugPanel(debug, draft.carlinkTrace)
    }
    CollapsibleCard("hevc", stringResource(R.string.carlink_hevc), hevcSummary(draft), initiallyExpanded = false) {
        Options(listOf(stringResource(R.string.carlink_hevc_stream_only), stringResource(R.string.carlink_hevc_off),
            stringResource(R.string.carlink_hevc_signal)), draft.carlinkHevc.ordinal) {
            draft = draft.copy(carlinkHevc = org.openxplay.stream.clink.ClinkHevcMode.entries[it])
        }
        Hint(stringResource(R.string.carlink_hevc_hint))
    }
    CollapsibleCard("size", stringResource(R.string.dev_physical_width), if (width == 0) stringResource(R.string.dev_physical_width_off) else "$width mm", initiallyExpanded = false) {
        if (debug != null) {
            val widths = listOf(0) + (100..400 step 25)
            Text(stringResource(R.string.dev_physical_width))
            Options(widths.map { if (it == 0) stringResource(R.string.dev_physical_width_off) else "$it mm" },
                widths.indexOf(width).coerceAtLeast(0)) { width = widths[it]; debug.settingsStore.physicalWidthOverrideMm = width }
            Hint(stringResource(R.string.dev_physical_width_hint))
        }
    }
    CollapsibleCard("areas", stringResource(R.string.car_view_areas), menuToggleSummary(draft.viewAreas), initiallyExpanded = false) {
        if (debug != null) ViewAreasPanel(debug)
    }
}
