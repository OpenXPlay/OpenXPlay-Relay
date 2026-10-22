// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.relay

import android.app.Application
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.openxplay.direlay.settings.CarSelection
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class CarModelsTest {
    private fun models() = CarModels.parse(RuntimeEnvironment.getApplication().assets.open("car-models.json").bufferedReader().use { it.readText() })
    private val selected = CarSelection("xpeng", model = "g6-2023")
    private val screen = ScreenDataKey(2401, 1201, 280)
    @Test fun teslaModelsHaveBothNamesAndNoUnmeasuredBrowserScreens() {
        val catalog = models()
        val brand = requireNotNull(catalog.brand(CarSelection("tesla")))
        assertEquals("tesla", catalog.brands.last().id)
        assertEquals("Tesla", brand.name(Locale.ENGLISH))
        assertEquals("特斯拉", brand.name(Locale.SIMPLIFIED_CHINESE))
        val expected = linkedMapOf(
            "model-3-2017" to ("Model 3 (2017–2023)" to "Model 3（2017–2023 款）"),
            "model-3-2024" to ("Model 3 (2024 and later)" to "Model 3 焕新版（2024 款起）"),
            "model-y-2020" to ("Model Y (2020–2024)" to "Model Y（2020–2024 款）"),
            "model-y-2025" to ("Model Y (2025 and later)" to "Model Y 焕新版（2025 款起）"),
            "model-y-l-2025" to ("Model Y L" to "Model Y L"),
            "model-s-2021" to ("Model S (2021 and later)" to "Model S（2021 款起）"),
            "model-s-2012" to ("Model S (2012–2020)" to "Model S（2012–2020 款）"),
            "model-x-2021" to ("Model X (2021 and later)" to "Model X（2021 款起）"),
            "model-x-2015" to ("Model X (2015–2020)" to "Model X（2015–2020 款）"),
            "cybertruck-2023" to ("Cybertruck" to "Cybertruck"))
        assertEquals(expected.keys.toList(), brand.models.map { it.id })
        expected.forEach { (id, names) ->
            val selection = CarSelection("tesla", model = id)
            assertEquals(selection, selection.normalized())
            val model = requireNotNull(catalog.model(selection))
            assertEquals(names.first, model.name(Locale.ENGLISH))
            assertEquals(names.second, model.name(Locale.SIMPLIFIED_CHINESE))
            assertTrue(model.screens.isEmpty())
        }
        assertEquals(CarSelection("tesla"), CarSelection("tesla", model = "unknown").normalized())
        assertEquals(CarSelection("tesla", model = "other", modelText = "Custom"),
            CarSelection("tesla", model = "other", modelText = " Custom ").normalized())
    }
    @Test fun packagedAssetHasLocalizedNamesAndObservedSizes() {
        val models = models()
        val brand = requireNotNull(models.brand(selected))
        val model = requireNotNull(models.model(selected))
        assertEquals("XPeng", brand.name(Locale.ENGLISH))
        assertEquals("小鹏", brand.name(Locale.SIMPLIFIED_CHINESE))
        assertEquals("XPeng", brand.name(Locale.TRADITIONAL_CHINESE))
        assertEquals("XPeng", brand.name(Locale.forLanguageTag("zh-Hant")))
        assertEquals("XPeng", brand.name(Locale.JAPANESE))
        assertEquals("G6 2023 款", model.name(Locale.SIMPLIFIED_CHINESE))
        assertEquals("G6 2023", model.name(Locale.TRADITIONAL_CHINESE))
        assertEquals("G6 2023", model.name(Locale.forLanguageTag("ar")))
        val data = model.screens.single()
        assertEquals(screen, data.screen)
        assertEquals(listOf(1533 to 1014, 2401 to 1201, 856 to 1014),
            CarScreenShare.sizes(data.records, true).map { it.width to it.height })
        assertEquals(listOf(1533 to 907, 2401 to 1201, 1533 to 1014),
            CarScreenShare.sizes(data.records, false).map { it.width to it.height })
        assertEquals(listOf(16L, 12L, 4L), CarScreenShare.sizes(data.records, true).map { it.count })
        assertEquals(listOf(5L, 4L, 1L), CarScreenShare.sizes(data.records, false).map { it.count })
        assertNull(models.model(CarSelection("other", model = "other")))
        assertTrue(runCatching { CarModels.parse("{}"); }.isFailure)
    }
    @Test fun mergesCountsByExactScreenAndModeWithoutMutatingLocalData() {
        val builtIn = requireNotNull(models().model(selected)).screens
        val local = listOf(WindowSizeRecord(2401, 1201, 280, 1533, 1014, true, 3, 20),
            WindowSizeRecord(2401, 1201, 280, 1533, 1014, false, 2, 21),
            WindowSizeRecord(1920, 1080, 160, 800, 600, false, 50, 22))
        val merged = ViewAreaPlanner.mergeRecords(local, builtIn, screen)
        assertEquals(6, merged.size)
        assertEquals(19L, merged.single { it.width == 1533 && it.height == 1014 && it.fullscreen }.count)
        assertEquals(3L, merged.single { it.width == 1533 && it.height == 1014 && !it.fullscreen }.count)
        assertEquals(3L, local.first().count)
        for (different in listOf(screen.copy(width = 2400), screen.copy(height = 1200), screen.copy(densityDpi = 281))) {
            val row = local.first().copy(screenWidth = different.width, screenHeight = different.height, densityDpi = different.densityDpi)
            assertEquals(listOf(row), ViewAreaPlanner.mergeRecords(local + row, builtIn, different))
        }
        val areas = ViewAreaPlanner.plan(1920, 960, merged)
        assertEquals(1533.0 / 1014, areas[1].width.toDouble() / areas[1].height, .003)
        assertTrue(areas[1].libraries.contains(ViewAreaLibrary.FULLSCREEN))
        assertEquals(6, builtIn.single().records.size)
    }
    @Test fun copiedTextHasThreeLinesOtherTextsObservedCountsAndOnlyApprovedFields() {
        val local = listOf(WindowSizeRecord(2401, 1201, 280, 1533, 907, false, 2, 99),
            WindowSizeRecord(2401, 1201, 280, 2401, 1201, true, 4, 100),
            WindowSizeRecord(2401, 1201, 320, 800, 600, false, 8, 101))
        val receiver = JSONObject("""{"userAgent":"Car browser","client":"secret","address":"private","name":"private","userAgentData":{"model":"private"},"videoDecode":[{"codec":"avc1.640028","width":1920,"height":1080,"preferHardware":true,"fps30":{"supported":true,"smooth":true,"powerEfficient":false,"id":"private"},"id":"private"}],"audioDecoder":[{"codec":"opus","supported":true,"name":"private"}]}""")
        val text = CarScreenShare.text("Screen data", "Brand: My brand · Model: My model", "0.1.2",
            CarSelection("other", " My brand\n", "other", " My model\t"), screen, local, receiver)
        val lines = text.lines()
        assertEquals(3, lines.size)
        assertEquals("Screen data", lines[0]); assertEquals("Brand: My brand · Model: My model", lines[1])
        val json = JSONObject(lines[2])
        assertEquals(setOf("format", "app", "brand", "brandText", "model", "modelText", "screen", "fullscreen", "windowed", "userAgent", "videoDecode", "audioDecoder"), json.keys().asSequence().toSet())
        assertEquals(1, json.getInt("format")); assertEquals("0.1.2", json.getString("app"))
        assertEquals("other", json.getString("brand")); assertEquals("other", json.getString("model"))
        assertEquals("My brand", json.getString("brandText")); assertEquals("My model", json.getString("modelText"))
        assertEquals(280, json.getJSONObject("screen").getInt("densityDpi"))
        assertEquals(4, json.getJSONArray("fullscreen").getJSONObject(0).getInt("count"))
        assertEquals(1, json.getJSONArray("windowed").length())
        assertEquals("Car browser", json.getString("userAgent"))
        assertEquals("opus", json.getJSONArray("audioDecoder").getJSONObject(0).getString("codec"))
        assertFalse(text.contains("private")); assertFalse(text.contains("secret")); assertFalse(text.contains("lastSeen"))
        val known = JSONObject(CarScreenShare.text("Title", "Car", "1", selected, screen, local).lines()[2])
        assertEquals("xpeng", known.getString("brand")); assertEquals("g6-2023", known.getString("model"))
        assertEquals("", known.getString("brandText")); assertEquals("", known.getString("modelText"))
        assertFalse(known.has("userAgent")); assertFalse(known.has("videoDecode"))
    }
    @Test fun diagnosticsKeepLatestInfoForTheExactScreenWithoutCopyingAnotherBrowser() {
        var clock = 0L
        val diagnostics = ReceiverDiagnostics { clock }
        fun info(width: Int, userAgent: String) = """{"type":"receiver-info","screen":{"width":$width,"height":686},"devicePixelRatio":1.75,"userAgent":"$userAgent"}"""
        diagnostics.report("private-client", "receiver-info", info(1372, "old"))
        diagnostics.report("another-client", "receiver-info", info(1920, "different"))
        diagnostics.report("private-client", "receiver-info", info(1372, "latest"))
        assertEquals("latest", diagnostics.latestInfo(screen)?.getString("userAgent"))
        assertNull(diagnostics.latestInfo(screen.copy(densityDpi = 281)))
        clock = ReceiverDiagnostics.EXPIRY_NANOS
        assertTrue(diagnostics.snapshot().isEmpty())
        assertEquals("latest", diagnostics.latestInfo(screen)?.getString("userAgent"))
        diagnostics.clear(); assertNull(diagnostics.latestInfo(screen))
    }
}
