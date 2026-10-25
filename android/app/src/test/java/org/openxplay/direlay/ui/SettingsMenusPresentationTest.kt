// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.CarBrand
import org.openxplay.direlay.relay.CarModel
import org.openxplay.direlay.relay.CarYear
import org.openxplay.direlay.relay.CarModels
import org.openxplay.direlay.settings.CarSelection
import org.openxplay.direlay.storage.MaterialStatus
import java.util.Locale

class SettingsMenusPresentationTest {
    private val models = CarModels(listOf(CarBrand("xpeng", mapOf("en" to "XPeng", "zh-CN" to "小鹏"),
        listOf(CarModel("g6", mapOf("en" to "G6", "zh-CN" to "G6"),
            listOf(CarYear("g6-2023", mapOf("en" to "2023", "zh-CN" to "2023"), emptyList())))))))

    @Test fun identityPageUsesItsMenuTitleAndStatusLabels() {
        assertEquals(R.string.menu_identity, settingsPageTitle("settings/identity"))
        assertEquals(R.string.direlay_installed, identityStatusKey(MaterialStatus.Installed))
        assertEquals(R.string.direlay_missing, identityStatusKey(MaterialStatus.Missing))
        assertEquals(R.string.dev_invalid_identity, identityStatusKey(MaterialStatus.Invalid("Invalid files")))
    }

    @Test fun summariesResolveCatalogIdsInTheCurrentLocale() {
        val car = CarSelection("xpeng", model = "g6", year = "g6-2023")
        assertEquals("XPeng · G6 · 2023", carSelectionSummary(car, models, Locale.ENGLISH, "None", "Other"))
        assertEquals("小鹏 · G6 · 2023", carSelectionSummary(car, models, Locale.SIMPLIFIED_CHINESE, "无", "其他"))
    }

    @Test fun incompleteSelectionsLeaveEmptyPartsOut() {
        assertEquals("XPeng", carSelectionSummary(CarSelection("xpeng"), models, Locale.ENGLISH, "None", "Other"))
        assertEquals("XPeng · G6", carSelectionSummary(CarSelection("xpeng", model = "g6"), models, Locale.ENGLISH, "None", "Other"))
        assertEquals("XPeng · G6 · Custom", carSelectionSummary(CarSelection("xpeng", model = "g6", year = "other", yearText = "Custom"),
            models, Locale.ENGLISH, "None", "Other"))
    }

    @Test fun customAndUnavailableModelsNeverExposeRawCatalogIds() {
        assertEquals("My brand · My model · My year", carSelectionSummary(CarSelection("other", "My brand", "other", "My model", "other", "My year"),
            models, Locale.ENGLISH, "None", "Other"))
        assertEquals("其他 · 其他 · 其他", carSelectionSummary(CarSelection("other", model = "other", year = "other"),
            models, Locale.SIMPLIFIED_CHINESE, "无", "其他"))
        assertEquals("None", carSelectionSummary(CarSelection("xpeng", model = "g6", year = "g6-2023"), null, Locale.ENGLISH, "None", "Other"))
        assertEquals("None", carSelectionSummary(CarSelection(), models, Locale.ENGLISH, "None", "Other"))
    }
}
