// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.CarModels
import org.openxplay.direlay.settings.CarSelection
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

@Composable fun CarPicker(car: CarSelection, models: CarModels?, locale: Locale, change: (CarSelection) -> Unit) {
    val choices = carPickerOptions(car, models, locale, stringResource(R.string.car_not_sure), stringResource(R.string.car_other))
    val errors = carSelectionErrors(car)
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SmallHeading(stringResource(R.string.car_brand))
        if (car.brand.isEmpty()) Hint(stringResource(R.string.car_brand_required))
        Options(choices.brands.map { it.label }, choices.brands.indexOfFirst { it.id == car.brand }) {
            change(CarSelection(brand = choices.brands[it].id))
        }
        if (car.brand == "other") SettingField(stringResource(R.string.car_brand), car.brandText, errors["car.brand"]) {
            change(car.copy(brandText = CarSelection.cleanText(it)))
        }
        if (car.brand.isNotEmpty()) {
            SmallHeading(stringResource(R.string.car_model))
            if (car.model.isEmpty() && car.brand != "other") Hint(stringResource(R.string.car_model_required))
            if (car.brand != "other") Options(choices.models.map { it.label }, choices.models.indexOfFirst { it.id == car.model }) {
                change(car.copy(model = choices.models[it].id, modelText = "", year = "", yearText = ""))
            }
            if (car.brand == "other" || car.model == "other")
                SettingField(stringResource(R.string.car_model), car.modelText, errors["car.model"]) {
                    change(car.copy(model = "other", modelText = CarSelection.cleanText(it)))
                }
            if (car.model.isNotEmpty()) {
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
