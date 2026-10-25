// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.relay

import org.openxplay.stream.core.FrameRatePreference
import kotlin.math.roundToInt

object CarPlayFrameRate {
    fun requested(setting: FrameRatePreference, negotiated: Double?): Int = when (setting) {
        FrameRatePreference.FPS_30 -> 30
        FrameRatePreference.FPS_35 -> 35
        FrameRatePreference.FPS_40 -> 40
        FrameRatePreference.FPS_45 -> 45
        FrameRatePreference.FPS_50 -> 50
        FrameRatePreference.FPS_55 -> 55
        FrameRatePreference.FPS_60 -> 60
        FrameRatePreference.AUTO -> negotiated?.takeIf { it.isFinite() }?.coerceIn(30.0, 60.0)?.roundToInt() ?: 60
    }
}
