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
    private val selected = CarSelection("xpeng", model = "g6", year = "g6-2023")
    private val screen = ScreenDataKey(2401, 1201, 280)
    @Test fun parserAcceptsOnlyFormatTwoAndKeepsNameSizeAndCountValidation() {
        fun root() = JSONObject(RuntimeEnvironment.getApplication().assets.open("car-models.json").bufferedReader().use { it.readText() })
        fun brand(root: JSONObject) = root.getJSONArray("brands").getJSONObject(0)
        fun model(root: JSONObject) = brand(root).getJSONArray("models").getJSONObject(4)
        fun year(root: JSONObject) = model(root).getJSONArray("years").getJSONObject(0)
        fun data(root: JSONObject) = year(root).getJSONArray("screens").getJSONObject(0)
        val mutations: List<(JSONObject) -> Unit> = listOf(
            { it.put("format", 1) }, { it.put("format", 3) },
            { brand(it).getJSONObject("names").put("en", " ") },
            { model(it).getJSONObject("names").remove("en") },
            { year(it).getJSONObject("names").put("en", "") },
            { data(it).getJSONObject("screen").put("width", 0) },
            { data(it).getJSONObject("screen").put("height", 32769) },
            { data(it).getJSONObject("screen").put("densityDpi", -1) },
            { data(it).getJSONArray("fullscreen").getJSONObject(0).put("width", 0) },
            { data(it).getJSONArray("windowed").getJSONObject(0).put("height", 32769) },
            { data(it).getJSONArray("fullscreen").getJSONObject(0).put("count", 0) },
            { data(it).getJSONArray("windowed").getJSONObject(0).put("count", -1) })
        mutations.forEach { mutate ->
            val invalid = root().also(mutate)
            assertTrue(runCatching { CarModels.parse(invalid.toString()) }.isFailure)
        }
    }
    @Test fun assetAndNormalizationTableHaveExactlyTheSameUniqueIds() {
        val catalog = models()
        val ids = catalog.brands.associate { brand -> brand.id to brand.models.associate { model ->
            model.id to model.years.map { it.id }.toSet()
        } }
        assertEquals(CarSelection.catalogIds, ids)
        assertEquals(catalog.brands.size, ids.size)
        catalog.brands.forEach { brand ->
            assertEquals(brand.models.size, ids.getValue(brand.id).size)
            val years = brand.models.flatMap { it.years }
            assertEquals(years.size, years.map { it.id }.toSet().size)
            assertFalse(brand.names["en"].isNullOrBlank()); assertFalse(brand.names["zh-CN"].isNullOrBlank())
            brand.models.forEach { model ->
                assertFalse(model.names["en"].isNullOrBlank()); assertFalse(model.names["zh-CN"].isNullOrBlank())
                assertEquals(model.years.map { it.id }.sorted(), model.years.map { it.id })
                model.years.forEach { year ->
                    assertTrue(year.id.startsWith("${model.id}-"))
                    assertFalse(year.names["en"].isNullOrBlank()); assertFalse(year.names["zh-CN"].isNullOrBlank())
                    val selection = CarSelection(brand.id, model = model.id, year = year.id)
                    assertEquals(selection, selection.normalized())
                    assertEquals(selection, CarSelection(brand.id, model = year.id).normalized())
                    if (year.id != "g6-2023") assertTrue(year.screens.isEmpty())
                }
            }
        }
    }
    @Test fun teslaModelsGroupYearsOldestFirstWithBareLocalizedYearNames() {
        val brand = requireNotNull(models().brand(CarSelection("tesla")))
        assertEquals("Tesla", brand.name(Locale.ENGLISH))
        assertEquals("特斯拉", brand.name(Locale.SIMPLIFIED_CHINESE))
        assertEquals(listOf("model-3", "model-y", "model-y-l", "model-s", "model-x", "cybertruck"), brand.models.map { it.id })
        assertEquals(listOf("Model 3", "Model Y", "Model Y L", "Model S", "Model X", "Cybertruck"), brand.models.map { it.name(Locale.ENGLISH) })
        val years = brand.models.flatMap { it.years }
        assertEquals(listOf("model-3-2017", "model-3-2024", "model-y-2020", "model-y-2025", "model-y-l-2025",
            "model-s-2012", "model-s-2021", "model-x-2015", "model-x-2021", "cybertruck-2023"), years.map { it.id })
        assertEquals(listOf("2017–2023", "2024+", "2020–2024", "2025+", "2025+", "2012–2020", "2021+", "2015–2020", "2021+", "2023+"), years.map { it.name(Locale.ENGLISH) })
        assertEquals(listOf("2017–2023", "2024 起（焕新版）", "2020–2024", "2025 起（焕新版）", "2025 起",
            "2012–2020", "2021 起", "2015–2020", "2021 起", "2023 起"), years.map { it.name(Locale.SIMPLIFIED_CHINESE) })
    }
    @Test fun xpengModelsKeepLaunchOrderAndOnlyG62023HasMeasuredScreenData() {
        val brand = requireNotNull(models().brand(selected))
        assertEquals(listOf("g3", "p7", "p5", "g9", "g6", "x9", "mona-m03", "p7-plus", "g7", "gx", "mona-l03", "g9l"), brand.models.map { it.id })
        assertEquals(listOf("G3 / G3i", "P7 / P7i", "P5", "G9", "G6", "X9", "MONA M03", "P7+", "G7", "GX", "MONA L03", "G9L"), brand.models.map { it.name(Locale.ENGLISH) })
        val expected = listOf(listOf(2019, 2020, 2021, 2022), listOf(2020, 2021, 2022, 2023, 2024, 2025),
            listOf(2021, 2022, 2023, 2024), listOf(2022, 2024, 2025, 2026), listOf(2023, 2024, 2025, 2026),
            listOf(2024, 2025, 2026), listOf(2024, 2025, 2026), listOf(2024, 2025, 2026), listOf(2025, 2026), listOf(2026), listOf(2026), listOf(2026))
        brand.models.zip(expected).forEach { (model, years) ->
            assertEquals(model.name(Locale.ENGLISH), model.name(Locale.SIMPLIFIED_CHINESE))
            assertEquals(years.map(Int::toString), model.years.map { it.name(Locale.ENGLISH) })
            assertEquals(years.map(Int::toString), model.years.map { it.name(Locale.SIMPLIFIED_CHINESE) })
        }
        assertEquals(listOf("g6-2023"), brand.models.flatMap { it.years }.filter { it.screens.isNotEmpty() }.map { it.id })
        assertNull(models().year(selected.copy(year = "")))
        assertNull(models().year(selected.copy(model = "g9")))
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
        assertEquals("G6", model.name(Locale.SIMPLIFIED_CHINESE))
        assertEquals("G6", model.name(Locale.TRADITIONAL_CHINESE))
        assertEquals("G6", model.name(Locale.forLanguageTag("ar")))
        val year = requireNotNull(models.year(selected))
        assertEquals("2023", year.name(Locale.SIMPLIFIED_CHINESE))
        val data = year.screens.single()
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
        val builtIn = requireNotNull(models().year(selected)).screens
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
        val text = CarScreenShare.text("Screen data", "Brand: My brand · Model: My model · Year: My year", "0.1.2",
            CarSelection("other", " My brand\n", "other", " My model\t", "other", " My year\n"), screen, local, receiver)
        val lines = text.lines()
        assertEquals(3, lines.size)
        assertEquals("Screen data", lines[0]); assertEquals("Brand: My brand · Model: My model · Year: My year", lines[1])
        val json = JSONObject(lines[2])
        assertEquals(setOf("format", "app", "brand", "brandText", "model", "modelText", "year", "yearText", "screen", "fullscreen", "windowed", "userAgent", "videoDecode", "audioDecoder"), json.keys().asSequence().toSet())
        assertEquals(2, json.getInt("format")); assertEquals("0.1.2", json.getString("app"))
        assertEquals("other", json.getString("brand")); assertEquals("other", json.getString("model"))
        assertEquals("My brand", json.getString("brandText")); assertEquals("My model", json.getString("modelText"))
        assertEquals("other", json.getString("year")); assertEquals("My year", json.getString("yearText"))
        assertEquals(280, json.getJSONObject("screen").getInt("densityDpi"))
        assertEquals(4, json.getJSONArray("fullscreen").getJSONObject(0).getInt("count"))
        assertEquals(1, json.getJSONArray("windowed").length())
        assertEquals("Car browser", json.getString("userAgent"))
        assertEquals("opus", json.getJSONArray("audioDecoder").getJSONObject(0).getString("codec"))
        assertFalse(text.contains("private")); assertFalse(text.contains("secret")); assertFalse(text.contains("lastSeen"))
        val known = JSONObject(CarScreenShare.text("Title", "Car", "1", selected, screen, local).lines()[2])
        assertEquals("xpeng", known.getString("brand")); assertEquals("g6", known.getString("model")); assertEquals("g6-2023", known.getString("year"))
        assertEquals("", known.getString("yearText"))
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
