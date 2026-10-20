// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.relay

import android.app.Application
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.direlay.settings.VideoStatsWindow
import org.openxplay.stream.clink.ClinkStatistics
import org.openxplay.stream.core.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, application = Application::class)
class VideoStatsIntegrationTest {
    @Test fun pullCombinesMatchingReportsAndResetsForReloadWindowAndVisibility() {
        var now = 0L
        val status = RelayStatus { now }
        val stats = VideoStats { now }
        val session = SessionId(1)
        status.onSession(SessionState(session, SessionPhase.ACTIVE), SessionConfig(emptyMap()))
        fun web(rendered: Long, dropped: Long, relay: Long) {
            status.stats("web", OutputStats(Origin("web", "one"), ScreenId.MAIN, 4000, 0, dropped, rendered, 85000))
            status.receivers.report("one", "stats", """{"rendered":$rendered,"dropped":$dropped,"relayDroppedFrames":$relay}""")
        }
        var clink = ClinkStatistics(0, 10, 0, 0, 0, null, 0, 0)
        fun pull(window: VideoStatsWindow = VideoStatsWindow.SECONDS_5) = stats.sample(status, window, true, clink, null, null)
        web(100, 10, 2)
        assertNull(pull(VideoStatsWindow.OFF)); assertFalse(status.countVideoBytes)
        assertEquals(VideoMeasurement.Progress(5), pull()!!.iphone)
        assertTrue(status.countVideoBytes)
        repeat(50) { status.onVideoFrame(VideoFrame(ScreenId.MAIN, session, ByteArray(1000), 0, true, 0)) }
        now = 5_000_000_000L
        web(390, 11, 3)
        clink = clink.copy(placeholderFrames = 160)
        val measured = pull()!!
        assertEquals(10.0, (measured.iphone as VideoMeasurement.Values).fps, 0.0)
        assertEquals(80000.0, measured.iphone.bitsPerSecond, 0.0)
        assertTrue(measured.placeholderActive)
        val browser = measured.browsers.single()
        assertEquals(58.0, (browser.measurement as VideoMeasurement.Values).fps, 0.0)
        assertEquals(200.0 / 292, browser.measurement.droppedPercent, 0.0001)
        assertEquals(4000L, browser.decodeUs); assertEquals(85000L, browser.latencyUs)
        // A separately delivered diagnostics callback must not be combined with another typed report.
        status.receivers.report("one", "stats", """{"rendered":999,"dropped":11,"relayDroppedFrames":1}""")
        assertNull(status.receivers.relayDroppedFrames(status.webMeasurements().single()))
        now += 1_000_000_000L
        assertTrue(pull()!!.browsers.single().measurement is VideoMeasurement.Values)
        web(0, 0, 3)
        assertEquals(VideoMeasurement.Progress(5), pull()!!.browsers.single().measurement)
        assertEquals(VideoMeasurement.Progress(15), pull(VideoStatsWindow.SECONDS_15)!!.iphone)
        stats.clear()
        assertEquals(VideoMeasurement.Progress(5), pull()!!.iphone)
        assertNull(pull(VideoStatsWindow.OFF)); assertFalse(status.countVideoBytes)
        now += 20_000_000_000L
        assertTrue(pull()!!.browsers.isEmpty())
        assertEquals(VideoMeasurement.Progress(5), pull()!!.iphone)
        status.onSession(SessionState(SessionId(2), SessionPhase.ACTIVE), SessionConfig(emptyMap()))
        assertEquals(VideoMeasurement.Progress(5), pull()!!.iphone)
    }
}
