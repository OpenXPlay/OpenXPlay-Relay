// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.relay

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.*

class DisplayRefitTest {
    private val display = DisplayInfo(2401, 1201, 240, 60.0, viewport = Viewport(1533, 906))
    private val running = DisplayTarget(DisplayProfile(1920, 960, 240), CarPlayResolution.AUTO, CarPlayIconSize.STANDARD)
    private val desired = running.copy(profile = DisplayProfile(1534, 906, 240))
    private fun due(gate: DisplayRefit, now: Long, applied: DisplayTarget = running, next: DisplayTarget = desired,
        mode: DisplayMode = DisplayMode.AUTOMATIC, active: Boolean = true, viewport: Boolean = true) =
        gate.due(now, active, mode, applied, next, viewport)

    @Test fun viewAreasCancelGeometryAndSettingRestartsUntilDisabled() {
        val gate = DisplayRefit()
        assertFalse(due(gate, 0)); assertTrue(due(gate, 1000))
        for (next in listOf(desired, running.copy(iconSize = CarPlayIconSize.SMALL),
            desired.copy(resolution = CarPlayResolution.SCREEN))) {
            assertFalse(gate.due(2000, true, DisplayMode.AUTOMATIC, running, next, true, viewAreas = true))
            assertFalse(gate.due(20000, true, DisplayMode.AUTOMATIC, running, next, true, viewAreas = true))
        }
        assertFalse(due(gate, 21000)); assertTrue(due(gate, 22000))
    }
    @Test fun automaticUsesViewportWithScreenFallbackAndScreenIgnoresViewport() {
        assertEquals(CarDisplayFit.fit(1533, 906), fitDisplay(display, CarPlayResolution.AUTO))
        assertEquals(CarDisplayFit.fit(2401, 1201), fitDisplay(display.copy(viewport = null), CarPlayResolution.AUTO))
        assertEquals(CarDisplayFit.fit(2401, 1201), fitDisplay(display, CarPlayResolution.SCREEN))
    }
    @Test fun resolutionCapsAndDecoderBudgetPreserveAspectAndOrientation() {
        for (viewport in listOf(Viewport(3000, 2000), Viewport(2000, 3000))) {
            for ((resolution, cap) in listOf(CarPlayResolution.P1080 to 1080, CarPlayResolution.P720 to 720)) {
                val fitted = checkNotNull(fitDisplay(display.copy(viewport = viewport), resolution))
                assertTrue(minOf(fitted.width, fitted.height) <= cap)
                assertTrue(kotlin.math.abs(fitted.width.toDouble() / fitted.height - viewport.width.toDouble() / viewport.height) < .01)
                assertEquals(viewport.width > viewport.height, fitted.width > fitted.height)
                val budget = checkNotNull(fitDisplay(display.copy(viewport = viewport), resolution, 3600))
                assertTrue(CarDisplayFit.macroblocks(budget.width, budget.height) <= 3600)
            }
        }
    }
    @Test fun iconFactorsAreAppliedBeforeThePhysicalSizeClampAndSnap() {
        val profile = DisplayProfile(1920, 960, 240)
        assertEquals(284, physicalWidth(profile, CarPlayIconSize.SMALL))
        assertEquals(203, physicalWidth(profile, CarPlayIconSize.STANDARD))
        assertEquals(152, physicalWidth(profile, CarPlayIconSize.LARGE))
    }
    @Test fun thresholdsAreStrictlyGreaterThanThreePercentAspectOrTenPercentSide() {
        val base = DisplayProfile(1000, 1000, 160)
        assertFalse(DisplayRefit.significant(base, base.copy(width = 1030)))
        assertTrue(DisplayRefit.significant(base, base.copy(width = 1032)))
        assertFalse(DisplayRefit.significant(base, base.copy(width = 1100, height = 1100)))
        assertTrue(DisplayRefit.significant(base, base.copy(width = 1102, height = 1102)))
        assertFalse(DisplayRefit.significant(base, base.copy(width = 900, height = 900)))
        assertTrue(DisplayRefit.significant(base, base.copy(width = 898, height = 898)))
    }
    @Test fun requiresOneStableSecondAndKeepsLatestPendingChangeUntilHoldOffEnds() {
        val gate = DisplayRefit()
        assertFalse(due(gate, 0)); assertFalse(due(gate, 999)); assertTrue(due(gate, 1000))
        gate.restarted(1000)
        val newer = desired.copy(profile = DisplayProfile(1280, 720, 160))
        assertFalse(due(gate, 2000, next = newer))
        assertFalse(due(gate, 10999, next = newer))
        assertTrue(due(gate, 11000, next = newer))
        gate.restarted(11000)
        assertFalse(due(gate, 12000, applied = newer, next = newer))
        assertFalse(due(gate, 22000, applied = newer, next = newer))
    }
    @Test fun instabilityAndReturnToRunningCanvasCancelPendingWork() {
        val gate = DisplayRefit()
        assertFalse(due(gate, 0))
        assertFalse(due(gate, 999, next = running))
        assertFalse(due(gate, 1000))
        assertFalse(due(gate, 1999)); assertTrue(due(gate, 2000))
    }
    @Test fun settingChangesRestartWithUnchangedPixelsButOverridesAndStoppedSessionsDisableRefit() {
        val small = running.copy(iconSize = CarPlayIconSize.SMALL)
        val gate = DisplayRefit()
        assertFalse(due(gate, 0, next = small, viewport = false))
        assertTrue(due(gate, 1000, next = small, viewport = false))
        for (mode in listOf(DisplayMode.FIXED, DisplayMode.RELAY_SCREEN)) {
            assertFalse(due(gate, 2000, next = small, mode = mode))
            assertFalse(due(gate, 4000, next = small, mode = mode))
        }
        assertFalse(due(gate, 5000, active = false)); assertFalse(due(gate, 7000, active = false))
        assertFalse(due(gate, 8000, viewport = false)); assertFalse(due(gate, 10000, viewport = false))
        val screen = running.copy(resolution = CarPlayResolution.SCREEN)
        assertFalse(due(gate, 11000, applied = screen, next = desired.copy(resolution = CarPlayResolution.SCREEN)))
        assertFalse(due(gate, 13000, applied = screen, next = desired.copy(resolution = CarPlayResolution.SCREEN)))
    }
}
