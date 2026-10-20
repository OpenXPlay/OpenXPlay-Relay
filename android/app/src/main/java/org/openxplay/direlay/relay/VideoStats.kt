// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.relay

import org.openxplay.direlay.settings.VideoStatsWindow
import org.openxplay.direlay.video.VideoCompatibility
import org.openxplay.stream.clink.ClinkStatistics
import org.openxplay.stream.core.*
import kotlin.math.ceil

data class VideoCounters(val frames: Long, val bytes: Long = 0, val dropped: Long = 0,
    val placeholders: Long = 0, val stalls: Long = 0, val relayDropped: Long = 0) {
    fun precedes(other: VideoCounters) = frames < other.frames || bytes < other.bytes || dropped < other.dropped ||
        placeholders < other.placeholders || stalls < other.stalls || relayDropped < other.relayDropped
}

sealed interface VideoMeasurement {
    data class Progress(val seconds: Int) : VideoMeasurement
    data class Values(val fps: Double, val bitsPerSecond: Double, val droppedPercent: Double,
        val placeholders: Long, val frames: Long, val sendStalls: Long) : VideoMeasurement
}

/** One series, sampled by the visible activity. No timers or media-thread work. */
class VideoStatsSampler(private val clock: () -> Long = System::nanoTime) {
    private data class Sample(val at: Long, val counters: VideoCounters)
    private var samples = ArrayDeque<Sample>()
    private var session: SessionId? = null
    private var window = VideoStatsWindow.OFF
    val retainedSamples: Int get() = samples.size
    fun clear() { if (samples.isNotEmpty()) samples = ArrayDeque(); session = null; window = VideoStatsWindow.OFF }
    fun sample(session: SessionId, window: VideoStatsWindow, counters: VideoCounters) {
        if (window == VideoStatsWindow.OFF) { clear(); return }
        val now = clock()
        if (this.session != session || this.window != window || samples.lastOrNull()?.let { counters.precedes(it.counters) } == true)
            samples.clear()
        this.session = session; this.window = window
        if (samples.lastOrNull()?.let { now - it.at < 1_000_000_000L } == true) return
        if (samples.size == MAX_SAMPLES) samples.removeFirst()
        samples.addLast(Sample(now, counters))
    }
    fun report(window: VideoStatsWindow): VideoMeasurement {
        if (this.window != window) { samples.clear(); this.window = window }
        val last = samples.lastOrNull() ?: return VideoMeasurement.Progress(window.seconds)
        val first = samples.lastOrNull { last.at - it.at >= window.seconds * 1_000_000_000L }
            ?: return VideoMeasurement.Progress(ceil(window.seconds - (last.at - samples.first().at) / 1e9).toInt().coerceAtLeast(1))
        val seconds = (last.at - first.at) / 1e9
        val delivered = last.counters.frames - first.counters.frames
        val dropped = (last.counters.dropped - first.counters.dropped).toDouble() +
            (last.counters.relayDropped - first.counters.relayDropped).toDouble()
        return VideoMeasurement.Values(delivered / seconds, (last.counters.bytes - first.counters.bytes) * 8.0 / seconds,
            if (delivered.toDouble() + dropped == 0.0) 0.0 else dropped * 100 / (delivered + dropped),
            last.counters.placeholders - first.counters.placeholders, delivered, last.counters.stalls - first.counters.stalls)
    }
    companion object { const val MAX_SAMPLES = 16 }
}

data class CarlinkVideoDecision(val source: VideoFormat, val decision: VideoCompatibility.Decision)
data class BrowserVideoStats(val measurement: VideoMeasurement, val decodeUs: Long?, val latencyUs: Long?)
data class VideoStatsSnapshot(val window: VideoStatsWindow, val iphone: VideoMeasurement, val format: VideoFormat?,
    val carlink: VideoMeasurement?, val clink: ClinkStatistics?, val decision: CarlinkVideoDecision?, val screen: DisplayInfo?,
    val browsers: List<BrowserVideoStats>, val placeholderActive: Boolean = false)

/** Owns only bounded, disposable measurement windows; the runtime retains the latest reports. */
class VideoStats(private val clock: () -> Long = System::nanoTime) {
    private val iphone = VideoStatsSampler(clock)
    private val carlink = VideoStatsSampler(clock)
    private class Browser(clock: () -> Long) {
        val sampler = VideoStatsSampler(clock)
        var relayDropped = 0L
    }
    private val browsers = LinkedHashMap<Origin, Browser>()
    private var lastClink: ClinkStatistics? = null
    private var placeholderActive = false
    fun clear() { iphone.clear(); carlink.clear(); browsers.clear(); lastClink = null; placeholderActive = false }
    fun sample(status: RelayStatus, window: VideoStatsWindow, carlinkOn: Boolean,
        clink: ClinkStatistics?, decision: CarlinkVideoDecision?, screen: DisplayInfo?): VideoStatsSnapshot? {
        val session = status.session
        val enabled = window != VideoStatsWindow.OFF && session?.phase == SessionPhase.ACTIVE
        status.countVideoBytes = enabled
        if (!enabled) { clear(); return null }
        val id = checkNotNull(session).id
        iphone.sample(id, window, VideoCounters(status.mainFrames, status.videoBytes))
        val car = if (carlinkOn && clink != null) {
            val previous = lastClink
            if (previous == null || clink.liveFrames < previous.liveFrames || clink.placeholderFrames < previous.placeholderFrames)
                placeholderActive = clink.placeholderFrames > 0 && clink.liveFrames == 0L
            else if (clink.liveFrames > previous.liveFrames) placeholderActive = false
            else if (clink.placeholderFrames > previous.placeholderFrames) placeholderActive = true
            lastClink = clink
            carlink.sample(id, window, VideoCounters(clink.liveFrames, clink.liveBytes, clink.droppedFrames, clink.placeholderFrames, clink.sendStalls))
            carlink.report(window)
        } else { carlink.clear(); lastClink = null; placeholderActive = false; if (carlinkOn) VideoMeasurement.Progress(window.seconds) else null }
        val reports = status.webMeasurements().sortedBy { it.origin.clientId }.take(4)
        browsers.keys.retainAll(reports.map { it.origin }.toSet())
        val web = reports.map { report ->
            val browser = browsers.getOrPut(report.origin) { Browser(clock) }
            status.receivers.relayDroppedFrames(report)?.let { browser.relayDropped = it }
            val sampler = browser.sampler
            sampler.sample(id, window, VideoCounters(report.rendered, dropped = report.dropped,
                relayDropped = browser.relayDropped))
            BrowserVideoStats(sampler.report(window), report.decodeTimeUs, report.endToEndLatencyUs)
        }
        return VideoStatsSnapshot(window, iphone.report(window), status.videoFormat, car, clink, decision, screen, web, placeholderActive)
    }
}
