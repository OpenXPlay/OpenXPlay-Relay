// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.relay

import org.json.JSONArray
import org.json.JSONObject
import org.openxplay.direlay.settings.CarSelection
import java.util.Locale

data class ScreenDataKey(val width: Int, val height: Int, val densityDpi: Int) {
    fun matches(row: WindowSizeRecord) = row.screenWidth == width && row.screenHeight == height && row.densityDpi == densityDpi
    fun json() = JSONObject().put("width", width).put("height", height).put("densityDpi", densityDpi)
}
data class CarScreenData(val screen: ScreenDataKey, val records: List<WindowSizeRecord>)
data class CarModel(val id: String, val names: Map<String, String>, val screens: List<CarScreenData>) {
    fun name(locale: Locale) = localizedCarName(names, locale)
}
data class CarBrand(val id: String, val names: Map<String, String>, val models: List<CarModel>) {
    fun name(locale: Locale) = localizedCarName(names, locale)
}
fun localizedCarName(names: Map<String, String>, locale: Locale): String {
    // The app ships English and Simplified Chinese; Traditional Chinese falls back to English like the UI.
    val simplified = locale.language == "zh" && locale.script != "Hant" && locale.country !in listOf("TW", "HK", "MO")
    return names[if (simplified) "zh-CN" else "en"] ?: names.getValue("en")
}
class CarModels(val brands: List<CarBrand>) {
    fun brand(car: CarSelection) = brands.firstOrNull { it.id == car.brand }
    fun model(car: CarSelection) = brand(car)?.models?.firstOrNull { it.id == car.model }
    companion object {
        fun parse(text: String): CarModels {
            fun names(json: JSONObject): Map<String, String> = json.keys().asSequence().associateWith { json.getString(it) }
                .also { require(!it["en"].isNullOrBlank()) }
            val root = JSONObject(text)
            require(root.getInt("format") == 1)
            val brands = root.getJSONArray("brands")
            return CarModels((0 until brands.length()).map { b ->
                val brand = brands.getJSONObject(b)
                val models = brand.getJSONArray("models")
                CarBrand(brand.getString("id"), names(brand.getJSONObject("names")), (0 until models.length()).map { m ->
                    val model = models.getJSONObject(m)
                    val screens = model.getJSONArray("screens")
                    CarModel(model.getString("id"), names(model.getJSONObject("names")), (0 until screens.length()).map { s ->
                        val data = screens.getJSONObject(s)
                        val screen = data.getJSONObject("screen").let { ScreenDataKey(it.getInt("width"), it.getInt("height"), it.getInt("densityDpi")) }
                        require(listOf(screen.width, screen.height, screen.densityDpi).all { it in 1..32768 })
                        val records = listOf(true, false).flatMap { full ->
                            val rows = data.getJSONArray(if (full) "fullscreen" else "windowed")
                            (0 until rows.length()).map { i ->
                                val row = rows.getJSONObject(i)
                                val width = row.getInt("width"); val height = row.getInt("height"); val count = row.getLong("count")
                                require(width in 1..32768 && height in 1..32768 && count > 0)
                                WindowSizeRecord(screen.width, screen.height, screen.densityDpi, width, height, full, count, 0)
                            }
                        }
                        CarScreenData(screen, records)
                    })
                })
            })
        }
    }
}

/** Export only observed sizes and the receiver's browser/decoder capabilities. */
object CarScreenShare {
    fun sizes(records: List<WindowSizeRecord>, fullscreen: Boolean): List<WindowSizeRecord> = records.filter { it.fullscreen == fullscreen }
        .sortedWith(compareByDescending<WindowSizeRecord> { it.count }.thenByDescending { it.lastSeen }.thenBy { it.width }.thenBy { it.height })
    fun text(title: String, carLine: String, version: String, selection: CarSelection, screen: ScreenDataKey,
        records: List<WindowSizeRecord>, receiver: JSONObject? = null): String {
        val car = selection.normalized()
        fun rows(full: Boolean) = JSONArray().apply { sizes(records.filter(screen::matches), full).forEach {
            put(JSONObject().put("width", it.width).put("height", it.height).put("count", it.count))
        } }
        val json = JSONObject().put("format", 1).put("app", version).put("brand", car.brand).put("brandText", car.brandText)
            .put("model", car.model).put("modelText", car.modelText).put("screen", screen.json())
            .put("fullscreen", rows(true)).put("windowed", rows(false))
        receiver?.optString("userAgent")?.takeIf { it.isNotEmpty() }?.let { json.put("userAgent", it) }
        // Copy only the known capability fields, including nested MediaCapabilities results.
        for (key in listOf("videoDecode", "audioDecoder")) receiver?.optJSONArray(key)?.let { entries ->
            json.put(key, JSONArray().apply { for (i in 0 until entries.length()) entries.optJSONObject(i)?.let { row ->
                put(JSONObject().apply {
                    for (field in listOf("codec", "width", "height", "preferHardware", "preferSoftware", "mediaSource", "supported"))
                        row.opt(field)?.takeIf { it is String || it is Number || it is Boolean }?.let { put(field, it) }
                    for (field in listOf("fps30", "fps60")) row.optJSONObject(field)?.let { fps ->
                        put(field, JSONObject().apply { for (flag in listOf("supported", "smooth", "powerEfficient"))
                            (fps.opt(flag) as? Boolean)?.let { put(flag, it) } })
                    }
                })
            } })
        }
        return "$title\n$carLine\n$json"
    }
}
