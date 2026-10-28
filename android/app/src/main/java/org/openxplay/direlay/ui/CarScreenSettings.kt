// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.widget.Toast
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.presentation.CarScreenView

@Composable fun CarScreenSettings(draft: RelaySettings, view: CarScreenView, wireless: (RelaySettings) -> Boolean,
    clearHeadUnit: () -> Unit, target: String = "", openTrustedHttps: () -> Unit = {}, change: (RelaySettings) -> Unit) {
    val context = LocalContext.current
    val locale = LocalConfiguration.current.locales[0]
    val models = view.models
    val detected = view.detected
    val savedCanvas = view.savedCanvas
    val version = view.version
    SettingsToggleGroup(stringResource(R.string.car_view_areas), draft.viewAreas, R.drawable.symbol_monitor, IconHue.CAR,
        footer = stringResource(R.string.car_view_areas_hint)) { change(draft.copy(viewAreas = it)) }
    val none = stringResource(R.string.car_none)
    val other = stringResource(R.string.car_other)
    val car = draft.car
    SettingsGroup("car", R.drawable.symbol_car, IconHue.CAR, stringResource(R.string.car_and_screen), carSelectionSummary(car, models, locale, none, other), initiallyExpanded = false,
        targeted = target == "car", error = !car.complete) {
        CarPicker(car, models, locale) { change(draft.copy(car = it)) }
    }
    SettingsGroup("canvas", R.drawable.symbol_monitor, IconHue.CAR, stringResource(R.string.car_saved_canvas), savedCanvas?.let { "${it.profile.width} × ${it.profile.height}" } ?: none) {
        savedCanvas?.let { saved ->
            Text(stringResource(R.string.car_canvas_size, saved.profile.width, saved.profile.height, saved.profile.density))
            Hint(stringResource(when (saved.source) {
                "remembered" -> R.string.car_canvas_last
                "built-in model" -> R.string.car_canvas_model
                else -> R.string.car_canvas_default
            }))
        }
        OutlinedButton(onClick = clearHeadUnit, modifier = Modifier.heightIn(min = 48.dp)) {
            Text(stringResource(R.string.car_clear_screen))
        }
    }
    SettingsGroup("detected", R.drawable.symbol_monitor, IconHue.CAR, stringResource(R.string.car_detected), detected?.let { "${it.screen.width} × ${it.screen.height}" } ?: stringResource(R.string.car_no_screen)) {
        if (models?.year(car)?.screens?.isNotEmpty() == true) Hint(stringResource(R.string.car_built_in))
        val data = detected
        if (data == null) Text(stringResource(R.string.car_no_screen)) else {
            Text(stringResource(R.string.car_screen_size, data.screen.width, data.screen.height,
                String.format(locale, "%.2f", data.screen.densityDpi / 160.0)))
            for (full in listOf(true, false)) {
                SmallHeading(stringResource(if (full) R.string.view_areas_fullscreen else R.string.view_areas_window))
                val rows = CarScreenShare.sizes(data.records, full)
                if (rows.isEmpty()) Text(stringResource(R.string.car_no_sizes))
                rows.forEach { Text(stringResource(R.string.car_window_size, it.width, it.height, it.count)) }
            }
        }
    }
    val data = detected
    SettingsGroup("share", R.drawable.symbol_copy, IconHue.CAR, stringResource(R.string.car_share_title)) {
        Hint(stringResource(R.string.car_next_start))
        Hint(stringResource(R.string.car_share_hint))
        val title = stringResource(R.string.car_share_title)
        val cleaned = car.normalized()
        val brandName = if (cleaned.brand == "other") cleaned.brandText.ifEmpty { other } else models?.brand(cleaned)?.name(locale) ?: none
        val modelName = if (cleaned.model == "other") cleaned.modelText.ifEmpty { other } else models?.model(cleaned)?.name(locale) ?: none
        val yearName = if (cleaned.year == "other") cleaned.yearText.ifEmpty { other } else models?.year(cleaned)?.name(locale) ?: none
        val carLine = if (listOf(cleaned.brand, cleaned.model, cleaned.year).all { it.isNotEmpty() })
            stringResource(R.string.car_share_identity, brandName, modelName, yearName)
        else carSelectionSummary(cleaned, models, locale, none, other)
        val copied = stringResource(R.string.car_copied)
        OutlinedButton(enabled = data != null && version.isNotEmpty(), onClick = {
            if (data != null) {
                val text = CarScreenShare.text(title, carLine, version, cleaned, data.screen, data.records, data.receiver?.let { org.json.JSONObject(it) })
                context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(title, text))
                Toast.makeText(context, copied, Toast.LENGTH_SHORT).show()
            }
        }, modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.car_copy)) }
    }
}
