// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.core.FrameRatePreference

class CarPlayFrameRateTest {
    @Test fun fixedChoicesOverrideEveryNegotiatedRate() {
        val choices = mapOf(FrameRatePreference.FPS_30 to 30, FrameRatePreference.FPS_35 to 35,
            FrameRatePreference.FPS_40 to 40, FrameRatePreference.FPS_45 to 45,
            FrameRatePreference.FPS_50 to 50, FrameRatePreference.FPS_55 to 55, FrameRatePreference.FPS_60 to 60)
        assertEquals(FrameRatePreference.entries.toSet() - FrameRatePreference.AUTO, choices.keys)
        for ((setting, expected) in choices) for (negotiated in listOf(null, 24.0, 30.0, 59.94, 120.0, Double.NaN))
            assertEquals(expected, CarPlayFrameRate.requested(setting, negotiated))
    }

    @Test fun automaticUsesTheNegotiationRoundedAndClampedOrDefaultsToSixty() {
        val cases = listOf(null to 60, 30.0 to 30, 60.0 to 60, 59.94 to 60, 44.49 to 44, 44.5 to 45,
            0.0 to 30, 24.0 to 30, 120.0 to 60, -1e100 to 30, 1e100 to 60,
            Double.NaN to 60, Double.POSITIVE_INFINITY to 60, Double.NEGATIVE_INFINITY to 60)
        for ((negotiated, expected) in cases)
            assertEquals(expected, CarPlayFrameRate.requested(FrameRatePreference.AUTO, negotiated))
    }
}
