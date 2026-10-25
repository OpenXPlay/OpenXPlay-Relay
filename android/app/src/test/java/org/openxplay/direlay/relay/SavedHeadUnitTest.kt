// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.relay

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class SavedHeadUnitTest {
    private val context get() = RuntimeEnvironment.getApplication()
    private val preferences get() = context.getSharedPreferences("direlay-car-display", 0)
    private val screen = ScreenDataKey(2401, 1201, 280)
    private val report = HeadUnitDisplay(screen, ViewAreaSize(1533, 907), false, true)
    private val canvas = DisplayProfile(1920, 960, 240)
    private val saved get() = SavedHeadUnit(canvas, report, CarPlayResolution.AUTO, true, 36864)
    private fun settings() = RelaySettingsStore(context).load()
    private fun year() = checkNotNull(CarModels.parse(context.assets.open("car-models.json").bufferedReader().use { it.readText() })
        .year(CarSelection("xpeng", model = "g6", year = "g6-2023")))
    private fun store() = SavedHeadUnitStore(preferences)

    @Test fun screenWidthPresetsKeepStartupCanvasesAndUseTheirActualPixelWidth() {
        val settings = settings().copy(carplayResolution = CarPlayResolution.NATIVE, viewAreas = false, displayMode = DisplayMode.AUTOMATIC)
        val native = DisplayProfile(2400, 1200, 300)
        for ((preset, declared) in CarPlayScreenWidth.entries.zip(listOf(175, 250, 325, 375))) {
            val next = settings.copy(carplayScreenWidth = preset)
            for (selected in listOf(startupCarPlayScreen(next, native, saved, year()),
                startupCarPlayScreen(next, null, saved, null), startupCarPlayScreen(next, null, null, year()),
                startupCarPlayScreen(next.copy(displayMode = DisplayMode.FIXED, display = native), null, saved, year()))) {
                val profile = checkNotNull(selected).profile
                assertEquals(native, profile)
                assertEquals(declared, physicalWidth(profile, preset))
            }
        }
    }
    @Test fun nativeReFitsSavedScreenKeepsDecoderBudgetAndPlansInCanvasPixels() {
        val settings = settings().copy(carplayResolution = CarPlayResolution.NATIVE, viewAreas = false)
        val native = DisplayProfile(2400, 1200, 300)
        for (previous in listOf(CarPlayResolution.AUTO, null)) {
            assertEquals(CarPlayScreen(native, "remembered"), startupCarPlayScreen(settings, null, saved.copy(resolution = previous), null))
        }
        val value = saved.copy(canvas = native, resolution = CarPlayResolution.NATIVE, wholeScreen = false)
        store().save(value); assertEquals(value, store().load())
        assertEquals(CarPlayScreen(native, "remembered"), startupCarPlayScreen(settings, null, store().load(), null))
        assertEquals(canvas, startupCarPlayScreen(settings.copy(carplayResolution = CarPlayResolution.AUTO, viewAreas = true), null, value, null)?.profile)
        assertEquals(native, startupCarPlayScreen(settings, null, null, year())?.profile)
        val limited = checkNotNull(startupCarPlayScreen(settings, null, saved.copy(maxMacroblocks = 3600), null)).profile
        assertEquals(fitDisplay(report.display(), CarPlayResolution.NATIVE, 3600), limited)
        val areas = planHeadUnitAreas(native.width, native.height, report, emptyList(), year())
        assertEquals(ViewAreaSize(2400, 1200), areas[0])
        assertTrue(areas.size > 2)
        assertTrue(areas.all { it.width <= native.width && it.height <= native.height })
    }

    @Test fun persistsCanvasReportNullableFlagsAndDecoderBudgetTogether() {
        for (full in listOf(null, false, true)) for (preferred in listOf(null, false, true)) {
            val original = saved.copy(display = report.copy(fullscreen = full, preferred = preferred))
            store().save(original)
            val loaded = checkNotNull(store().load())
            assertEquals(original, loaded)
            assertEquals(original.display, HeadUnitDisplay.from(checkNotNull(loaded.display).display()))
        }
        val noViewport = saved.copy(display = report.copy(visible = null))
        store().save(noViewport); assertEquals(noViewport, store().load())
    }
    @Test fun acceptsLegacyCanvasAndRejectsCorruptReportWithoutLosingCanvas() {
        preferences.edit().clear().putInt("width", 800).putInt("height", 480).putInt("density", 160).apply()
        assertEquals(SavedHeadUnit(DisplayProfile(800, 480, 160), null), store().load())
        store().save(saved)
        for (key in listOf("screenWidth", "screenHeight", "screenDensity", "visibleWidth", "visibleHeight")) {
            store().save(saved); preferences.edit().putInt(key, 0).apply()
            assertEquals(canvas, store().load()?.canvas); assertNull(store().load()?.display)
        }
        preferences.edit().putInt("width", 0).apply(); assertNull(store().load())
    }
    @Test fun savedDisplayPlansBeforeAnyPageConnectsUsingOnlyItsRecords() {
        store().save(saved)
        val loaded = store().load()
        val display = planningDisplay(null, loaded, null)
        assertEquals(report, display)
        val local = listOf(WindowSizeRecord(2401, 1201, 280, 1533, 1014, true, 9, 1),
            WindowSizeRecord(2401, 1201, 280, 1533, 907, false, 5, 2),
            WindowSizeRecord(1920, 1080, 160, 800, 600, false, 99, 3))
        val areas = planHeadUnitAreas(1920, 960, display, local, null)
        assertEquals(3, areas.size)
        assertEquals(setOf(ViewAreaLibrary.FULLSCREEN), areas[1].libraries)
        assertEquals(setOf(ViewAreaLibrary.WINDOWED), areas[2].libraries)
        assertEquals(1533.0 / 907, areas[2].width.toDouble() / areas[2].height, .005)
        assertEquals(CarPlayScreen(canvas, "remembered"), startupCarPlayScreen(settings(), null, loaded, null))
        val currentOnly = planHeadUnitAreas(1920, 960, display, emptyList(), null)
        assertEquals(2, currentOnly.size)
        assertEquals(setOf(ViewAreaLibrary.WINDOWED), currentOnly[1].libraries)
        assertNull(ViewAreaPlanner.match(areas, 1533, 907, display?.fullscreen, display?.preferred))
    }
    @Test fun modelFallbackSuppliesScreenAndBothLibrariesWithResolutionCaps() {
        val model = year()
        val display = planningDisplay(null, null, model)
        assertEquals(screen, display?.screen)
        assertEquals(true, display?.preferred)
        for (resolution in CarPlayResolution.entries) {
            val settings = settings().copy(carplayResolution = resolution)
            val selected = checkNotNull(startupCarPlayScreen(settings, null, null, model))
            assertEquals("built-in model", selected.source)
            assertEquals(fitDisplay(checkNotNull(display).display(), resolution, wholeScreen = true), selected.profile)
            val areas = planHeadUnitAreas(selected.profile.width, selected.profile.height, display, emptyList(), model)
            assertTrue(areas.size > 1)
            assertTrue(areas.any { ViewAreaLibrary.FULLSCREEN in it.libraries })
            assertTrue(areas.any { ViewAreaLibrary.WINDOWED in it.libraries })
        }
        // A legacy canvas has no screen identity and must not override a selected model.
        assertEquals("built-in model", startupCarPlayScreen(settings(), null, SavedHeadUnit(CarDisplayFit.default, null), model)?.source)
        assertEquals(report, planningDisplay(null, saved, model))
        assertEquals(report.copy(screen = screen.copy(width = 2400)), planningDisplay(report.copy(screen = screen.copy(width = 2400)), saved, model))
    }
    @Test fun savedPlanningMergesBuiltInOnlyForExactScreenAndReFitsChangedSettingsWithinDecoderBudget() {
        val areas = planHeadUnitAreas(1920, 960, report, emptyList(), year())
        assertTrue(areas.size > 2)
        assertEquals(2, planHeadUnitAreas(1920, 960, report.copy(screen = screen.copy(densityDpi = 281)), emptyList(), year()).size)
        val limited = saved.copy(maxMacroblocks = 3600)
        val settings = settings().copy(carplayResolution = CarPlayResolution.P720)
        val selected = checkNotNull(startupCarPlayScreen(settings, null, limited, year()))
        assertEquals(fitDisplay(report.display(), CarPlayResolution.P720, 3600, true), selected.profile)
        assertTrue(CarDisplayFit.macroblocks(selected.profile.width, selected.profile.height) <= 3600)
        val live = DisplayProfile(800, 480, 160)
        assertEquals(CarPlayScreen(live, "car screen"), startupCarPlayScreen(settings, live, saved, year()))
        assertEquals(CarPlayScreen(live, "fixed"), startupCarPlayScreen(settings.copy(displayMode = DisplayMode.FIXED, display = live), null, saved, year()))
        assertNull(startupCarPlayScreen(settings.copy(displayMode = DisplayMode.RELAY_SCREEN), null, saved, year()))
    }
    @Test fun clearingRemovesBothScreenAndCanvasAndFallsBackToModelOrDefault() {
        store().save(saved); store().clear()
        assertTrue(preferences.all.isEmpty()); assertNull(store().load())
        assertNull(planningDisplay(null, store().load(), null))
        assertEquals(CarPlayScreen(CarDisplayFit.default, "default"), startupCarPlayScreen(settings(), null, store().load(), null))
        assertEquals("built-in model", startupCarPlayScreen(settings(), null, store().load(), year())?.source)
        store().save(saved); assertEquals(saved, store().load())
    }
    @Test fun legacyCarRecordsMatchMigratedSelectionsAndRewriteOnSave() {
        val legacy = CarSelection("xpeng", model = "g6-2023")
        val current = CarSelection("xpeng", model = "g6", year = "g6-2023")
        val value = saved.copy(car = legacy)
        assertEquals(value, savedHeadUnitForCar(value, current))
        assertEquals(saved.copy(car = current), savedHeadUnitForCar(saved.copy(car = current), legacy))
        store().save(saved)
        preferences.edit().putString("carBrand", "xpeng").putString("carModel", "g6-2023").remove("carYear").remove("carYearText").apply()
        val loaded = checkNotNull(store().load())
        assertEquals(current, loaded.car)
        assertEquals(loaded, savedHeadUnitForCar(loaded, current))
        assertNull(savedHeadUnitForCar(loaded, current.copy(year = "g6-2024")))
        assertNull(savedHeadUnitForCar(loaded, current.copy(year = "")))
        store().save(value)
        assertEquals("g6", preferences.getString("carModel", null))
        assertEquals("g6-2023", preferences.getString("carYear", null))
        val custom = CarSelection("other", "Brand", "other", "Model", "other", "Year")
        store().save(saved.copy(car = custom)); assertEquals(custom, store().load()?.car)
        assertEquals("Year", preferences.getString("carYearText", null))
        store().save(saved)
        assertFalse(preferences.contains("carYear")); assertFalse(preferences.contains("carYearText"))
    }
    @Test fun noSelectedYearNeverUsesBuiltInScreenData() {
        val catalog = CarModels.parse(context.assets.open("car-models.json").bufferedReader().use { it.readText() })
        val year = catalog.year(CarSelection("xpeng", model = "g6"))
        assertNull(year)
        assertNull(planningDisplay(null, null, year))
        assertEquals("default", startupCarPlayScreen(settings(), null, null, year)?.source)
        assertEquals(WirelessPictureSource.DEFAULT, WirelessPicturePolicy.resolve(settings(), null, year).source)
        assertEquals(1, planHeadUnitAreas(1920, 960, null, emptyList(), year).size)
    }
    @Test fun rememberedScreensBelongToTheirCarConfiguration() {
        val car = CarSelection("tesla", model = "model-y", year = "model-y-2025")
        val value = saved.copy(car = car)
        store().save(value)
        assertEquals(value, store().load())
        assertEquals(value, savedHeadUnitForCar(store().load(), car))
        assertNull(savedHeadUnitForCar(store().load(), CarSelection()))
        assertNull(savedHeadUnitForCar(null, car))
        assertNull(savedHeadUnitForCar(saved, car))
        assertEquals(saved, savedHeadUnitForCar(saved, CarSelection()))
    }
}
