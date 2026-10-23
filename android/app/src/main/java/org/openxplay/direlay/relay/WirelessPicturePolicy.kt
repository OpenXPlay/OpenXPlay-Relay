// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import kotlin.math.roundToInt
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.Viewport

enum class WirelessPictureSource { BROWSER, CAR_MODEL, DEFAULT, CUSTOM }
data class WirelessPictureResolution(val size: WirelessPictureSize, val source: WirelessPictureSource)

object WirelessPicturePolicy {
    /** No live display enters this path: wireless declarations must never feed their own next size. */
    fun resolve(settings: RelaySettings, saved: SavedHeadUnit? = null, model: CarModel? = null): WirelessPictureResolution {
        if (settings.wirelessPicture == WirelessPictureMode.CUSTOM)
            return WirelessPictureResolution(settings.wirelessPictureSize.fitted(), WirelessPictureSource.CUSTOM)
        val known = checkNotNull(startupCarPlayScreen(settings.copy(displayMode = DisplayMode.AUTOMATIC), null, saved, model))
        val source = when (known.source) {
            "remembered" -> WirelessPictureSource.BROWSER
            "built-in model" -> WirelessPictureSource.CAR_MODEL
            else -> WirelessPictureSource.DEFAULT
        }
        val shortSide = if (settings.dlnaPicture == DlnaPicture.P720) 720 else 1080
        val scale = shortSide.toDouble() / minOf(known.profile.width, known.profile.height)
        val size = WirelessPictureSize((known.profile.width * scale).roundToInt(), (known.profile.height * scale).roundToInt()).fitted()
        return WirelessPictureResolution(size, source)
    }

    fun customStart(settings: RelaySettings, auto: WirelessPictureResolution, display: Viewport?): WirelessPictureSize =
        if (!settings.dlnaOutput && settings.miracastOutput && display != null)
            WirelessPictureSize(display.width, display.height).fitted() else auto.size

    fun choose(settings: RelaySettings, mode: WirelessPictureMode, autoSize: WirelessPictureSize): RelaySettings =
        if (mode == WirelessPictureMode.CUSTOM && settings.wirelessPicture != mode)
            settings.copy(wirelessPicture = mode, wirelessPictureSize = autoSize)
        else settings.copy(wirelessPicture = mode)
}
