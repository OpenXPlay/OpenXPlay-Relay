// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.settings

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

class CarDisplayFitTest {
    @Test fun carSizesKeepOrientationAndFitEvenValidatedBounds() {
        val cases = listOf(
            listOf(1920, 720, 1920, 720, 240), listOf(1280, 720, 1280, 720, 160),
            listOf(800, 480, 800, 480, 160), listOf(1024, 600, 1024, 600, 160),
            listOf(2400, 1080, 1920, 864, 240), listOf(3200, 1440, 1920, 864, 240),
            listOf(1440, 3200, 864, 1920, 240), listOf(2560, 720, 1920, 540, 240),
            listOf(480, 272, 640, 362, 160), listOf(272, 480, 362, 640, 160),
            listOf(1921, 721, 1920, 720, 240), listOf(801, 481, 802, 482, 160),
            listOf(1279, 719, 1280, 720, 160), listOf(1281, 721, 1282, 722, 240),
            listOf(100, 100, 640, 640, 160),
            listOf(641, 642, 642, 644, 160), listOf(642, 641, 644, 642, 160),
        )
        cases.forEach { (w, h, expectedW, expectedH, dpi) ->
            val result = CarDisplayFit.fit(w, h)
            assertEquals(DisplayProfile(expectedW, expectedH, dpi), result, "$w × $h")
            result.validate()
            assertEquals(w < h, result.width < result.height)
            val scale = result.width.toDouble() / w
            assertTrue(kotlin.math.abs(result.height - h * scale) <= 2.0, "$w × $h aspect ratio")
        }
    }

    @Test fun impossibleOrMissingSizesAreRejectedWithoutDistortingTheCar() {
        for ((w, h) in listOf(0 to 720, 1280 to 0, -1 to 720, 10000 to 100, 100 to 10000)) {
            assertThrows(IllegalArgumentException::class.java) { CarDisplayFit.fit(w, h) }
        }
    }

    @Test fun modeMigrationPreservesFixedAndMovesLegacyDeviceDisplayToAutomatic() {
        assertEquals(DisplayMode.AUTOMATIC, migratedDisplayMode(null, null))
        assertEquals(DisplayMode.FIXED, migratedDisplayMode(null, CarDisplayFit.default))
        assertEquals(DisplayMode.RELAY_SCREEN, migratedDisplayMode("RELAY_SCREEN", null))
        assertEquals(DisplayMode.AUTOMATIC, migratedDisplayMode("AUTOMATIC", CarDisplayFit.default))
        assertEquals(DisplayMode.AUTOMATIC, migratedDisplayMode("FIXED", null))
    }

    @Test fun startUsesCarThenMemoryThenDefaultAndHonoursExplicitModes() {
        val car = DisplayProfile(1920, 720, 240)
        val memory = DisplayProfile(800, 480, 160)
        assertEquals(CarPlayScreen(car, "car screen"), resolveCarPlayScreen(DisplayMode.AUTOMATIC, null, car, memory))
        assertEquals(CarPlayScreen(memory, "remembered"), resolveCarPlayScreen(DisplayMode.AUTOMATIC, null, null, memory))
        assertEquals(CarPlayScreen(CarDisplayFit.default, "default"), resolveCarPlayScreen(DisplayMode.AUTOMATIC, null, null, null))
        assertEquals(CarPlayScreen(memory, "fixed"), resolveCarPlayScreen(DisplayMode.FIXED, memory, car, null))
        assertNull(resolveCarPlayScreen(DisplayMode.RELAY_SCREEN, memory, car, memory))
    }

    @Test fun reconnectIsOnlyOfferedForADifferentCarWhileAutomaticIsRunning() {
        val inUse = CarDisplayFit.default
        val changed = CarDisplayFit.fit(1920, 720)
        for (running in listOf(false, true)) for (mode in DisplayMode.entries) {
            assertEquals(running && mode == DisplayMode.AUTOMATIC, carDisplayMismatch(running, mode, inUse, changed))
            assertFalse(carDisplayMismatch(running, mode, inUse, inUse))
            assertFalse(carDisplayMismatch(running, mode, inUse, null))
            assertFalse(carDisplayMismatch(running, mode, null, changed))
        }
        assertFalse(carDisplayMismatch(true, DisplayMode.AUTOMATIC, changed, CarDisplayFit.fit(2560, 960)))
    }

    @Test fun carlinkLevel31DecoderCapsThePortraitCanvas() {
        val limit = CarDisplayFit.h264MaxMacroblocks("avc1.42E01F")
        assertEquals(3600, limit)
        val fitted = CarDisplayFit.fit(1072, 2320, limit)
        assertTrue(CarDisplayFit.macroblocks(fitted.width, fitted.height) <= 3600)
        assertTrue(fitted.height > fitted.width)
        assertEquals(1072.0 / 2320, fitted.width.toDouble() / fitted.height, 0.01)
        assertEquals(0, fitted.width % 2); assertEquals(0, fitted.height % 2)
    }

    @Test fun browserDecodersDoNotShrinkTheCanvas() {
        assertEquals(36864, CarDisplayFit.h264MaxMacroblocks("avc1.640033"))
        assertEquals(null, CarDisplayFit.h264MaxMacroblocks("hvc1.1.6.L120.90"))
        assertEquals(CarDisplayFit.fit(1920, 864), CarDisplayFit.fit(1920, 864, 36864))
    }
}
