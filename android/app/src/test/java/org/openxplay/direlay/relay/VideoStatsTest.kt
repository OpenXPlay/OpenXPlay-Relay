// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.VideoStatsWindow.*
import org.openxplay.stream.core.*

class VideoStatsTest {
    private var now = 0L
    private val sampler = VideoStatsSampler { now }
    private fun sample(second: Double, counters: VideoCounters, session: Long = 1) {
        now = (second * 1e9).toLong(); sampler.sample(SessionId(session), SECONDS_5, counters)
    }
    @Test fun warmupAndWindowMathUseActualElapsedTimeAndCounterDeltas() {
        sample(0.0, VideoCounters(100, 1000, 5, 7, 2, 10))
        assertEquals(VideoMeasurement.Progress(5), sampler.report(SECONDS_5))
        sample(0.5, VideoCounters(110, 2000, 5, 7, 2, 10))
        assertEquals(1, sampler.retainedSamples)
        sample(2.0, VideoCounters(140, 5000, 6, 7, 2, 11))
        assertEquals(VideoMeasurement.Progress(3), sampler.report(SECONDS_5))
        sample(5.0, VideoCounters(200, 11000, 10, 9, 5, 15))
        val report = sampler.report(SECONDS_5) as VideoMeasurement.Values
        assertEquals(20.0, report.fps, 0.0001)
        assertEquals(16000.0, report.bitsPerSecond, 0.0001)
        assertEquals(1000.0 / 110, report.droppedPercent, 0.0001)
        assertEquals(2L, report.placeholders); assertEquals(3L, report.sendStalls)
        sample(7.0, VideoCounters(210, 12000, 10, 9, 5, 15))
        assertEquals(14.0, (sampler.report(SECONDS_5) as VideoMeasurement.Values).fps, 0.0001)
    }
    @Test fun idleVariableRateIsZeroAndNeverDividesByZero() {
        sample(0.0, VideoCounters(0)); sample(5.0, VideoCounters(0))
        assertEquals(VideoMeasurement.Values(0.0, 0.0, 0.0, 0, 0, 0), sampler.report(SECONDS_5))
    }
    @Test fun sessionCounterAndWindowChangesRestartProgress() {
        val full = VideoCounters(100, 100, 100, 100, 100, 100)
        val backwards = listOf(full.copy(frames = 99), full.copy(bytes = 99), full.copy(dropped = 99),
            full.copy(placeholders = 99), full.copy(stalls = 99), full.copy(relayDropped = 99))
        for (counter in backwards) {
            sampler.clear(); sample(0.0, full); sample(5.0, full); sample(6.0, counter)
            assertEquals(VideoMeasurement.Progress(5), sampler.report(SECONDS_5))
        }
        sample(7.0, full, session = 2)
        assertEquals(VideoMeasurement.Progress(5), sampler.report(SECONDS_5))
        sampler.sample(SessionId(2), SECONDS_15, full)
        assertEquals(VideoMeasurement.Progress(15), sampler.report(SECONDS_15))
        assertEquals(1, sampler.retainedSamples)
    }
    @Test fun everySeriesIsBoundedAndOffAndVisibilityDiscardHistory() {
        repeat(1000) {
            now = it * 1_000_000_000L
            sampler.sample(SessionId(1), SECONDS_15, VideoCounters(it.toLong()))
            assertTrue(sampler.retainedSamples <= 16)
        }
        assertEquals(1.0, (sampler.report(SECONDS_15) as VideoMeasurement.Values).fps, 0.0)
        sampler.sample(SessionId(1), OFF, VideoCounters(2000))
        assertEquals(0, sampler.retainedSamples)
        sampler.clear(); sample(1001.0, VideoCounters(2000))
        assertEquals(VideoMeasurement.Progress(5), sampler.report(SECONDS_5))
    }
}
