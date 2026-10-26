// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.CarModels
import org.openxplay.direlay.settings.CarSelection
import org.openxplay.direlay.ui.design.*
import java.util.Locale

internal data class CarPickerOption(val id: String, val label: String)
internal data class CarPickerOptions(val brands: List<CarPickerOption>, val models: List<CarPickerOption>, val years: List<CarPickerOption>)

internal fun carPickerOptions(car: CarSelection, models: CarModels?, locale: Locale, notSure: String, other: String): CarPickerOptions {
    val custom = CarPickerOption("other", other)
    return CarPickerOptions(models?.brands.orEmpty().map { CarPickerOption(it.id, it.name(locale)) } + custom,
        models?.brand(car)?.models.orEmpty().map { CarPickerOption(it.id, it.name(locale)) } + custom,
        listOf(CarPickerOption("", notSure)) + models?.model(car)?.years.orEmpty().map { CarPickerOption(it.id, it.name(locale)) } + custom)
}

internal fun carSelectionErrors(car: CarSelection): Map<String, Int> = buildMap {
    if (car.brand.isEmpty() || car.brand == "other" && car.brandText.isBlank()) put("car.brand", R.string.car_brand_required)
    if (car.model.isEmpty() || car.model == "other" && car.modelText.isBlank()) put("car.model", R.string.car_model_required)
}

internal fun carPickerLevelLabel(id: String, text: String, options: List<CarPickerOption>): String? = when {
    id.isEmpty() -> null
    id == "other" -> text.trim().takeIf { it.isNotEmpty() }
    else -> options.firstOrNull { it.id == id }?.label ?: id
}

@Composable fun CarPicker(car: CarSelection, models: CarModels?, locale: Locale, change: (CarSelection) -> Unit) {
    val choices = carPickerOptions(car, models, locale, stringResource(R.string.car_not_sure), stringResource(R.string.car_other))
    val errors = carSelectionErrors(car)
    val brand = carPickerLevelLabel(car.brand, car.brandText, choices.brands)
    val model = carPickerLevelLabel(car.model, car.modelText, choices.models)
    var brandExpanded by rememberSaveable(car.brand) { mutableStateOf(brand == null) }
    var modelExpanded by rememberSaveable(car.brand, car.model) { mutableStateOf(model == null) }
    val motion = LocalRelayMotion.current
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SetupEntrance(0) {
            CarPickerLevel(stringResource(R.string.car_brand), brand, brandExpanded, { brandExpanded = it }) {
                if (car.brand.isEmpty()) Hint(stringResource(R.string.car_brand_required))
                Options(choices.brands.map { it.label }, choices.brands.indexOfFirst { it.id == car.brand }) {
                    val id = choices.brands[it].id
                    brandExpanded = id == "other"
                    if (id != car.brand) change(CarSelection(brand = id))
                }
                if (car.brand == "other") CarPickerText(stringResource(R.string.car_brand), car.brandText, errors["car.brand"],
                    { brandExpanded = false }) {
                    change(car.copy(brandText = CarSelection.cleanText(it)))
                }
            }
        }
        AnimatedVisibility(brand != null, enter = relayDisclosureEnter(motion), exit = relayDisclosureExit(motion)) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SetupEntrance(1) {
                    CarPickerLevel(stringResource(R.string.car_model), model, modelExpanded, { modelExpanded = it }) {
                        if (car.model.isEmpty() && car.brand != "other") Hint(stringResource(R.string.car_model_required))
                        if (car.brand != "other") Options(choices.models.map { it.label }, choices.models.indexOfFirst { it.id == car.model }) {
                            val id = choices.models[it].id
                            modelExpanded = id == "other"
                            if (id != car.model) change(car.copy(model = id, modelText = "", year = "", yearText = ""))
                        }
                        if (car.brand == "other" || car.model == "other")
                            CarPickerText(stringResource(R.string.car_model), car.modelText, errors["car.model"],
                                { modelExpanded = false }) {
                                change(car.copy(model = "other", modelText = CarSelection.cleanText(it)))
                            }
                    }
                }
                AnimatedVisibility(model != null, enter = relayDisclosureEnter(motion), exit = relayDisclosureExit(motion)) {
                    SetupEntrance(2) {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            SmallHeading(stringResource(R.string.car_year))
                            if (car.brand != "other" && car.model != "other")
                                Options(choices.years.map { it.label }, choices.years.indexOfFirst { it.id == car.year }) {
                                    change(car.copy(year = choices.years[it].id, yearText = ""))
                                }
                            if (car.brand == "other" || car.model == "other" || car.year == "other")
                                SettingField(stringResource(R.string.car_year), car.yearText) {
                                    change(car.copy(year = "other", yearText = CarSelection.cleanText(it)))
                                }
                        }
                    }
                }
            }
        }
    }
}

@Composable private fun CarPickerLevel(title: String, label: String?, expanded: Boolean, expand: (Boolean) -> Unit,
    content: @Composable ColumnScope.() -> Unit) {
    val motion = LocalRelayMotion.current
    val folded = label != null && !expanded
    val changeLabel = stringResource(R.string.setup_change)
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        AnimatedVisibility(folded, enter = relayDisclosureEnter(motion), exit = relayDisclosureExit(motion)) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                FilterChip(true, { expand(true) }, label = { Text(label.orEmpty()) },
                    modifier = Modifier.weight(1f, fill = false).heightIn(min = 48.dp)
                        .semantics { contentDescription = "$title, ${label.orEmpty()}" }, shape = RelayTokens.chip)
                TextButton(onClick = { expand(true) }, Modifier.heightIn(min = 48.dp)
                    .semantics { contentDescription = "$changeLabel, $title" }) {
                    Text(changeLabel)
                }
            }
        }
        AnimatedVisibility(!folded, enter = relayDisclosureEnter(motion), exit = relayDisclosureExit(motion)) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SmallHeading(title)
                content()
            }
        }
    }
}

@Composable private fun CarPickerText(title: String, value: String, error: Int?, fold: () -> Unit, change: (String) -> Unit) {
    var hadFocus by remember { mutableStateOf(false) }
    Box(Modifier.onFocusChanged {
        if (hadFocus && !it.hasFocus && value.isNotBlank()) fold()
        hadFocus = it.hasFocus
    }.focusGroup()) {
        SettingField(title, value, error, change = change)
    }
}
