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

    @Test fun zeekrPickerLocalizesTheBrandAndLaunchYearWhileKeepingYearOptional() {
        val catalog = CarModels(models.brands + CarBrand("zeekr", mapOf("en" to "ZEEKR", "zh-CN" to "极氪"),
            listOf(CarModel("007-gt", mapOf("en" to "007 GT", "zh-CN" to "007 GT"),
                listOf(CarYear("007-gt-2025", mapOf("en" to "2025+", "zh-CN" to "2025 起"), emptyList()))))))
        val car = CarSelection("zeekr", model = "007-gt")
        val english = carPickerOptions(car, catalog, Locale.ENGLISH, "Not sure", "Other")
        val chinese = carPickerOptions(car, catalog, Locale.SIMPLIFIED_CHINESE, "不确定", "其他")
        assertEquals(listOf("xpeng", "zeekr", "other"), english.brands.map { it.id })
        assertEquals(CarPickerOption("zeekr", "ZEEKR"), english.brands[1])
        assertEquals(CarPickerOption("zeekr", "极氪"), chinese.brands[1])
        assertEquals(listOf(CarPickerOption("007-gt", "007 GT"), CarPickerOption("other", "Other")), english.models)
        assertEquals("007 GT", chinese.models.first().label)
        assertEquals(listOf(CarPickerOption("", "Not sure"), CarPickerOption("007-gt-2025", "2025+"),
            CarPickerOption("other", "Other")), english.years)
        assertEquals(listOf(CarPickerOption("", "不确定"), CarPickerOption("007-gt-2025", "2025 起"),
            CarPickerOption("other", "其他")), chinese.years)
        assertTrue(carSelectionErrors(car).isEmpty())
        assertFalse(homeCarRowVisible(car))
        assertEquals("ZEEKR · 007 GT", carSelectionSummary(car, catalog, Locale.ENGLISH, "None", "Other"))
        assertEquals("极氪 · 007 GT · 2025 起",
            carSelectionSummary(car.copy(year = "007-gt-2025"), catalog, Locale.SIMPLIFIED_CHINESE, "无", "其他"))
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
    @Test fun onlyChosenLevelsFoldAndCustomTextMustBePresentBeforeTheNextLevelAppears() {
        val options = listOf(CarPickerOption("xpeng", "XPeng"), CarPickerOption("g6", "G6"), CarPickerOption("other", "Other"))
        assertNull(carPickerLevelLabel("", "", options))
        assertNull(carPickerLevelLabel("other", "  ", options))
        assertEquals("XPeng", carPickerLevelLabel("xpeng", "", options))
        assertEquals("G6", carPickerLevelLabel("g6", "", options))
        assertEquals("My car", carPickerLevelLabel("other", " My car ", options))
        assertEquals("xpeng", carPickerLevelLabel("xpeng", "", emptyList()))
    }

}
