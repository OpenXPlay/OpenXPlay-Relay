// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.os.Build
import android.widget.Toast
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.selection.selectableGroup
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
import org.openxplay.direlay.publicname.hasTrustedNameConfiguration

private data class DetectedCarScreen(val screen: ScreenDataKey, val records: List<WindowSizeRecord>, val receiver: org.json.JSONObject?)
@Composable fun CarScreenSettings(draft: RelaySettings, runtime: RelayRuntime,
    openTrustedHttps: () -> Unit = {}, openOutput: () -> Unit = {}, change: (RelaySettings) -> Unit) {
    val context = LocalContext.current
    val locale = LocalConfiguration.current.locales[0]
    var models by remember { mutableStateOf<CarModels?>(null) }
    var detected by remember { mutableStateOf<DetectedCarScreen?>(null) }
    var savedCanvas by remember { mutableStateOf<CarPlayScreen?>(null) }
    var version by remember { mutableStateOf("") }
    LaunchedEffect(runtime) {
        val loaded = withContext(Dispatchers.IO) {
            runtime.carModels to context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty()
        }
        models = loaded.first; version = loaded.second
        while (true) {
            savedCanvas = withContext(Dispatchers.IO) { runtime.savedCanvas }
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
    CollapsibleCard("viewAreas", stringResource(R.string.car_view_areas), stringResource(R.string.car_view_areas_hint), initiallyExpanded = true) {
        SettingsRow(stringResource(R.string.car_view_areas), stringResource(R.string.car_view_areas_hint), R.drawable.symbol_car,
            trailing = { LabeledSwitch(stringResource(R.string.car_view_areas), draft.viewAreas, { change(draft.copy(viewAreas = it)) }) })
    }
    val none = stringResource(R.string.car_none)
    val other = stringResource(R.string.car_other)
    val brands = models?.brands.orEmpty()
    val car = draft.car
    CollapsibleCard("car", stringResource(R.string.car_and_screen), carSelectionSummary(car, models, locale, none, other), initiallyExpanded = false) {
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
    }
    if (car.brand == "tesla" || draft.teslaBrowser != TeslaBrowserMode.OFF)
        TeslaBrowserSettings(draft, runtime, openTrustedHttps, openOutput, change)
    CollapsibleCard("canvas", stringResource(R.string.car_saved_canvas), savedCanvas?.let { "${it.profile.width} × ${it.profile.height}" } ?: none) {
        savedCanvas?.let { saved ->
            Text(stringResource(R.string.car_canvas_size, saved.profile.width, saved.profile.height, saved.profile.density))
            Hint(stringResource(when (saved.source) {
                "remembered" -> R.string.car_canvas_last
                "built-in model" -> R.string.car_canvas_model
                else -> R.string.car_canvas_default
            }))
        }
        OutlinedButton(onClick = { runtime.clearSavedHeadUnit() }, modifier = Modifier.heightIn(min = 48.dp)) {
            Text(stringResource(R.string.car_clear_screen))
        }
    }
    CollapsibleCard("detected", stringResource(R.string.car_detected), detected?.let { "${it.screen.width} × ${it.screen.height}" } ?: stringResource(R.string.car_no_screen)) {
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
    }
    val data = detected
    CollapsibleCard("share", stringResource(R.string.car_share_title), stringResource(R.string.car_share_hint)) {
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
}

@Composable private fun TeslaBrowserSettings(draft: RelaySettings, runtime: RelayRuntime,
    openTrustedHttps: () -> Unit, openOutput: () -> Unit, change: (RelaySettings) -> Unit) {
    val name by runtime.publicName.view.collectAsState()
    val options = listOf(RadioOption(R.string.tesla_off, R.string.tesla_intro),
        RadioOption(R.string.tesla_virtual_name, R.string.tesla_virtual_name_hint),
        RadioOption(R.string.tesla_public_address, R.string.tesla_public_address_hint))
    CollapsibleCard("tesla", stringResource(R.string.tesla_title), stringResource(options[draft.teslaBrowser.ordinal].title),
        initiallyExpanded = true) {
        Hint(stringResource(R.string.tesla_intro))
        Column(Modifier.selectableGroup()) {
            options.forEachIndexed { index, option ->
                RadioOptionRow(option, draft.teslaBrowser.ordinal == index,
                    supportingText = when (index) {
                        0 -> ""
                        2 -> stringResource(R.string.tesla_public_address_hint, draft.teslaPublicAddress)
                        else -> null
                    }) { change(draft.copy(teslaBrowser = TeslaBrowserMode.entries[index])) }
            }
        }
        val address = draft.teslaAddress()
        if (address != null) {
            CollapsibleCard("teslaAddress", stringResource(R.string.tesla_address), address) {
                val phoneAddresses = runtime.ipv4 - listOfNotNull(runtime.teslaAddress?.address).toSet()
                SettingField(stringResource(R.string.tesla_address), address,
                    R.string.tesla_address_invalid.takeUnless { TeslaAddressPolicy.valid(address, phoneAddresses) }) {
                    change(if (draft.teslaBrowser == TeslaBrowserMode.VIRTUAL_NAME) draft.copy(teslaVirtualAddress = it)
                        else draft.copy(teslaPublicAddress = it))
                }
                if (TeslaAddressPolicy.needsAndroid13Warning(Build.VERSION.SDK_INT, address))
                    Hint(stringResource(R.string.tesla_address_android13))
            }
            Hint(stringResource(R.string.tesla_requirements))
            if (draft.carlinkOutput && !draft.bothOutputs) {
                Hint(stringResource(R.string.tesla_needs_browser))
                TextButton(onClick = openOutput, modifier = Modifier.heightIn(min = 48.dp)) {
                    Text(stringResource(R.string.tesla_open_output))
                }
            }
            if (!runtime.settingsStore.wireless(draft)) Hint(stringResource(R.string.tesla_needs_wireless))
            if (draft.wirelessNetwork.choice == WirelessNetworkChoice.EXISTING_WIFI)
                Hint(stringResource(R.string.tesla_needs_phone_network))
            if (draft.teslaBrowser == TeslaBrowserMode.VIRTUAL_NAME && !name.hasTrustedNameConfiguration()) {
                Hint(stringResource(R.string.tesla_needs_name))
                TextButton(onClick = openTrustedHttps, modifier = Modifier.heightIn(min = 48.dp)) {
                    Text(stringResource(R.string.tesla_open_name))
                }
            }
        }
    }
}
