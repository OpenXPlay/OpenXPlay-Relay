// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.CarBrand
import org.openxplay.direlay.relay.CarModel
import org.openxplay.direlay.relay.CarModels
import org.openxplay.direlay.relay.CarYear
import org.openxplay.direlay.settings.CarSelection
import java.util.Locale

class CarPickerTest {
    private val models = CarModels(listOf(CarBrand("xpeng", mapOf("en" to "XPeng", "zh-CN" to "小鹏"),
        listOf(CarModel("g6", mapOf("en" to "G6"),
            listOf(CarYear("g6-2023", mapOf("en" to "2023"), emptyList())))))))

    @Test fun requiredChoicesHaveNoNoneChipAndOnlyYearDefaultsToNotSure() {
        for (car in listOf(CarSelection(), CarSelection("xpeng"), CarSelection("xpeng", model = "g6"))) {
            val choices = carPickerOptions(car, models, Locale.ENGLISH, "Not sure", "Other")
            assertFalse(choices.brands.any { it.id.isEmpty() })
            assertFalse(choices.models.any { it.id.isEmpty() })
            assertEquals(CarPickerOption("", "Not sure"), choices.years.first())
            assertEquals("other", choices.brands.last().id)
            assertEquals("other", choices.models.last().id)
            assertEquals("other", choices.years.last().id)
            if (car.brand.isEmpty()) assertEquals(-1, choices.brands.indexOfFirst { it.id == car.brand })
            if (car.model.isEmpty()) assertEquals(-1, choices.models.indexOfFirst { it.id == car.model })
            assertEquals(0, choices.years.indexOfFirst { it.id == car.year })
        }
        val choices = carPickerOptions(CarSelection("xpeng", model = "g6"), models, Locale.SIMPLIFIED_CHINESE, "不确定", "其他")
        assertEquals(listOf("小鹏", "其他"), choices.brands.map { it.label })
        assertEquals(listOf("g6", "other"), choices.models.map { it.id })
        assertEquals(listOf("", "g6-2023", "other"), choices.years.map { it.id })
    }

    @Test fun otherRemainsAvailableWhileTheCatalogLoads() {
        val choices = carPickerOptions(CarSelection(), null, Locale.ENGLISH, "Not sure", "Other")
        assertEquals(listOf(CarPickerOption("other", "Other")), choices.brands)
        assertEquals(listOf(CarPickerOption("other", "Other")), choices.models)
        assertEquals(listOf("", "other"), choices.years.map { it.id })
    }

    @Test fun missingRequiredOtherTextHasAnErrorAndYearNeverDoes() {
        assertEquals(mapOf("car.brand" to R.string.car_brand_required, "car.model" to R.string.car_model_required),
            carSelectionErrors(CarSelection("other", " ", "other", " ")))
        assertEquals(mapOf("car.model" to R.string.car_model_required), carSelectionErrors(CarSelection("xpeng")))
        assertTrue(carSelectionErrors(CarSelection("xpeng", model = "g6", year = "other")).isEmpty())
        assertTrue(carSelectionErrors(CarSelection("other", "Brand", "other", "Model")).isEmpty())
    }

    @Test fun homeRecommendsOnlyIncompleteCarsAndItsDestinationIsTheCarPage() {
        for (car in listOf(CarSelection(), CarSelection("xpeng"), CarSelection("other", "Brand", "other"),
            CarSelection("xpeng", model = "other", modelText = " "))) assertTrue(homeCarRowVisible(car))
        for (car in listOf(CarSelection("xpeng", model = "g6"), CarSelection("other", "Brand", "other", "Model"),
            CarSelection("xpeng", model = "g6", year = "other"))) assertFalse(homeCarRowVisible(car))
        assertEquals("settings/car#car", relayRoute("settings:car"))
    }
}
