// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test

class HighPerformanceTest {
    @Test fun disabledHiddenOrNotLiveNeverActivates() {
        assertFalse(highPerformanceActive(false, 1_000L, 0L, true, 30_000L))
        assertFalse(highPerformanceActive(true, 1_000L, 0L, false, 30_000L))
        assertFalse(highPerformanceActive(true, null, 0L, true, 30_000L))
    }

    @Test fun liveBecomesMinimalAtTwentySeconds() {
        assertFalse(highPerformanceActive(true, 1_000L, 0L, true, 20_999L))
        assertTrue(highPerformanceActive(true, 1_000L, 0L, true, 21_000L))
        assertTrue(highPerformanceActive(true, 1_000L, 0L, true, 21_001L))
    }

    @Test fun touchWakesAndRearmsFromTheLaterOfLiveAndTouch() {
        assertFalse(highPerformanceActive(true, 1_000L, 25_000L, true, 25_000L))
        assertFalse(highPerformanceActive(true, 1_000L, 25_000L, true, 44_999L))
        assertTrue(highPerformanceActive(true, 1_000L, 25_000L, true, 45_000L))
        assertFalse(highPerformanceActive(true, 30_000L, 25_000L, true, 49_999L))
        assertTrue(highPerformanceActive(true, 30_000L, 25_000L, true, 50_000L))
    }

    @Test fun endingLiveClearsTheDeadlineAndANewSessionWaitsAgain() {
        assertTrue(highPerformanceActive(true, 1_000L, 2_000L, true, 30_000L))
        assertFalse(highPerformanceActive(true, null, 2_000L, true, 30_000L))
        assertFalse(highPerformanceActive(true, 30_000L, 0L, true, 30_000L))
        assertTrue(highPerformanceActive(true, 30_000L, 0L, true, 50_000L))
    }
}
