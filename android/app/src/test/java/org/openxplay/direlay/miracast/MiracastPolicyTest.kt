// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.stream.core.*
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class MiracastPolicyTest {
    private val display = MiracastDisplay(7, "Car", 1920, 1080, 240, 59.94)
    @Test fun pictureOverrideChangesOnlyAttachedCapabilitySizeAndKeepsTheNegotiatedDisplayMetadata() {
        val picture = MiracastPicture(Viewport(1920, 720), stretch = true)
        assertEquals(DisplayInfo(1920, 720, 240, 59.94),
            MiracastPolicy.capabilities(emptyList(), display, picture).displays[ScreenId.MAIN])
        assertTrue(MiracastPolicy.capabilities(emptyList(), null, picture).displays.isEmpty())
        assertEquals(DisplayInfo(1920, 1080, 240, 59.94),
            MiracastPolicy.capabilities(emptyList(), display, MiracastPicture(stretch = true)).displays[ScreenId.MAIN])
        assertEquals(DisplayInfo(720, 1280, 240, 59.94),
            MiracastPolicy.capabilities(emptyList(), display, MiracastPicture(Viewport(720, 1280))).displays[ScreenId.MAIN])
    }
    @Test fun stretchUsesTheWholePhysicalDisplayForEveryPictureAspectRatioWhileFitLetterboxes() {
        for ((width, height) in listOf(640 to 480, 720 to 1280, 1920 to 720, 100 to 100, 0 to 0)) {
            assertEquals(Viewport(1920, 1080), MiracastPolicy.fit(1920, 1080, width, height, stretch = true))
            assertEquals(Viewport(1080, 1920), MiracastPolicy.fit(1080, 1920, width, height, stretch = true))
        }
        assertEquals(Viewport(1920, 720), MiracastPolicy.fit(1920, 1080, 1920, 720, stretch = false))
        assertEquals(Viewport(1440, 1080), MiracastPolicy.fit(1920, 1080, 640, 480, stretch = false))
    }

    @Test fun choiceIgnoresTheDefaultAndInvalidDisplaysAndPreservesPresentationOrder() {
        assertNull(MiracastPolicy.display(emptyList()))
        assertNull(MiracastPolicy.display(listOf(display.copy(id = 0))))
        assertEquals(display, MiracastPolicy.display(listOf(display.copy(id = 0), display.copy(width = 0), display, display.copy(id = 9))))
    }
    @Test fun missingOrInvalidRefreshAndDensityUseDefaultsWithoutSkippingTheFirstDisplay() {
        for (refresh in listOf(0.0, -1.0, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY)) {
            for (density in listOf(0, -1)) {
                val invalid = display.copy(refreshHz = refresh, densityDpi = density)
                val fallback = display.copy(refreshHz = 60.0, densityDpi = 160)
                assertEquals(fallback, MiracastPolicy.display(listOf(invalid, display.copy(id = 9))))
                assertEquals(DisplayInfo(1920, 1080, 160, 60.0), MiracastPolicy.capabilities(emptyList(), invalid).displays[ScreenId.MAIN])
            }
        }
    }
    @Test fun capabilitiesUsePhysicalDisplaySizeAndOfferNoInputOrAudio() {
        val decoders = listOf(DecoderSupport("avc1.64002a", true), DecoderSupport("hvc1.1.6.L153", true))
        val absent = MiracastPolicy.capabilities(decoders, null)
        val attached = MiracastPolicy.capabilities(decoders, display)
        assertTrue(absent.displays.isEmpty())
        assertEquals(decoders, absent.decoders)
        assertEquals(absent.copy(displays = attached.displays), attached)
        assertEquals(DisplayInfo(1920, 1080, 240, 59.94), attached.displays[ScreenId.MAIN])
        assertEquals(LatencyClass.INTERACTIVE, attached.latency)
        assertEquals(0, attached.touch)
        assertTrue(attached.keys.isEmpty())
        assertTrue(attached.audio.isEmpty())
        assertFalse(attached.microphone)
    }
    @Test fun letterboxPreservesAspectRatioForLandscapePortraitAndSquarePictures() {
        assertEquals(Viewport(1440, 1080), MiracastPolicy.fit(1920, 1080, 640, 480))
        assertEquals(Viewport(608, 1080), MiracastPolicy.fit(1920, 1080, 720, 1280))
        assertEquals(Viewport(1080, 608), MiracastPolicy.fit(1080, 1920, 1920, 1080))
        assertEquals(Viewport(720, 720), MiracastPolicy.fit(1280, 720, 100, 100))
        assertEquals(Viewport(1280, 720), MiracastPolicy.fit(1280, 720, 0, 0))
    }
    @Test fun formatComparisonUsesConfigurationContentAndIgnoresRepeatedObjectsAndFpsHints() {
        val first = avcFormat()
        assertTrue(MiracastPolicy.sameFormat(first, avcFormat(avcBytes.copyOf(), 60.0)))
        assertTrue(MiracastPolicy.sameFormat(null, null))
        assertFalse(MiracastPolicy.sameFormat(first, null))
        assertFalse(MiracastPolicy.sameFormat(first, avcFormat(avcBytes.also { it[3] = 31 })))
        val larger = avcFormat(hex("01 64 00 28 ff e1 00 0f 67 64 00 28 ad ff ff c2 20 74 03 c0 11 3f 2a 01 00 04 68 ce 3c 80 fd f8 f8 00"))
        assertEquals(1920 to 1080, larger.width to larger.height)
        assertFalse(MiracastPolicy.sameFormat(first, larger))
        assertFalse(MiracastPolicy.sameFormat(first, VideoFormat.create(ScreenId.MAIN, VideoCodec.H265, hevcBytes)))
    }
}
