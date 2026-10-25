// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.DisplayMode
import org.openxplay.direlay.settings.DisplayProfile

@Composable
fun DeveloperVideo(state: SettingsEditorState) {
    var draft by state.draft
    val custom by state.customDisplayState
    val ms = if (LocalRelayMotion.current) 220 else 0
    SettingsGroup("display", R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.direlay_display_profile), developerDisplaySummary(draft), initiallyExpanded = true, footer = stringResource(R.string.display_automatic_hint)) {
        val profiles = DisplayProfile.presets
        Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.display_relay_screen)) +
            profiles.map { "${it.width} × ${it.height} · ${it.density} dpi" } + stringResource(R.string.direlay_custom),
            if (custom) profiles.size + 2 else when (draft.displayMode) {
                DisplayMode.AUTOMATIC -> 0
                DisplayMode.RELAY_SCREEN -> 1
                DisplayMode.FIXED -> profiles.indexOf(draft.display) + 2
            }) { index ->
            state.customDisplay = index == profiles.size + 2
            if (custom) state.updateDisplay() else {
                state.clearDisplayErrors()
                draft = draft.copy(
                displayMode = when (index) { 0 -> DisplayMode.AUTOMATIC; 1 -> DisplayMode.RELAY_SCREEN; else -> DisplayMode.FIXED },
                display = if (index < 2) null else profiles[index - 2])
            }
        }

        AnimatedVisibility(custom, enter = expandVertically(tween(ms), expandFrom = Alignment.Top), exit = shrinkVertically(tween(ms), shrinkTowards = Alignment.Top)) {
            Column {
                EditorNumberField(state, "width", R.string.direlay_width, (draft.display?.width ?: 1280).toString(),
                    valid = { displayNumberValid("width", it) }) { value -> state.updateDisplay("width", value) }
                EditorNumberField(state, "height", R.string.direlay_height, (draft.display?.height ?: 720).toString(),
                    valid = { displayNumberValid("height", it) }) { value -> state.updateDisplay("height", value) }
                EditorNumberField(state, "density", R.string.direlay_density, (draft.display?.density ?: 160).toString(),
                    valid = { displayNumberValid("density", it) }) { value -> state.updateDisplay("density", value) }
            }
        }
    }
}
