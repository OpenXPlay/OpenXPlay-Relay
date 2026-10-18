// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.settings

import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

object CarDisplayFit {
    val default = DisplayProfile(1280, 720, 160)

    /**
     * [maxMacroblocks] is the receiver decoder's frame-size limit (H.264 MaxFS); it takes priority over
     * the minimum size, because a canvas the head unit cannot decode shows only black (2026-10-15:
     * 888×1920 sent to a level 3.1 Carlink decoder, limit 3600 macroblocks = 1280×720).
     */
    fun fit(width: Int, height: Int, maxMacroblocks: Int? = null): DisplayProfile {
        val fitted = fit(width, height)
        if (maxMacroblocks == null || macroblocks(fitted.width, fitted.height) <= maxMacroblocks) return fitted
        var scale = kotlin.math.sqrt(maxMacroblocks * 256.0 / (fitted.width.toDouble() * fitted.height))
        while (true) {
            val w = ((fitted.width * scale / 2).toInt() * 2).coerceAtLeast(320)
            val h = ((fitted.height * scale / 2).toInt() * 2).coerceAtLeast(240)
            if (macroblocks(w, h) <= maxMacroblocks || scale < 0.1)
                return DisplayProfile(w, h, if (max(w, h) <= 1280 && min(w, h) <= 720) 160 else 240).also { it.validate() }
            scale *= 0.98
        }
    }

    fun macroblocks(width: Int, height: Int) = ((width + 15) / 16) * ((height + 15) / 16)

    /** H.264 MaxFS by level for `avc1.PPCCLL` codec strings; null when unknown or not H.264. */
    fun h264MaxMacroblocks(codecString: String): Int? {
        val match = Regex("^avc[13]\\.[0-9A-Fa-f]{4}([0-9A-Fa-f]{2})$").find(codecString) ?: return null
        return when (match.groupValues[1].toInt(16)) {
            in 0..10 -> 99; 11 -> 396; 12, 13, 20 -> 396; 21 -> 792; 22, 30 -> 1620; 31 -> 3600; 32 -> 5120
            40, 41 -> 8192; 42 -> 8704; 50 -> 22080; 51, 52 -> 36864; else -> 139264
        }
    }

    private fun fit(width: Int, height: Int): DisplayProfile {
        require(width > 0 && height > 0)
        val portrait = height > width
        val minWidth = if (portrait) 360 else 640
        val minHeight = if (portrait) 640 else 360
        val maxWidth = if (portrait) 1080 else 1920
        val maxHeight = if (portrait) 1920 else 1080
        val lower = max(minWidth.toDouble() / width, minHeight.toDouble() / height)
        val upper = min(maxWidth.toDouble() / width, maxHeight.toDouble() / height)
        // Ratios outside these bounds cannot satisfy both limits without distortion.
        require(lower <= upper) { "Car display aspect ratio cannot fit the CarPlay bounds." }
        val scale = 1.0.coerceIn(lower, upper)
        fun even(value: Double, low: Int, high: Int) = ((value / 2).roundToInt() * 2).coerceIn(low, high)
        var w = even(width * scale, minWidth, maxWidth)
        var h = even(height * scale, minHeight, maxHeight)
        // Rounding nearly square odd sizes must not turn a portrait or landscape into a square.
        if (height > width && h <= w) h = w + 2
        if (width > height && w <= h) w = h + 2
        return DisplayProfile(w, h, if (max(w, h) <= 1280 && min(w, h) <= 720) 160 else 240)
            .also { it.validate() }
    }
}

enum class DisplayMode { AUTOMATIC, RELAY_SCREEN, FIXED }
fun migratedDisplayMode(stored: String?, profile: DisplayProfile?): DisplayMode =
    DisplayMode.entries.firstOrNull { it.name == stored && (it != DisplayMode.FIXED || profile != null) }
        ?: if (profile != null) DisplayMode.FIXED else DisplayMode.AUTOMATIC

data class CarPlayScreen(val profile: DisplayProfile, val source: String)
fun resolveCarPlayScreen(mode: DisplayMode, fixed: DisplayProfile?, car: DisplayProfile?, remembered: DisplayProfile?, builtIn: DisplayProfile? = null): CarPlayScreen? =
    when (mode) {
        DisplayMode.RELAY_SCREEN -> null
        DisplayMode.FIXED -> CarPlayScreen(checkNotNull(fixed), "fixed")
        DisplayMode.AUTOMATIC -> when {
            car != null -> CarPlayScreen(car, "car screen")
            remembered != null -> CarPlayScreen(remembered, "remembered")
            builtIn != null -> CarPlayScreen(builtIn, "built-in model")
            else -> CarPlayScreen(CarDisplayFit.default, "default")
        }
    }

fun carDisplayMismatch(running: Boolean, mode: DisplayMode, inUse: DisplayProfile?, car: DisplayProfile?) =
    running && mode == DisplayMode.AUTOMATIC && inUse != null && car != null && inUse != car
