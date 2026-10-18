// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.relay

import android.content.SharedPreferences
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.DisplayInfo
import org.openxplay.stream.core.Viewport

/** Only the display fields used by the relay, independent of Stream's persistence format. */
data class HeadUnitDisplay(val screen: ScreenDataKey, val visible: ViewAreaSize?, val fullscreen: Boolean?, val preferred: Boolean?) {
    fun display() = DisplayInfo(screen.width, screen.height, screen.densityDpi, 60.0,
        viewport = visible?.let { Viewport(it.width, it.height) }, fullscreen = fullscreen, fullscreenPreferred = preferred)
    companion object {
        fun from(display: DisplayInfo) = HeadUnitDisplay(ScreenDataKey(display.widthPx, display.heightPx, display.densityDpi),
            display.viewport?.let { ViewAreaSize(it.width, it.height) }, display.fullscreen, display.fullscreenPreferred)
        fun builtIn(data: CarScreenData) = HeadUnitDisplay(data.screen, null, true, true)
    }
}
data class SavedHeadUnit(val canvas: DisplayProfile, val display: HeadUnitDisplay?, val resolution: CarPlayResolution? = null,
    val wholeScreen: Boolean = false, val maxMacroblocks: Int? = null)

/** One atomic preference edit keeps the canvas and its originating report together; reads accept legacy canvases. */
class SavedHeadUnitStore(private val preferences: SharedPreferences) {
    fun load(): SavedHeadUnit? = runCatching {
        val canvas = DisplayProfile(preferences.getInt("width", 0), preferences.getInt("height", 0),
            preferences.getInt("density", 0)).also { it.validate() }
        val display = runCatching {
            fun size(key: String) = preferences.getInt(key, 0).also { require(it in 1..32768) }
            fun flag(key: String): Boolean? = when (preferences.getInt(key, -1)) { 0 -> false; 1 -> true; -1 -> null; else -> error("Invalid flag") }
            val screen = ScreenDataKey(size("screenWidth"), size("screenHeight"), size("screenDensity"))
            val visible = if (preferences.contains("visibleWidth")) ViewAreaSize(size("visibleWidth"), size("visibleHeight")) else null
            HeadUnitDisplay(screen, visible, flag("fullscreen"), flag("fullscreenPreferred"))
        }.getOrNull()
        SavedHeadUnit(canvas, display, CarPlayResolution.entries.firstOrNull { it.name == preferences.getString("resolution", null) },
            preferences.getBoolean("wholeScreen", false), preferences.getInt("maxMacroblocks", 0).takeIf { it > 0 })
    }.getOrNull()
    fun save(value: SavedHeadUnit) {
        val report = checkNotNull(value.display)
        fun flag(value: Boolean?) = when (value) { true -> 1; false -> 0; null -> -1 }
        preferences.edit().putInt("width", value.canvas.width).putInt("height", value.canvas.height).putInt("density", value.canvas.density)
            .putInt("screenWidth", report.screen.width).putInt("screenHeight", report.screen.height).putInt("screenDensity", report.screen.densityDpi)
            .apply { if (report.visible == null) { remove("visibleWidth"); remove("visibleHeight") } else {
                putInt("visibleWidth", report.visible.width); putInt("visibleHeight", report.visible.height)
            } }.putInt("fullscreen", flag(report.fullscreen)).putInt("fullscreenPreferred", flag(report.preferred))
            .putString("resolution", value.resolution?.name).putBoolean("wholeScreen", value.wholeScreen)
            .putInt("maxMacroblocks", value.maxMacroblocks ?: 0).apply()
    }
    fun clear() { preferences.edit().clear().apply() }
}

fun planningDisplay(live: HeadUnitDisplay?, saved: SavedHeadUnit?, model: CarModel?): HeadUnitDisplay? =
    live ?: saved?.display ?: model?.screens?.firstOrNull()?.let(HeadUnitDisplay::builtIn)

fun startupCarPlayScreen(settings: RelaySettings, live: DisplayProfile?, saved: SavedHeadUnit?, model: CarModel?): CarPlayScreen? {
    val whole = settings.viewAreas && settings.displayMode == DisplayMode.AUTOMATIC
    val builtIn = model?.screens?.firstOrNull()?.let { fitDisplay(HeadUnitDisplay.builtIn(it).display(), settings.carplayResolution, wholeScreen = true) }
    val remembered = saved?.takeIf { it.display != null || builtIn == null }?.let {
        if (it.display != null && (it.resolution != null || settings.carplayResolution == CarPlayResolution.NATIVE) && (it.resolution != settings.carplayResolution || it.wholeScreen != whole))
            fitDisplay(it.display.display(), settings.carplayResolution, it.maxMacroblocks, whole) else it.canvas
    }
    return resolveCarPlayScreen(settings.displayMode, settings.display, live, remembered, builtIn)
}

fun planHeadUnitAreas(width: Int, height: Int, display: HeadUnitDisplay?, local: List<WindowSizeRecord>, model: CarModel?): List<ViewAreaSize> {
    val records = display?.let { ViewAreaPlanner.mergeRecords(local, model?.screens.orEmpty(), it.screen) }.orEmpty()
    return ViewAreaPlanner.plan(width, height, records, display?.visible, display?.fullscreen)
}
