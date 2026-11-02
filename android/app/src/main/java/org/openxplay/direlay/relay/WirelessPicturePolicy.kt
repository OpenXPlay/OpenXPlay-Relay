// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import kotlin.math.roundToInt
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.DisplayInfo
import org.openxplay.stream.core.Viewport

enum class WirelessPictureSource { BROWSER, CAR_MODEL, DEFAULT, CUSTOM }
data class WirelessPictureResolution(val size: WirelessPictureSize, val source: WirelessPictureSource, val densityDpi: Int = 160) {
    fun display() = DisplayInfo(size.width, size.height, densityDpi, 30.0)
}

object WirelessPicturePolicy {
    /** No live display enters this path: wireless declarations must never feed their own next size. */
    fun resolve(settings: RelaySettings, saved: SavedHeadUnit? = null, year: CarYear? = null): WirelessPictureResolution {
        if (settings.wirelessPicture == WirelessPictureMode.CUSTOM)
            return WirelessPictureResolution(settings.wirelessPictureSize.fitted(), WirelessPictureSource.CUSTOM)
        if (WirelessStandIn.wirelessOnly(settings.outputs)) {
            val display = saved?.display?.screen ?: year?.screens?.firstOrNull()?.screen
            return when {
                display != null -> WirelessPictureResolution(WirelessPictureSize(display.width, display.height),
                    if (saved?.display != null) WirelessPictureSource.BROWSER else WirelessPictureSource.CAR_MODEL, display.densityDpi)
                else -> WirelessPictureResolution(WirelessPictureSize(1920, 1080), WirelessPictureSource.DEFAULT, 240)
            }
        }
        // Keep the former default declaration when an interactive output shares the canvas.
        val known = checkNotNull(startupCarPlayScreen(settings.copy(displayMode = DisplayMode.AUTOMATIC), null, saved, year))
        val source = when (known.source) {
            "remembered" -> WirelessPictureSource.BROWSER
            "built-in model" -> WirelessPictureSource.CAR_MODEL
            else -> WirelessPictureSource.DEFAULT
        }
        val scale = 720.0 / minOf(known.profile.width, known.profile.height)
        val size = WirelessPictureSize((known.profile.width * scale).roundToInt(), (known.profile.height * scale).roundToInt()).fitted()
        return WirelessPictureResolution(size, source)
    }

    fun customCanvas(settings: RelaySettings): Boolean =
        WirelessStandIn.wirelessOnly(settings.outputs) && settings.wirelessPicture == WirelessPictureMode.CUSTOM

    fun resolution(settings: RelaySettings): CarPlayResolution =
        if (customCanvas(settings)) CarPlayResolution.AUTO else settings.carplayResolution

    fun canvas(settings: RelaySettings, picture: WirelessPictureResolution, decoderLimit: Int? = null): DisplayProfile? =
        fitDisplay(picture.display(), resolution(settings), decoderLimit, wholeScreen = true)

    fun startupScreen(settings: RelaySettings, live: DisplayProfile?, saved: SavedHeadUnit?, year: CarYear?, decoderLimit: Int?): CarPlayScreen? {
        if (!WirelessStandIn.wirelessOnly(settings.outputs) || !settings.dlnaOutput && !customCanvas(settings))
            return startupCarPlayScreen(settings, live, saved, year)
        val picture = resolve(settings, saved, year)
        val screen = resolveCarPlayScreen(settings.displayMode, settings.display, live ?: canvas(settings, picture, decoderLimit), null)
        return if (live == null && screen?.source == "car screen") screen.copy(source = when (picture.source) {
            WirelessPictureSource.BROWSER -> "remembered"
            WirelessPictureSource.CAR_MODEL -> "built-in model"
            else -> "default"
        }) else screen
    }

    fun customStart(settings: RelaySettings, auto: WirelessPictureResolution, display: Viewport?): WirelessPictureSize =
        if (!settings.dlnaOutput && settings.miracastOutput && display != null)
            WirelessPictureSize(display.width, display.height).fitted() else auto.size.fitted()

    fun choose(settings: RelaySettings, mode: WirelessPictureMode, autoSize: WirelessPictureSize): RelaySettings =
        if (mode == WirelessPictureMode.CUSTOM && settings.wirelessPicture != mode)
            settings.copy(wirelessPicture = mode, wirelessPictureSize = autoSize)
        else settings.copy(wirelessPicture = mode)
}
