// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.openxplay.stream.core.FrameRatePreference
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.*

class WirelessPicturePolicyTest {
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null, FrameRatePreference.AUTO, false, false, false, null)
    private fun saved(width: Int, height: Int) = SavedHeadUnit(DisplayProfile(width, height, 160),
        HeadUnitDisplay(ScreenDataKey(width, height, 160), null, true, true))
    private fun year(width: Int, height: Int) = CarYear("car", mapOf("en" to "Car"),
        listOf(CarScreenData(ScreenDataKey(width, height, 160), emptyList())))

    private val wireless = base.copy(outputs = setOf(RelayOutput.DLNA))
    private val input = InputCapabilities(setOf(VideoCodec.H264), setOf(ScreenId.MAIN), false, emptySet(), false, true, ReconfigureCost.NONE)
    private fun capabilities(display: DisplayInfo, codec: String = "avc1.64002a") = OutputCapabilities(
        LatencyClass.BUFFERED, listOf(DecoderSupport(codec, true)), mapOf(ScreenId.MAIN to display), 0, emptySet(), emptySet(), false)

    @Test fun followDeclaresTheMeasuredWholeScreenThenModelThen1080DefaultWithoutRefitting() {
        val measured = saved(1920, 1080).copy(canvas = DisplayProfile(1280, 720, 160),
            display = HeadUnitDisplay(ScreenDataKey(2560, 1440, 320), ViewAreaSize(1280, 600), false, false),
            resolution = CarPlayResolution.P720, maxMacroblocks = 3600)
        for (resolution in CarPlayResolution.entries) {
            val settings = wireless.copy(carplayResolution = resolution)
            assertEquals(WirelessPictureResolution(WirelessPictureSize(2560, 1440), WirelessPictureSource.BROWSER, 320),
                WirelessPicturePolicy.resolve(settings, measured, year(1280, 720)))
            assertEquals(WirelessPictureResolution(WirelessPictureSize(2401, 1201), WirelessPictureSource.CAR_MODEL),
                WirelessPicturePolicy.resolve(settings, null, year(2401, 1201)))
            assertEquals(WirelessPictureResolution(WirelessPictureSize(1920, 1080), WirelessPictureSource.DEFAULT, 240),
                WirelessPicturePolicy.resolve(settings))
        }
        val legacy = SavedHeadUnit(DisplayProfile(1920, 720, 160), null)
        assertEquals(WirelessPictureSource.CAR_MODEL, WirelessPicturePolicy.resolve(wireless, legacy, year(1280, 720)).source)
        assertEquals(WirelessPictureSource.DEFAULT, WirelessPicturePolicy.resolve(wireless, legacy).source)
        for (mode in DisplayMode.entries) assertEquals(WirelessPicturePolicy.resolve(wireless, measured),
            WirelessPicturePolicy.resolve(wireless.copy(displayMode = mode, display = DisplayProfile(800, 480, 160)), measured))
    }

    @Test fun followFitsLikeTheBrowserForEveryResolutionIncludingTheDecoderLimit() {
        for (screen in listOf(1920 to 720, 1920 to 1080, 2560 to 1440))
            for (resolution in CarPlayResolution.entries) for (limit in listOf(null, 3600)) {
                val settings = wireless.copy(carplayResolution = resolution)
                val picture = WirelessPicturePolicy.resolve(settings, saved(screen.first, screen.second))
                val browser = DisplayInfo(screen.first, screen.second, 160, 60.0)
                assertEquals(fitDisplay(browser, resolution, limit, wholeScreen = true), WirelessPicturePolicy.canvas(settings, picture, limit))
                val policy = CarDisplayPolicy(resolution = { WirelessPicturePolicy.resolution(settings) })
                policy.derive(input, listOf(capabilities(picture.display(), if (limit == null) "unknown" else "avc1.64001f")))
                assertEquals(WirelessPicturePolicy.canvas(settings, picture, limit), policy.carDisplay)
            }
        for (resolution in CarPlayResolution.entries) {
            assertEquals(DisplayProfile(1920, 720, 240), WirelessPicturePolicy.canvas(wireless.copy(carplayResolution = resolution),
                WirelessPicturePolicy.resolve(wireless, saved(1920, 720))))
            assertEquals(if (resolution == CarPlayResolution.P720) DisplayProfile(1280, 720, 160) else DisplayProfile(1920, 1080, 240),
                WirelessPicturePolicy.canvas(wireless.copy(carplayResolution = resolution), WirelessPicturePolicy.resolve(wireless)))
        }
    }

    @Test fun allFifteenOutputSetsKeepInteractiveResolutionAndDeclarationsWhileWirelessCustomIgnoresTheCap() {
        for (mask in 1..15) {
            val outputs = RelayOutput.entries.filterIndexed { index, _ -> mask and (1 shl index) != 0 }.toSet()
            val onlyWireless = RelayOutput.BROWSER !in outputs && RelayOutput.CARLINK !in outputs
            for (mode in WirelessPictureMode.entries) for (resolution in CarPlayResolution.entries) {
                val settings = base.copy(outputs = outputs, wirelessPicture = mode, carplayResolution = resolution,
                    wirelessPictureSize = WirelessPictureSize(1920, 1080))
                assertEquals(onlyWireless && mode == WirelessPictureMode.CUSTOM, WirelessPicturePolicy.customCanvas(settings))
                val effective = if (onlyWireless && mode == WirelessPictureMode.CUSTOM) CarPlayResolution.AUTO else resolution
                assertEquals(effective, WirelessPicturePolicy.resolution(settings))
                val display = DisplayInfo(1920, 1080, 240, 60.0, viewport = Viewport(1600, 900))
                val policy = CarDisplayPolicy(resolution = { WirelessPicturePolicy.resolution(settings) })
                policy.derive(input, outputs.map { capabilities(display.copy(viewport = if (onlyWireless) null else display.viewport)) })
                assertEquals(fitDisplay(display, effective, wholeScreen = onlyWireless), policy.carDisplay)
                if (!onlyWireless) assertEquals(startupCarPlayScreen(settings, policy.carDisplay, null, null),
                    WirelessPicturePolicy.startupScreen(settings, policy.carDisplay, null, null, null))
                val declaration = WirelessPicturePolicy.resolve(settings)
                assertEquals(if (onlyWireless || mode == WirelessPictureMode.CUSTOM) WirelessPictureSize(1920, 1080)
                    else WirelessPictureSize(1280, 720), declaration.size)
            }
        }
        assertFalse(WirelessPicturePolicy.customCanvas(base.copy(outputs = emptySet(), wirelessPicture = WirelessPictureMode.CUSTOM)))
    }

    @Test fun customDeclaresExactValidPixelsAndKeepsDecoderProtection() {
        for (resolution in CarPlayResolution.entries) {
            val settings = wireless.copy(wirelessPicture = WirelessPictureMode.CUSTOM, carplayResolution = resolution,
                wirelessPictureSize = WirelessPictureSize(1920, 1080))
            val picture = WirelessPicturePolicy.resolve(settings, saved(1280, 480), year(720, 1280))
            assertEquals(WirelessPictureResolution(WirelessPictureSize(1920, 1080), WirelessPictureSource.CUSTOM), picture)
            assertEquals(DisplayProfile(1920, 1080, 240), WirelessPicturePolicy.canvas(settings, picture))
            assertEquals(CarDisplayFit.fit(1920, 1080, 3600), WirelessPicturePolicy.canvas(settings, picture, 3600))
            assertEquals(WirelessPictureSize(1920, 720), WirelessPicturePolicy.resolve(
                settings.copy(wirelessPictureSize = WirelessPictureSize(3840, 1440))).size)
        }
    }

    @Test fun startupWithoutALiveDeclarationUsesTheSameWirelessCanvasAndRetainsExplicitDisplayModes() {
        for (resolution in CarPlayResolution.entries) {
            val settings = wireless.copy(carplayResolution = resolution)
            assertEquals(CarPlayScreen(checkNotNull(WirelessPicturePolicy.canvas(settings, WirelessPicturePolicy.resolve(settings))), "default"),
                WirelessPicturePolicy.startupScreen(settings, null, null, null, null))
            assertEquals("remembered", WirelessPicturePolicy.startupScreen(settings, null, saved(1920, 720), null, null)?.source)
            assertEquals("built-in model", WirelessPicturePolicy.startupScreen(settings, null, null, year(1920, 720), null)?.source)
            assertEquals(CarPlayScreen(DisplayProfile(800, 480, 160), "fixed"), WirelessPicturePolicy.startupScreen(
                settings.copy(displayMode = DisplayMode.FIXED, display = DisplayProfile(800, 480, 160)), null, null, null, null))
            assertNull(WirelessPicturePolicy.startupScreen(settings.copy(displayMode = DisplayMode.RELAY_SCREEN), null, null, null, null))
            val miracast = settings.copy(outputs = setOf(RelayOutput.MIRACAST))
            assertEquals(startupCarPlayScreen(miracast, null, saved(1280, 720), null),
                WirelessPicturePolicy.startupScreen(miracast, null, saved(1280, 720), null, null))
        }
    }

    @Test fun customStartsFromCurrentAutoAndThenKeepsEditsUntilAutoIsSelectedAgain() {
        val auto = WirelessPicturePolicy.resolve(wireless, saved(1920, 720))
        val both = base.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.DLNA, RelayOutput.MIRACAST))
        assertEquals(auto.size, WirelessPicturePolicy.customStart(both, auto, Viewport(1280, 720)))
        assertEquals(WirelessPictureSize(1080, 1920), WirelessPicturePolicy.customStart(
            base.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.MIRACAST)), auto, Viewport(1080, 1920)))
        assertEquals(auto.size, WirelessPicturePolicy.customStart(base.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.MIRACAST)), auto, null))
        assertEquals(WirelessPictureSize(1920, 1080), WirelessPicturePolicy.customStart(wireless,
            WirelessPicturePolicy.resolve(wireless, saved(3840, 2160)), null))
        val custom = WirelessPicturePolicy.choose(base, WirelessPictureMode.CUSTOM, auto.size)
        assertEquals(auto.size, custom.wirelessPictureSize)
        val edited = custom.copy(wirelessPictureSize = WirelessPictureSize(1024, 600))
        assertEquals(edited, WirelessPicturePolicy.choose(edited, WirelessPictureMode.CUSTOM, auto.size))
        assertEquals(edited.copy(wirelessPicture = WirelessPictureMode.AUTO),
            WirelessPicturePolicy.choose(edited, WirelessPictureMode.AUTO, auto.size))
    }

    @Test fun customValidationUsesEvenLandscapeBoundsAndTheirPortraitMirror() {
        for (size in listOf(WirelessPictureSize(640, 360), WirelessPictureSize(1920, 1080), WirelessPictureSize(1920, 360),
            WirelessPictureSize(360, 640), WirelessPictureSize(1080, 1920), WirelessPictureSize(360, 1920), WirelessPictureSize(1024, 600)))
            size.validate()
        for (size in listOf(WirelessPictureSize(639, 360), WirelessPictureSize(640, 359), WirelessPictureSize(1922, 1080),
            WirelessPictureSize(1920, 1082), WirelessPictureSize(360, 1922), WirelessPictureSize(1281, 720),
            WirelessPictureSize(1280, 721), WirelessPictureSize(0, 0), WirelessPictureSize(-640, 360)))
            assertThrows(IllegalArgumentException::class.java) { size.validate() }
    }
}
