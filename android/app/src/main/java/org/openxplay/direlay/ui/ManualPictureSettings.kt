// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.ViewAreaReference
import org.openxplay.direlay.relay.ViewAreaSize
import org.openxplay.direlay.relay.WirelessPicturePolicy
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.settings.DisplayMode
import org.openxplay.direlay.ui.design.*

@Composable internal fun PictureReference(text: String, use: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(text, Modifier.weight(1f), style = RelayTokens.footnote, color = MaterialTheme.colorScheme.onSurfaceVariant)
        TextButton(onClick = use, modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.picture_use_reference)) }
    }
}

@Composable internal fun DisplaySizeFields(state: SettingsEditorState) {
    val profile = state.draft.value.display ?: CarDisplayFit.default
    for ((key, label, initial) in listOf(Triple("width", R.string.direlay_width, profile.width),
        Triple("height", R.string.direlay_height, profile.height), Triple("density", R.string.direlay_density, profile.density))) {
        EditorNumberField(state, key, label, initial.toString(), valid = { displayNumberValid(key, it) }) {
            state.updateDisplay(key, it)
        }
    }
}

@Composable internal fun CarPlaySizeSettings(state: SettingsEditorState, reference: DisplayProfile, target: String, openOutput: () -> Unit) {
    var draft by state.draft
    val automatic = draft.displayMode == DisplayMode.AUTOMATIC
    val customWireless = WirelessPicturePolicy.customCanvas(draft)
    SettingsGroup("resolution", R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.picture_resolution), resolutionSummary(draft),
        initiallyExpanded = target.isEmpty() || target == "resolution", targeted = target == "resolution",
        error = listOf("width", "height", "density").any { it in state.invalidNumbers },
        footer = if (!automatic) null else if (customWireless) stringResource(R.string.wireless_custom_resolution,
            draft.wirelessPictureSize.width, draft.wirelessPictureSize.height)
            else if (draft.carplayResolution == CarPlayResolution.NATIVE) stringResource(R.string.picture_native_hint) else null) {
        Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.picture_manual)),
            when (draft.displayMode) { DisplayMode.AUTOMATIC -> 0; DisplayMode.FIXED -> 1; DisplayMode.RELAY_SCREEN -> -1 }) {
            state.chooseDisplayMode(if (it == 0) DisplayMode.AUTOMATIC else DisplayMode.FIXED, reference)
        }
        when (draft.displayMode) {
            DisplayMode.AUTOMATIC -> Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.picture_native), "1080p", "720p", stringResource(R.string.picture_screen)),
                CarPlayResolution.entries.indexOf(draft.carplayResolution)) { draft = draft.copy(carplayResolution = CarPlayResolution.entries[it]) }
            DisplayMode.FIXED -> {
                DisplaySizeFields(state)
                PictureReference(stringResource(R.string.picture_size_reference, reference.width, reference.height, reference.density)) {
                    state.useDisplayReference(reference)
                }
            }
            DisplayMode.RELAY_SCREEN -> Hint(stringResource(R.string.picture_size_developer))
        }
    }
    if (automatic && customWireless) TextButton(onClick = openOutput) { Text(stringResource(R.string.ui_output)) }
}

@Composable internal fun CarPlayWidthSettings(state: SettingsEditorState, reference: DisplayProfile, target: String) {
    var draft by state.draft
    val custom = draft.carplayScreenWidthCustomMm
    val automaticMm = physicalWidth(reference, draft.carplayScreenWidth)
    SettingsGroup("width", R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.picture_screen_width), "${custom ?: draft.carplayScreenWidth.mm} mm",
        initiallyExpanded = false, targeted = target == "width", error = "carplayScreenWidthCustomMm" in state.invalidNumbers,
        footer = stringResource(R.string.picture_screen_width_hint)) {
        Options(CarPlayScreenWidth.entries.map { "${it.mm} mm" } + stringResource(R.string.direlay_custom),
            if (custom != null) CarPlayScreenWidth.entries.size else CarPlayScreenWidth.entries.indexOf(draft.carplayScreenWidth)) { index ->
            state.chooseScreenWidth(CarPlayScreenWidth.entries.getOrNull(index), automaticMm)
        }
        if (custom != null) {
            EditorNumberField(state, "carplayScreenWidthCustomMm", R.string.picture_screen_width_custom, custom.toString(),
                valid = { it.toIntOrNull()?.let(::customScreenWidthValid) == true }) { draft = draft.copy(carplayScreenWidthCustomMm = it.toInt()) }
            PictureReference(stringResource(R.string.picture_screen_width_reference, automaticMm)) { state.useScreenWidthReference(automaticMm) }
        }
    }
}

@Composable internal fun ManualViewAreasSettings(state: SettingsEditorState, reference: ViewAreaReference) {
    val draft by state.draft
    InsetGroup {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.picture_manual)), draft.viewAreasMode.ordinal) {
                state.chooseViewAreas(mode = ViewAreasMode.entries[it])
            }
            if (draft.viewAreasMode == ViewAreasMode.MANUAL) {
                for (full in listOf(true, false)) {
                    SmallHeading(stringResource(if (full) R.string.view_areas_fullscreen else R.string.view_areas_window))
                    val prefix = if (full) "viewAreasFullscreen" else "viewAreasWindowed"
                    val size = if (full) draft.viewAreasFullscreen else draft.viewAreasWindowed
                    for ((suffix, label, initial) in listOf(Triple("Width", R.string.direlay_width, size?.width), Triple("Height", R.string.direlay_height, size?.height))) {
                        val key = prefix + suffix
                        SettingField(stringResource(label), state.numberText[key] ?: initial?.toString().orEmpty(),
                            error = R.string.settings_advanced_error.takeIf { key in state.invalidNumbers }, number = true) { state.updateViewArea(key, it) }
                    }
                }
                if (draft.viewAreasFullscreen == null && draft.viewAreasWindowed == null)
                    Text(stringResource(R.string.view_areas_manual_required), style = RelayTokens.footnote, color = MaterialTheme.colorScheme.error)
                if (reference.empty) Hint(stringResource(R.string.view_areas_reference_empty)) else {
                    val none = stringResource(R.string.view_areas_reference_none)
                    fun label(size: ViewAreaSize?) = size?.let { "${it.width} × ${it.height}" } ?: none
                    PictureReference(stringResource(R.string.view_areas_reference, label(reference.fullscreen), label(reference.windowed))) {
                        state.useViewAreaReference(reference)
                    }
                }
            }
        }
    }
}
