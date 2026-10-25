// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.Viewport

class WirelessPicturePolicyTest {
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null, 30, false, false, false, null)
    private fun saved(width: Int, height: Int) = SavedHeadUnit(DisplayProfile(width, height, 160),
        HeadUnitDisplay(ScreenDataKey(width, height, 160), null, true, true))
    private fun year(width: Int, height: Int) = CarYear("car", mapOf("en" to "Car"),
        listOf(CarScreenData(ScreenDataKey(width, height, 160), emptyList())))

    @Test fun autoUsesSavedBrowserThenSelectedModelThenSixteenByNineWithoutAFixedOrLiveSize() {
        val fixed = base.copy(displayMode = DisplayMode.FIXED, display = DisplayProfile(800, 480, 160))
        for (settings in listOf(base, fixed, base.copy(displayMode = DisplayMode.RELAY_SCREEN))) {
            assertEquals(WirelessPictureResolution(WirelessPictureSize(1920, 720), WirelessPictureSource.BROWSER),
                WirelessPicturePolicy.resolve(settings, saved(1920, 720), year(1280, 720)))
            assertEquals(WirelessPictureResolution(WirelessPictureSize(1920, 720), WirelessPictureSource.CAR_MODEL),
                WirelessPicturePolicy.resolve(settings, null, year(1280, 480)))
            assertEquals(WirelessPictureResolution(WirelessPictureSize(1280, 720), WirelessPictureSource.DEFAULT),
                WirelessPicturePolicy.resolve(settings))
        }
        val legacy = SavedHeadUnit(DisplayProfile(1920, 720, 160), null)
        assertEquals(WirelessPictureSource.CAR_MODEL, WirelessPicturePolicy.resolve(base, legacy, year(1280, 720)).source)
        val declaration = WirelessPicturePolicy.resolve(fixed, saved(1920, 720)).size
        assertEquals(fixed.display, startupCarPlayScreen(fixed, DisplayProfile(declaration.width, declaration.height, 160), null, null)?.profile)
    }

    @Test fun classesScaleTheShortSideAndFitBoundsForWidePortraitAndOddScreens() {
        val cases = listOf(
            Triple(1280 to 720, WirelessPictureSize(1280, 720), WirelessPictureSize(1920, 1080)),
            Triple(1600 to 960, WirelessPictureSize(1200, 720), WirelessPictureSize(1800, 1080)),
            Triple(720 to 1280, WirelessPictureSize(720, 1280), WirelessPictureSize(1080, 1920)),
            Triple(1920 to 720, WirelessPictureSize(1920, 720), WirelessPictureSize(1920, 720)))
        for ((screen, p720, p1080) in cases) {
            assertEquals(p720, WirelessPicturePolicy.resolve(base, saved(screen.first, screen.second)).size)
            assertEquals(p1080, WirelessPicturePolicy.resolve(base.copy(dlnaPicture = DlnaPicture.P1080), saved(screen.first, screen.second)).size)
        }
        for (picture in DlnaPicture.entries) {
            val size = WirelessPicturePolicy.resolve(base.copy(dlnaPicture = picture), null, year(2401, 1201)).size
            size.validate()
            assertEquals(2401.0 / 1201, size.width.toDouble() / size.height, .005)
        }
    }

    @Test fun customIgnoresSavedScreensModelsAndClassAndFitsBeforeDeclaring() {
        for (picture in DlnaPicture.entries) {
            val settings = base.copy(wirelessPicture = WirelessPictureMode.CUSTOM, dlnaPicture = picture,
                wirelessPictureSize = WirelessPictureSize(1280, 480))
            assertEquals(WirelessPictureResolution(WirelessPictureSize(1280, 480), WirelessPictureSource.CUSTOM),
                WirelessPicturePolicy.resolve(settings, saved(1920, 1080), year(720, 1280)))
            assertEquals(WirelessPictureSize(1920, 720), WirelessPicturePolicy.resolve(
                settings.copy(wirelessPictureSize = WirelessPictureSize(3840, 1440))).size)
        }
    }

    @Test fun customStartsFromCurrentAutoAndThenKeepsEditsUntilAutoIsSelectedAgain() {
        val auto = WirelessPicturePolicy.resolve(base, saved(1920, 720))
        val both = base.copy(dlnaOutput = true, miracastOutput = true)
        assertEquals(auto.size, WirelessPicturePolicy.customStart(both, auto, Viewport(1280, 720)))
        assertEquals(WirelessPictureSize(1080, 1920), WirelessPicturePolicy.customStart(
            base.copy(miracastOutput = true), auto, Viewport(1080, 1920)))
        assertEquals(auto.size, WirelessPicturePolicy.customStart(base.copy(miracastOutput = true), auto, null))
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
