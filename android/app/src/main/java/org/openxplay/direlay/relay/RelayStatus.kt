// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.relay

import org.openxplay.stream.core.*
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

data class RelayFailure(val outputId: String?, val text: String, val atMillis: Long = 0)

/** Model-only observer supplements HubListener, which has no frame or connection callbacks. */
class RelayStatus(private val redactFailure: (String) -> String = { redactFailureText(it) },
    private val wallClock: () -> Long = System::currentTimeMillis,
    private val connectionLog: org.openxplay.direlay.logs.ConnectionLog? = null,
    private val browser: org.openxplay.direlay.logs.BrowserTimeline? = null,
    private val carPlayStage: CarPlayStageTracker? = null,
    private val iphoneLink: IphoneLinkTracker? = null,
    private val clock: () -> Long = System::nanoTime) : HubListener, ProjectionOutput {
    override val id = "relay-status"
    private val frames = ConcurrentHashMap<ScreenId, AtomicLong>()
    @Volatile var countVideoBytes = false
    private val bytes = AtomicLong()
    val videoBytes: Long get() = bytes.get()
    val mainFrames: Long get() = frames[ScreenId.MAIN]?.get() ?: 0L
    private data class WebReport(val stats: OutputStats, val seen: Long)
    private val webReports = LinkedHashMap<Origin, WebReport>()
    @Synchronized fun webMeasurements(): List<OutputStats> {
        val cutoff = clock() - 15_000_000_000L
        webReports.entries.removeIf { it.value.seen < cutoff }
        return webReports.values.map { it.stats }
    }
    @Synchronized private fun rememberWeb(stats: OutputStats) {
        if (stats.screen != ScreenId.MAIN) return
        webReports.remove(stats.origin)
        if (webReports.size >= ReceiverDiagnostics.MAX_CLIENTS) webReports.remove(webReports.keys.first())
        webReports[stats.origin] = WebReport(stats, clock())
    }
    @Volatile var videoCodec: VideoCodec? = null
        private set
    /** The iPhone's main-screen stream format for the admin page; null between sessions. */
    @Volatile var videoFormat: VideoFormat? = null
        private set
    val receivers = ReceiverDiagnostics()
    @Volatile var session: SessionState? = null
        private set
    @Volatile var failures: List<RelayFailure> = emptyList()
        private set
    val failure: String? get() = failures.firstOrNull()?.text
    val frameCounts: Map<Int, Long> get() = frames.mapKeys { it.key.value }.mapValues { it.value.get() }
    val recentWebReporters: Int get() = webMeasurements().size
    private val webClients = linkedSetOf<String>()
    val connectedWebPages: Int get() = synchronized(this) { webClients.size }
    @Synchronized fun clearWebClients() { webClients.clear(); webReports.clear() }
    @Synchronized private fun webClient(client: String, type: String) {
        if (type == "relay-close") {
            webClients.remove(client)
            webReports.keys.removeAll { it.clientId == client }
        } else if (type == "receiver-info" || type == "stats") webClients.add(client)
    }

    private val measurements = ConcurrentHashMap<String, OutputStats>()
    val outputMeasurements: Map<String, OutputStats> get() = measurements.toMap()
    private val latestStats = ConcurrentHashMap<String, String>()
    val outputStats: Map<String, String> get() = latestStats.toMap()
    override fun stats(outputId: String, stats: OutputStats) {
        measurements[outputId] = stats
        latestStats[outputId] = stats.toString()
        if (outputId == "web") rememberWeb(stats)
    }
    override fun receiverDiagnostics(outputId: String, origin: Origin, type: String, json: String) {
        if (outputId == "web") {
            webClient(origin.clientId, type)
            runCatching { browser?.report(origin.clientId, type, json) }
        }
        receivers.report(origin.clientId, type, json)
        if (type == "fullscreen-exit") android.util.Log.i("OpenXPlay Relay", "Fullscreen exit ${origin.clientId}: $json")
    }
    @Synchronized fun clearFailure() { failures = emptyList() }
    @Synchronized private fun rememberFailure(outputId: String?, failure: Throwable) {
        val text = "${outputId ?: "Input"}: ${failureText(failure, redactFailure)}"
        failures = (listOf(RelayFailure(outputId, text, wallClock())) + failures.filterNot { it.text == text }).take(10)
    }
    override fun outputFailed(outputId: String, failure: Throwable) {
        rememberFailure(outputId, failure)
    }
    override fun inputFailed(failure: Throwable) {
        rememberFailure(null, failure)
    }
    override fun open(host: OutputHost) = Unit
    override fun onSession(state: SessionState, config: SessionConfig) {
        if (session?.id != state.id || session?.phase != state.phase) {
            carPlayStage?.onStreamPhase(state.phase)
            iphoneLink?.onStreamPhase(state.phase)
        }
        if (session?.id != state.id) { synchronized(this) { webReports.clear() }; frames.clear(); bytes.set(0); clearFailure(); awaitingFirstVideo = true }
        if (session?.id != state.id || state.phase == SessionPhase.ENDED) { videoCodec = null; videoFormat = null }
        session = state
    }
    override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) {
        if (screen == ScreenId.MAIN) { videoCodec = format?.codec; videoFormat = format }
    }
    override fun onVideoFrame(frame: VideoFrame) {
        if (awaitingFirstVideo && frame.screen == ScreenId.MAIN && frame.session == session?.id && session?.phase == SessionPhase.ACTIVE) {
            awaitingFirstVideo = false
            runCatching { connectionLog?.phone("first_video", "codec=$videoCodec size=${videoFormat?.width}x${videoFormat?.height}") }
        }
        if (countVideoBytes && frame.screen == ScreenId.MAIN && frame.session == session?.id && session?.phase == SessionPhase.ACTIVE)
            bytes.addAndGet(frame.data.size.toLong())
        if (frame.session == session?.id && session?.phase == SessionPhase.ACTIVE) {
            frames.computeIfAbsent(frame.screen) { AtomicLong() }.incrementAndGet()
        }
    }
    override fun onAudioFormat(stream: Int, format: AudioFormat?) = Unit
    override fun onAudioPacket(packet: AudioPacket) = Unit
    override fun onMicrophone(stream: Int, request: MicRequest?) = Unit
    override fun close() { countVideoBytes = false; synchronized(this) { webReports.clear(); webClients.clear() }; receivers.clear() }
    @Volatile private var awaitingFirstVideo = false
}
