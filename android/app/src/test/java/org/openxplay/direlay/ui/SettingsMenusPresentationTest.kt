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
import org.openxplay.direlay.settings.TeslaBrowserMode
import org.openxplay.direlay.storage.MaterialStatus
import org.openxplay.direlay.ui.design.IconHue
import java.util.Locale

class SettingsMenusPresentationTest {
    @Test fun teslaIsAlwaysInTheFirstMenuGroupDirectlyAfterCarAndScreen() {
        assertEquals(listOf("settings/output", "settings/car", "settings/tesla", "settings/picture", "settings/audio"),
            settingsMainPages.map { it.route })
        val tesla = settingsMainPages.single { it.route == "settings/tesla" }
        assertEquals(R.string.tesla_title, tesla.title)
        assertEquals(R.string.tesla_title, settingsPageTitle(tesla.route))
        assertEquals(R.drawable.symbol_car, tesla.icon)
        assertEquals(IconHue.CAR, settingsMenuHue(tesla.title))
    }

    @Test fun teslaMenuSummaryUsesTheSelectedModesTitleIncludingOff() {
        val titles = mapOf(TeslaBrowserMode.OFF to R.string.tesla_off, TeslaBrowserMode.TESLAMIRROR to R.string.tesla_mirror,
            TeslaBrowserMode.VIRTUAL_NAME to R.string.tesla_virtual_name, TeslaBrowserMode.PUBLIC_ADDRESS to R.string.tesla_public_address,
            TeslaBrowserMode.TESLA_ANDROID to R.string.tesla_android)
        TeslaBrowserMode.entries.forEach { mode -> assertEquals(titles.getValue(mode), teslaBrowserOption(mode).title) }
    }

    @Test fun frameRateAndPictureSummariesShowTheSuppliedSessionOrNextStartRate() {
        val auto = org.openxplay.stream.core.FrameRatePreference.AUTO
        assertEquals("Auto · 60 fps", frameRateSummary(auto, 60, "Auto"))
        assertEquals("自动 · 30 fps", frameRateSummary(auto, 30, "自动"))
        assertEquals("45 fps", frameRateSummary(org.openxplay.stream.core.FrameRatePreference.FPS_45, 45, "Auto"))
        assertEquals("Automatic · 200 mm · H.265 · 60 fps", pictureMenuSummary("Automatic", 200, "H.265", 60))
    }

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
