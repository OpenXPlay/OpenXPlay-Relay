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
import kotlinx.coroutines.*
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.*

private data class DetectedCarScreen(val screen: ScreenDataKey, val records: List<WindowSizeRecord>, val receiver: org.json.JSONObject?)
@Composable fun CarScreenSettings(draft: RelaySettings, runtime: RelayRuntime, change: (RelaySettings) -> Unit) {
    val context = LocalContext.current
    val locale = LocalConfiguration.current.locales[0]
    var models by remember { mutableStateOf<CarModels?>(null) }
    var detected by remember { mutableStateOf<DetectedCarScreen?>(null) }
    var version by remember { mutableStateOf("") }
    LaunchedEffect(runtime) {
        val loaded = withContext(Dispatchers.IO) {
            runtime.carModels to context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty()
        }
        models = loaded.first; version = loaded.second
        while (true) {
            detected = withContext(Dispatchers.IO) {
                val rows = runtime.windowRecords.snapshot()
                rows.firstOrNull()?.let { latest ->
                    val screen = ScreenDataKey(latest.screenWidth, latest.screenHeight, latest.densityDpi)
                    DetectedCarScreen(screen, rows.filter(screen::matches), runtime.status.receivers.latestInfo(screen))
                }
            }
            delay(500)
        }
    }
    SettingsRow(stringResource(R.string.car_view_areas), stringResource(R.string.car_view_areas_hint), R.drawable.symbol_car,
        trailing = { LabeledSwitch(stringResource(R.string.car_view_areas), draft.viewAreas, { change(draft.copy(viewAreas = it)) }) })
    val none = stringResource(R.string.car_none)
    val other = stringResource(R.string.car_other)
    val brands = models?.brands.orEmpty()
    val car = draft.car
    SmallHeading(stringResource(R.string.car_brand))
    val brandIds = listOf("") + brands.map { it.id } + "other"
    Options(listOf(none) + brands.map { it.name(locale) } + other, brandIds.indexOf(car.brand)) {
        change(draft.copy(car = CarSelection(brand = brandIds[it], model = if (brandIds[it] == "other") "other" else "")))
    }
    if (car.brand == "other") SettingField(stringResource(R.string.car_brand), car.brandText) {
        change(draft.copy(car = car.copy(brandText = CarSelection.cleanText(it))))
    }
    if (car.brand.isNotEmpty()) {
        SmallHeading(stringResource(R.string.car_model))
        if (car.brand != "other") {
            val choices = models?.brand(car)?.models.orEmpty()
            val ids = listOf("") + choices.map { it.id } + "other"
            Options(listOf(none) + choices.map { it.name(locale) } + other, ids.indexOf(car.model)) {
                change(draft.copy(car = car.copy(model = ids[it], modelText = "")))
            }
        }
        if (car.brand == "other" || car.model == "other") SettingField(stringResource(R.string.car_model), car.modelText) {
            change(draft.copy(car = car.copy(modelText = CarSelection.cleanText(it))))
        }
    }
    SmallHeading(stringResource(R.string.car_detected))
    if (models?.model(car)?.screens?.isNotEmpty() == true) Hint(stringResource(R.string.car_built_in))
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
    Hint(stringResource(R.string.car_next_start))
    Hint(stringResource(R.string.car_share_hint))
    val title = stringResource(R.string.car_share_title)
    val cleaned = car.normalized()
    val brandName = if (cleaned.brand == "other") cleaned.brandText.ifEmpty { other } else models?.brand(cleaned)?.name(locale) ?: none
    val modelName = if (cleaned.model == "other") cleaned.modelText.ifEmpty { other } else models?.model(cleaned)?.name(locale) ?: none
    val carLine = stringResource(R.string.car_share_identity, brandName, modelName)
    val copied = stringResource(R.string.car_copied)
    OutlinedButton(enabled = data != null && version.isNotEmpty(), onClick = {
        if (data != null) {
            val text = CarScreenShare.text(title, carLine, version, cleaned, data.screen, data.records, data.receiver)
            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(title, text))
            Toast.makeText(context, copied, Toast.LENGTH_SHORT).show()
        }
    }, modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.car_copy)) }
}
