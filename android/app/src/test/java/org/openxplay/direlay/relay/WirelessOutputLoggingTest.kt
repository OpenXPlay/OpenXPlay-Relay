// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.DisplayInfo
import org.openxplay.stream.core.FrameRatePreference
import com.shilapi.xcertplay.orchestration.MfiTarget

class WirelessOutputLoggingTest {
    @Test fun canvasLineRecordsTheFrozenSizeDeclarationAndRequestedControls() {
        val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null, FrameRatePreference.AUTO, false, false, false, null,
            outputs = setOf(RelayOutput.DLNA))
        for ((resolution, wire) in listOf(CarPlayResolution.AUTO to "auto", CarPlayResolution.NATIVE to "native",
            CarPlayResolution.P1080 to "1080p", CarPlayResolution.P720 to "720p", CarPlayResolution.SCREEN to "screen"))
            for (mode in WirelessPictureMode.entries) for (source in listOf("car screen", "remembered", "built-in model", "default", "fixed", "carlink picture")) {
                val settings = base.copy(carplayResolution = resolution, wirelessPicture = mode)
                val screen = CarPlayScreen(DisplayProfile(1920, 720, 240), source)
                val picture = if (mode == WirelessPictureMode.AUTO) "auto" else "custom"
                val label = if (source == "carlink picture") "fixed" else source
                assertEquals("canvas 1920x720 source=$label declared=2560x1440 resolution=$wire picture=$picture",
                    wirelessLogLine(wirelessCanvasLogLine(screen, DisplayInfo(2560, 1440, 240, 30.0), settings)))
                assertEquals("canvas 1920x720 source=$label declared=- resolution=$wire picture=$picture",
                    wirelessLogLine(wirelessCanvasLogLine(screen, null, settings)))
            }
    }
    @Test fun statisticsLogImmediatelyThenAtMostEveryTenSecondsWithoutSlidingTheDeadline() {
        val limiter = WirelessStatisticsLogLimiter()
        assertTrue(limiter.allow(0))
        assertFalse(limiter.allow(0))
        assertFalse(limiter.allow(1_000_000_000))
        assertFalse(limiter.allow(9_999_999_999))
        assertTrue(limiter.allow(10_000_000_000))
        assertFalse(limiter.allow(19_999_999_999))
        assertTrue(limiter.allow(20_000_000_000))
        assertTrue(limiter.allow(60_000_000_000))
    }
    @Test fun outputsHaveIndependentLimitsAndMonotonicTimeCanStartNegativeOrWrap() {
        val dlna = WirelessStatisticsLogLimiter()
        val miracast = WirelessStatisticsLogLimiter()
        assertTrue(dlna.allow(-1))
        assertTrue(miracast.allow(-1))
        assertFalse(dlna.allow(0))
        assertTrue(dlna.allow(9_999_999_999))
        val wrapped = WirelessStatisticsLogLimiter()
        assertTrue(wrapped.allow(Long.MAX_VALUE - 1_000_000_000))
        assertFalse(wrapped.allow(Long.MIN_VALUE))
        assertTrue(wrapped.allow(Long.MIN_VALUE + 9_000_000_000))
    }
    @Test fun wirelessLinesRedactUrlsAndKeepMultilineFailuresOnOneLine() {
        val failure = "failure decoder_failed: token=test-secret\r\n" +
            "stream http://192.0.2.1:1234/stream/private-token.ts\n" +
            "source HTTPS://example.test/video?token=another-token\rtail"
        val line = wirelessLogLine(redactFailureText(failure))
        assertEquals("failure decoder_failed: token=[redacted] stream [redacted-url] source [redacted-url] tail", line)
        assertEquals("displays none", wirelessLogLine("displays none"))
    }
}
