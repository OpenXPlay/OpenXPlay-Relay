// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.*

class CarlinkPicturePolicyTest {
    private val settings = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        autoStartOnBoot = false, autoStartWhenReady = false, localPreview = false, webPort = null,
        outputs = setOf(RelayOutput.CARLINK), carlinkPictureDefault = PictureSize(1920, 720))
    private val input = InputCapabilities(setOf(VideoCodec.H264), setOf(ScreenId.MAIN), false, emptySet(), false, true, ReconfigureCost.NONE)
    private fun caps(width: Int, height: Int) = OutputCapabilities(LatencyClass.INTERACTIVE,
        listOf(DecoderSupport("avc1.64001f", true)), mapOf(ScreenId.MAIN to DisplayInfo(width, height, 160, 60.0)),
        0, emptySet(), emptySet(), false)

    @Test fun trialOverridesDefaultIncludingExplicitAutomatic() {
        assertEquals(PictureSize(1920, 720), settings.effectiveCarlinkPicture(null))
        assertEquals(PictureSize(1024, 768), settings.effectiveCarlinkPicture(CarlinkPictureTrial(PictureSize(1024, 768))))
        assertNull(settings.effectiveCarlinkPicture(CarlinkPictureTrial(null)))
        assertNull(carlinkPictureScreen(settings, CarlinkPictureTrial(null), primary = true))
    }
    @Test fun startAndCarlinkPrimaryUseTheDecoderLimitedCanvasAheadOfResolutionChoices() {
        for (resolution in CarPlayResolution.entries) {
            val current = settings.copy(carplayResolution = resolution)
            val expected = CarPlayScreen(CarDisplayFit.fit(1920, 720, 3600), "carlink picture")
            assertEquals(expected, carlinkPictureScreen(current, null, primary = false, starting = true, decoderLimit = 3600))
            assertEquals(expected, carlinkPictureScreen(current, null, primary = true, decoderLimit = 3600))
            assertTrue(CarDisplayFit.macroblocks(expected.profile.width, expected.profile.height) <= 3600)
        }
        assertEquals(DisplayProfile(1920, 720, 240), carlinkPictureScreen(settings, null, primary = false, starting = true)?.profile)
    }
    @Test fun fixedWinsBrowserPrimaryIgnoresAndAutomaticIsUnchanged() {
        assertNull(carlinkPictureScreen(settings.copy(displayMode = DisplayMode.FIXED, display = CarDisplayFit.default), null, true, true))
        assertNotNull(carlinkPictureScreen(settings.copy(displayMode = DisplayMode.RELAY_SCREEN), null, true))
        val both = settings.copy(outputs = setOf(RelayOutput.BROWSER, RelayOutput.CARLINK))
        assertNull(carlinkPictureScreen(both, null, primary = false, starting = true))
        assertNull(carlinkPictureScreen(both, null, primary = false))
        assertNotNull(carlinkPictureScreen(both, null, primary = true))
        assertNull(carlinkPictureScreen(settings.copy(outputs = setOf(RelayOutput.BROWSER)), null, true, true))
        assertNull(carlinkPictureScreen(settings.copy(carlinkPictureDefault = null), null, true, true))
    }
    @Test fun reportsKeepAutomaticMemoryWhileCustomCanvasIgnoresTheirGeometry() {
        val policy = CarDisplayPolicy()
        val browser = caps(1024, 768)
        var frozen: DisplayProfile? = null
        for (carlink in listOf(caps(1920, 1080), caps(1280, 800), caps(1024, 600))) {
            policy.derive(input, listOf(carlink))
            assertSame(carlink, policy.primaryOutput)
            val custom = carlinkPictureScreen(settings, null, policy.primaryOutput === carlink, decoderLimit = policy.decoderLimit)
            if (frozen == null) frozen = custom?.profile
            assertEquals(frozen, custom?.profile)
            assertEquals(fitDisplay(checkNotNull(policy.primaryDisplay), settings.carplayResolution, 3600), policy.carDisplay)
            assertEquals(policy.carDisplay, policy.lastCarDisplay)
            policy.derive(input, listOf(browser, carlink))
            assertSame(browser, policy.primaryOutput)
            assertNull(carlinkPictureScreen(settings, null, policy.primaryOutput === carlink))
        }
    }
    @Test fun refitIgnoresReportsAndPendingDefaultsButStillAppliesPhysicalWidth() {
        val applied = DisplayTarget(CarDisplayFit.fit(1920, 720, 3600), CarPlayResolution.AUTO,
            CarPlayScreenWidth.MM200, ScreenDataKey(1920, 1080, 160))
        val report = DisplayTarget(CarDisplayFit.default, CarPlayResolution.P720,
            CarPlayScreenWidth.MM200, ScreenDataKey(1280, 800, 240))
        val desired = carlinkPictureRefitTarget(applied, report, CarPlayScreenWidth.MM200, true)
        assertEquals(applied, desired)
        val refit = DisplayRefit()
        assertFalse(refit.due(0, true, DisplayMode.AUTOMATIC, applied, desired, true))
        assertFalse(refit.due(2000, true, DisplayMode.AUTOMATIC, applied, desired, true))
        val wider = carlinkPictureRefitTarget(applied, report, CarPlayScreenWidth.MM250, true)
        assertFalse(refit.due(3000, true, DisplayMode.AUTOMATIC, applied, wider, false))
        assertTrue(refit.due(4000, true, DisplayMode.AUTOMATIC, applied, wider, false))
        assertEquals(report, carlinkPictureRefitTarget(applied, report, CarPlayScreenWidth.MM200, false))
    }
    @Test fun runStopAndProcessRecreationClearTrialsButDeliberateRestartsPreserveThem() {
        val storage = object : AutoStartStorage {
            private val values = mutableMapOf<String, String>()
            override fun read(key: String) = values[key]
            override fun write(key: String, value: String) { values[key] = value }
        }
        val selection = CarlinkPictureSelection()
        val tracker = SessionStopTracker(BootIdentitySource { "test-boot" }, storage, selection::stopped)
        for (size in listOf(PictureSize(1024, 768), null)) {
            val trial = CarlinkPictureTrial(size)
            selection.choose(trial)
            tracker.observe(true); tracker.restarting(); tracker.observe(false)
            assertEquals(trial, selection.trial.value)
            tracker.observe(true); tracker.observe(false)
            assertNull(selection.trial.value)
            assertEquals(settings.carlinkPictureDefault, settings.effectiveCarlinkPicture(selection.trial.value))
            selection.choose(trial)
            assertNull(CarlinkPictureSelection().trial.value)
            selection.stopped()
            assertNull(selection.trial.value)
        }
    }
}
