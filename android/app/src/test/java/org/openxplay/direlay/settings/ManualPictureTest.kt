// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay.settings

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

class ManualPictureTest {
    @Test fun declaredWidthPrioritizesDeveloperThenExactCustomThenScaledPreset() {
        val canvas = DisplayProfile(2400, 1200, 300)
        for (preset in CarPlayScreenWidth.entries) {
            assertEquals(225, declaredPhysicalWidth(canvas, preset, customMm = 235, developerOverrideMm = 225))
            assertEquals(235, declaredPhysicalWidth(canvas, preset, customMm = 235))
            assertEquals(physicalWidth(canvas, preset), declaredPhysicalWidth(canvas, preset))
        }
        assertEquals(250, declaredPhysicalWidth(canvas, CarPlayScreenWidth.MM200))
    }

    @Test fun canvasReferenceUsesOnlyARunningAutomaticSessionAndOtherwiseTheSavedAutomaticCanvas() {
        val saved = DisplayProfile(800, 480, 160)
        val applied = DisplayProfile(1920, 720, 240)
        for (running in listOf(false, true)) for (mode in DisplayMode.entries) {
            assertEquals(if (running && mode == DisplayMode.AUTOMATIC) applied else saved,
                referenceCarPlayCanvas(running, mode, applied, saved))
            assertEquals(saved, referenceCarPlayCanvas(running, mode, null, saved))
        }
        // A parked manual profile has no effect on automatic canvas selection.
        assertEquals(CarPlayScreen(saved, "remembered"), resolveCarPlayScreen(DisplayMode.AUTOMATIC, applied, null, saved))
    }

    @Test fun viewAreaInputUsesDisplayBoundsRatherThanWirelessPictureFitting() {
        for (size in listOf(PictureSize(320, 240), PictureSize(3840, 2160), PictureSize(320, 2160))) size.validateViewArea()
        for (size in listOf(PictureSize(318, 240), PictureSize(320, 238), PictureSize(3842, 2160),
            PictureSize(3840, 2162), PictureSize(801, 480), PictureSize(800, 481)))
            assertThrows(IllegalArgumentException::class.java) { size.validateViewArea() }
    }
}
