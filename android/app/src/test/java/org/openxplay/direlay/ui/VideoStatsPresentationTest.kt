// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.ui

import java.util.Locale
import org.openxplay.direlay.R
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.VideoStatsWindow
import org.openxplay.direlay.video.VideoCompatibility
import org.openxplay.stream.clink.ClinkStatistics
import org.openxplay.stream.core.*

class VideoStatsPresentationTest {
    private fun text(resource: Int, vararg arguments: Any) = VideoStatsText(resource, arguments.toList())
    private val measured = VideoMeasurement.Values(59.8, 8_400_000.0, 0.3, 0, 299, 2)
    private val format = VideoFormat.create(ScreenId.MAIN, VideoCodec.H264, org.openxplay.direlay.bridge.avcConfig)
    private fun snapshot() = VideoStatsSnapshot(VideoStatsWindow.SECONDS_5, measured, format, measured,
        ClinkStatistics(299, 0, 1, 1000, 2, VideoCodec.H264, 1920, 1080), null,
        DisplayInfo(1532, 1013, 160, 60.0), listOf(BrowserVideoStats(measured, 4000, 85000)))
    @Test fun allSectionsAndNumbersAreLocaleIndependent() {
        val before = Locale.getDefault()
        try {
            Locale.setDefault(Locale.GERMANY)
            val result = presentVideoStats(snapshot())
            assertEquals(text(R.string.video_stats_average, "5"), result.caption)
            assertEquals(listOf(text(R.string.video_stats_iphone), text(R.string.video_stats_carlink),
                text(R.string.video_stats_browser, 1)), result.sections.map { it.title })
            assertEquals(listOf(text(R.string.video_stats_input, "H.264", "640 × 480", "59.8", "8.4")), result.sections[0].lines)
            assertEquals(listOf(text(R.string.video_stats_passthrough, "H.264", "1920 × 1080", "59.8", "0.3"),
                text(R.string.video_stats_screen, "1532 × 1013", "60"), text(R.string.video_stats_send_stalls, "2")), result.sections[1].lines)
            assertEquals(listOf(text(R.string.video_stats_browser_values, "59.8", "0.3", "4 ms", "85 ms")), result.sections[2].lines)
        } finally { Locale.setDefault(before) }
    }
    @Test fun warmupPlaceholderTranscodeAndUnavailableValues() {
        val progress = VideoMeasurement.Progress(12)
        val warm = snapshot().copy(window = VideoStatsWindow.SECONDS_15, iphone = progress, carlink = progress,
            browsers = listOf(BrowserVideoStats(progress, null, null)))
        assertEquals(text(R.string.video_stats_measuring, "12"), presentVideoStats(warm).caption)
        assertTrue(presentVideoStats(warm).sections.all { it.lines.first() == text(R.string.video_stats_measuring, "12") })
        val placeholder = snapshot().copy(carlink = measured.copy(frames = 0, placeholders = 10, sendStalls = 0))
        assertEquals(text(R.string.video_stats_placeholder), presentVideoStats(placeholder).sections[1].lines.first())
        assertEquals(2, presentVideoStats(placeholder).sections[1].lines.size)
        val target = VideoCompatibility.Target(VideoCodec.H264, 66, 40, "avc1.420028")
        val transcode = snapshot().copy(decision = CarlinkVideoDecision(format, VideoCompatibility.Decision(VideoCompatibility.Mode.TRANSCODE, target)))
        assertEquals(text(R.string.video_stats_transcode, "H.264", "H.264 Baseline", "1920 × 1080", "59.8", "0.3"),
            presentVideoStats(transcode).sections[1].lines.first())
        val missing = snapshot().copy(format = null, carlink = null, browsers = listOf(BrowserVideoStats(measured, null, null)))
        assertEquals(text(R.string.video_stats_input, "—", "—", "59.8", "8.4"), presentVideoStats(missing).sections[0].lines.first())
        assertEquals(text(R.string.video_stats_browser_values, "59.8", "0.3", "—", "—"), presentVideoStats(missing).sections[1].lines.first())
        assertEquals(text(R.string.video_stats_average, "15"), presentVideoStats(snapshot().copy(window = VideoStatsWindow.SECONDS_15)).caption)
    }
    @Test fun hevcAndPlaceholderAfterLiveVideoUseTheLatestState() {
        val hevc = VideoFormat.create(ScreenId.MAIN, VideoCodec.H265, org.openxplay.direlay.bridge.hevcConfig)
        val value = snapshot().copy(format = hevc,
            clink = snapshot().clink!!.copy(codec = VideoCodec.H265), placeholderActive = true)
        assertEquals(text(R.string.video_stats_input, "H.265", "${hevc.width} × ${hevc.height}", "59.8", "8.4"),
            presentVideoStats(value).sections[0].lines.first())
        assertEquals(text(R.string.video_stats_placeholder), presentVideoStats(value).sections[1].lines.first())
        assertEquals(text(R.string.video_stats_passthrough, "H.265", "1920 × 1080", "59.8", "0.3"),
            presentVideoStats(value.copy(placeholderActive = false)).sections[1].lines.first())
    }
    @Test fun browserResourcesAndClientLimit() {
        val result = presentVideoStats(snapshot().copy(browsers = List(8) { BrowserVideoStats(measured, null, null) }))
        assertEquals(text(R.string.video_stats_average, "5"), result.caption)
        assertEquals(text(R.string.video_stats_passthrough, "H.264", "1920 × 1080", "59.8", "0.3"),
            result.sections[1].lines.first())
        result.sections.drop(2).forEachIndexed { index, section ->
            assertEquals(text(R.string.video_stats_browser, index + 1), section.title)
            assertEquals(listOf(text(R.string.video_stats_browser_values, "59.8", "0.3", "—", "—")), section.lines)
        }
        assertEquals(6, result.sections.size)
    }
}
