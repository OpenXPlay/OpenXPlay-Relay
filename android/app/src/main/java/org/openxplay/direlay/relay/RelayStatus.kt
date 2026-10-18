// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.relay

import org.openxplay.stream.core.*
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/** Model-only observer supplements HubListener, which has no frame or connection callbacks. */
class RelayStatus : HubListener, ProjectionOutput {
    override val id = "relay-status"
    private val frames = ConcurrentHashMap<ScreenId, AtomicLong>()
    private val reporters = ConcurrentHashMap<Origin, Long>()
    @Volatile var videoCodec: VideoCodec? = null
        private set
    /** The iPhone's main-screen stream format for the admin page; null between sessions. */
    @Volatile var videoFormat: VideoFormat? = null
        private set
    val receivers = ReceiverDiagnostics()
    @Volatile var session: SessionState? = null
        private set
    @Volatile var failure: String? = null
        private set
    val frameCounts: Map<Int, Long> get() = frames.mapKeys { it.key.value }.mapValues { it.value.get() }
    val recentWebReporters: Int get() {
        val cutoff = System.nanoTime() - 15_000_000_000L
        reporters.entries.removeIf { it.value < cutoff }
        return reporters.size
    }

    private val measurements = ConcurrentHashMap<String, OutputStats>()
    val outputMeasurements: Map<String, OutputStats> get() = measurements.toMap()
    private val latestStats = ConcurrentHashMap<String, String>()
    val outputStats: Map<String, String> get() = latestStats.toMap()
    override fun stats(outputId: String, stats: OutputStats) {
        measurements[outputId] = stats
        latestStats[outputId] = stats.toString()
        if (outputId == "web") reporters[stats.origin] = System.nanoTime()
    }
    override fun receiverDiagnostics(outputId: String, origin: Origin, type: String, json: String) {
        receivers.report(origin.clientId, type, json)
        if (type == "fullscreen-exit") android.util.Log.i("OpenXPlay Relay", "Fullscreen exit ${origin.clientId}: $json")
    }
    fun clearFailure() { failure = null }
    override fun outputFailed(outputId: String, failure: Throwable) {
        this.failure = "$outputId: ${failure.message ?: failure.javaClass.simpleName}"
    }
    override fun inputFailed(failure: Throwable) {
        this.failure = "Input: ${failure.message ?: failure.javaClass.simpleName}"
    }
    override fun open(host: OutputHost) = Unit
    override fun onSession(state: SessionState, config: SessionConfig) {
        if (session?.id != state.id) { frames.clear(); clearFailure() }
        if (session?.id != state.id || state.phase == SessionPhase.ENDED) { videoCodec = null; videoFormat = null }
        session = state
    }
    override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) {
        if (screen == ScreenId.MAIN) { videoCodec = format?.codec; videoFormat = format }
    }
    override fun onVideoFrame(frame: VideoFrame) {
        if (frame.session == session?.id && session?.phase == SessionPhase.ACTIVE) {
            frames.computeIfAbsent(frame.screen) { AtomicLong() }.incrementAndGet()
        }
    }
    override fun onAudioFormat(stream: Int, format: AudioFormat?) = Unit
    override fun onAudioPacket(packet: AudioPacket) = Unit
    override fun onMicrophone(stream: Int, request: MicRequest?) = Unit
    override fun close() { reporters.clear(); receivers.clear() }
}
