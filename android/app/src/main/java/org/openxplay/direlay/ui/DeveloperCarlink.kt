// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.runtime.*
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.video.CarlinkGeometry
import org.openxplay.direlay.video.VideoCompatibility
import org.openxplay.stream.clink.ClinkHevcMode
import org.openxplay.stream.clink.ClinkPictureSize

internal fun carlinkGeometryValues(geometry: CarlinkGeometry) = with(geometry) {
    listOf("${window.width}×${window.height}", "${canvas.width}×${canvas.height}",
        "${area.x},${area.y} ${area.width}×${area.height}", "${picture.width}×${picture.height}")
}

@Composable
fun DeveloperCarlink(state: SettingsEditorState, view: DeveloperView, debug: RelayRuntime?) {
    val draft by state.draft
    val overrides by state.developerDraft
    CollapsibleCard("debug", stringResource(R.string.carlink_debug), menuToggleSummary(draft.carlinkTrace), initiallyExpanded = true) {
        Toggle(stringResource(R.string.carlink_trace_enabled), overrides.carlinkTrace, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(carlinkTrace = enabled) }
        }
        view.carlinkGeometry?.let { geometry ->
            val mode = stringResource(when (view.carlinkGeometryMode) {
                VideoCompatibility.Mode.PASSTHROUGH -> R.string.carlink_passthrough
                VideoCompatibility.Mode.TRANSCODE -> R.string.carlink_transcode
                else -> R.string.dev_unavailable
            })
            KeyValue(stringResource(R.string.carlink_geometry), stringResource(R.string.carlink_geometry_value,
                *(carlinkGeometryValues(geometry) + mode).toTypedArray()))
        }
        if (debug != null) CarlinkDebugPanel(debug, draft.carlinkTrace)
    }
    CollapsibleCard("hevc", stringResource(R.string.carlink_hevc), hevcSummary(draft), initiallyExpanded = false) {
        Toggle(stringResource(R.string.carlink_hevc), overrides.carlinkHevcEnabled, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(carlinkHevcEnabled = enabled) }
        }
        DeveloperOverrideContent(overrides.carlinkHevcEnabled) {
            Options(listOf(stringResource(R.string.carlink_hevc_stream_only), stringResource(R.string.carlink_hevc_off),
                stringResource(R.string.carlink_hevc_signal)), overrides.carlinkHevc.ordinal, enabled = overrides.active) { index ->
                state.changeDeveloper { it.copy(carlinkHevc = ClinkHevcMode.entries[index]) }
            }
            Hint(stringResource(R.string.carlink_hevc_hint), enabled = overrides.active)
        }
    }
    CollapsibleCard("picture", stringResource(R.string.carlink_picture_size),
        stringResource(if (draft.carlinkPictureSize == ClinkPictureSize.WINDOW) R.string.carlink_picture_window else R.string.carlink_picture_mode),
        initiallyExpanded = false) {
        Toggle(stringResource(R.string.carlink_picture_size), overrides.carlinkPictureSizeEnabled, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(carlinkPictureSizeEnabled = enabled) }
        }
        DeveloperOverrideContent(overrides.carlinkPictureSizeEnabled) {
            Options(listOf(stringResource(R.string.carlink_picture_window), stringResource(R.string.carlink_picture_mode)),
                overrides.carlinkPictureSize.ordinal, enabled = overrides.active) { index ->
                state.changeDeveloper { it.copy(carlinkPictureSize = ClinkPictureSize.entries[index]) }
            }
            Hint(stringResource(R.string.carlink_picture_size_hint), enabled = overrides.active)
        }
    }
    val width = overrides.effectivePhysicalWidth()
    CollapsibleCard("size", stringResource(R.string.dev_physical_width),
        if (width == 0) stringResource(R.string.dev_physical_width_off) else "$width mm", initiallyExpanded = false) {
        Toggle(stringResource(R.string.dev_physical_width), overrides.physicalWidthEnabled, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(physicalWidthEnabled = enabled,
                physicalWidthOverrideMm = it.physicalWidthOverrideMm.takeIf { value -> value > 0 } ?: 200) }
        }
        DeveloperOverrideContent(overrides.physicalWidthEnabled) {
            val widths = (100..400 step 25).toList()
            Options(widths.map { "$it mm" }, widths.indexOf(overrides.physicalWidthOverrideMm), enabled = overrides.active) { index ->
                state.changeDeveloper { it.copy(physicalWidthOverrideMm = widths[index]) }
            }
            Hint(stringResource(R.string.dev_physical_width_hint), enabled = overrides.active)
        }
    }
    CollapsibleCard("areas", stringResource(R.string.car_view_areas), menuToggleSummary(draft.viewAreas), initiallyExpanded = false) {
        if (debug != null) ViewAreasPanel(debug)
    }
}
