// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.video

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.direlay.settings.CarDisplayFit
import org.openxplay.stream.core.*

class CarlinkGeometryTest {
    private val window = Viewport(1532, 1013)
    @Test fun wholeCanvasIsIdentityAndWindowNeverUpscalesThePicture() {
        val g = CarlinkGeometry.calculate(1156, 764, CarlinkCrop(0, 0, 1156, 764), window, decoderLimit = 3600)
        assertTrue(g.identity)
        assertEquals(Viewport(1156, 764), g.picture)
        assertEquals(window, g.window)
    }
    @Test fun explicitPictureSizeCompensatesTheWindowAspect() {
        val g = CarlinkGeometry.calculate(1156, 764, CarlinkCrop(0, 0, 1156, 764), window,
            Viewport(1280, 720), 3600)
        assertEquals(Viewport(1280, 720), g.picture)
        assertFalse(g.identity)
    }
    @Test fun areaAndDecoderLimitDetermineAnEvenPictureWithTheAreaAspect() {
        val g = CarlinkGeometry.calculate(1920, 1080, CarlinkCrop(100, 20, 1532, 1013), window, decoderLimit = 3600)
        assertFalse(g.identity)
        assertEquals(0, g.picture.width % 2)
        assertEquals(0, g.picture.height % 2)
        assertTrue(g.picture.width <= 1532 && g.picture.height <= 1013)
        assertTrue(CarDisplayFit.macroblocks(g.picture.width, g.picture.height) <= 3600)
        assertEquals(1532.0 / 1013, g.picture.width.toDouble() / g.picture.height, .005)
    }
    @Test fun cornersCentreAndReleaseMapThroughTheSameCrop() {
        val g = CarlinkGeometry.calculate(1600, 1200, CarlinkCrop(100, 200, 1200, 800), window)
        for ((u, v) in listOf(0.0 to 0.0, 1.0 to 0.0, 0.0 to 1.0, 1.0 to 1.0, .5 to .5)) {
            val mapped = g.map(TouchContact(2, u, v, false))
            assertEquals((100 + u * 1200) / 1600, mapped.x, 1e-12)
            assertEquals((200 + v * 800) / 1200, mapped.y, 1e-12)
            assertEquals(2, mapped.id)
            assertFalse(mapped.pressed)
        }
        val identity = CarlinkGeometry.calculate(1600, 1200, CarlinkCrop(0, 0, 1600, 1200), window)
        val contact = TouchContact(0, .25, .75, true)
        assertEquals(contact, identity.map(contact))
    }
    @Test fun rejectsAreasOutsideTheCanvas() {
        assertThrows(IllegalArgumentException::class.java) {
            CarlinkGeometry.calculate(100, 100, CarlinkCrop(0, 0, 102, 100), window)
        }
        assertThrows(IllegalArgumentException::class.java) {
            CarlinkGeometry.calculate(100, 100, CarlinkCrop(-1, 0, 100, 100), window)
        }
    }
}
