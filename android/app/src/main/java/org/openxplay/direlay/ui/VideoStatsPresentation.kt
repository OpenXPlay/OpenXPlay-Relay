// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.ui

import java.util.Locale
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.video.VideoCompatibility
import org.openxplay.stream.core.VideoCodec

data class VideoStatsText(val resource: Int, val arguments: List<Any> = emptyList())
data class VideoStatsSection(val title: VideoStatsText, val lines: List<VideoStatsText>)
data class VideoStatsPresentation(val caption: VideoStatsText, val sections: List<VideoStatsSection>)

/** Numbers remain identical across locales, including decimal separators. */
fun presentVideoStats(value: VideoStatsSnapshot): VideoStatsPresentation {
    fun decimal(number: Double) = String.format(Locale.ROOT, "%.1f", number)
    fun codec(codec: VideoCodec?) = when (codec) { VideoCodec.H264 -> "H.264"; VideoCodec.H265 -> "H.265"; null -> "—" }
    fun size(width: Int?, height: Int?) = if (width != null && height != null && width > 0 && height > 0) "$width × $height" else "—"
    fun progress(value: VideoMeasurement.Progress) = VideoStatsText(R.string.video_stats_measuring, listOf(value.seconds.toString()))
    fun ms(value: Long?) = value?.let { String.format(Locale.ROOT, "%.0f ms", it / 1000.0) } ?: "—"
    val sections = mutableListOf<VideoStatsSection>()
    val input = when (val report = value.iphone) {
        is VideoMeasurement.Progress -> progress(report)
        is VideoMeasurement.Values -> VideoStatsText(R.string.video_stats_input, listOf(codec(value.format?.codec),
            size(value.format?.width, value.format?.height), decimal(report.fps), decimal(report.bitsPerSecond / 1e6)))
    }
    sections += VideoStatsSection(VideoStatsText(R.string.video_stats_iphone), listOf(input))
    value.carlink?.let { report ->
        val lines = mutableListOf<VideoStatsText>()
        lines += when {
            report is VideoMeasurement.Progress -> progress(report)
            report is VideoMeasurement.Values && (value.placeholderActive || report.placeholders > 0 && report.frames == 0L) ->
                VideoStatsText(R.string.video_stats_placeholder)
            else -> {
                report as VideoMeasurement.Values
                val decision = value.decision
                val details = listOf(size(value.clink?.width, value.clink?.height), decimal(report.fps), decimal(report.droppedPercent))
                if (decision?.decision?.mode == VideoCompatibility.Mode.TRANSCODE) {
                    val target = decision.decision.target
                    val profile = when (target?.profile) { 66 -> " Baseline"; 77 -> " Main"; 100 -> " High"; else -> "" }
                    VideoStatsText(R.string.video_stats_transcode,
                        listOf(codec(decision.source.codec), "${codec(target?.codec)}$profile") + details)
                } else VideoStatsText(R.string.video_stats_passthrough, listOf(codec(value.clink?.codec)) + details)
            }
        }
        value.screen?.let {
            val hz = if (it.refreshHz % 1.0 == 0.0) it.refreshHz.toLong().toString() else decimal(it.refreshHz)
            lines += VideoStatsText(R.string.video_stats_screen, listOf(size(it.widthPx, it.heightPx), hz))
        }
        if (report is VideoMeasurement.Values && report.sendStalls > 0)
            lines += VideoStatsText(R.string.video_stats_send_stalls, listOf(report.sendStalls.toString()))
        sections += VideoStatsSection(VideoStatsText(R.string.video_stats_carlink), lines)
    }
    value.browsers.take(4).forEachIndexed { index, browser ->
        val line = when (val report = browser.measurement) {
            is VideoMeasurement.Progress -> progress(report)
            is VideoMeasurement.Values -> VideoStatsText(R.string.video_stats_browser_values,
                listOf(decimal(report.fps), decimal(report.droppedPercent), ms(browser.decodeUs), ms(browser.latencyUs)))
        }
        sections += VideoStatsSection(VideoStatsText(R.string.video_stats_browser, listOf(index + 1)), listOf(line))
    }
    val caption = (value.iphone as? VideoMeasurement.Progress)?.let(::progress)
        ?: VideoStatsText(R.string.video_stats_average, listOf(value.window.seconds.toString()))
    return VideoStatsPresentation(caption, sections)
}
