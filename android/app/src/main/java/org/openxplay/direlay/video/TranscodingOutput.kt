// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.video

import org.openxplay.stream.core.*

/** Implementations enqueue work; close cancels promptly and releases resources on their worker. */
interface VideoPipeline {
    fun start()
    fun offer(frame: VideoFrame)
    fun requestKeyFrame()
    fun updateGeometry(geometry: CarlinkGeometry) {}
    fun close()
    interface Listener {
        fun format(format: VideoFormat)
        fun frame(frame: VideoFrame)
        fun failed(error: Exception)
        fun inputKeyFrame() {}
    }
    fun interface Factory {
        fun create(input: VideoFormat, target: VideoCompatibility.Target, listener: Listener): VideoPipeline
    }
}

/** Adapts any Stream output without exposing platform codecs to Stream or to the input. */
class TranscodingOutput(
    private val inner: ProjectionOutput,
    private val factory: VideoPipeline.Factory,
    private val report: (VideoFormat, VideoCompatibility.Decision, Boolean) -> Unit = { _, _, _ -> },
    private val observeCapabilities: (OutputCapabilities?) -> Unit = {},
    private val deliveryCodecs: Set<VideoCodec> = VideoCodec.entries.toSet(),
    private val onFailure: (VideoFormat, Exception) -> Unit = { _, _ -> },
    private val carlinkGeometry: Boolean = false,
    private val reportGeometry: (CarlinkGeometry?, VideoCompatibility.Mode) -> Unit = { _, _ -> },
) : ProjectionOutput by inner {
    private class Stream(val format: VideoFormat) {
        var decision = VideoCompatibility.Decision(VideoCompatibility.Mode.PASSTHROUGH)
        var pipeline: VideoPipeline? = null
        var revision = 0L
        var failed = false
    }
    private val gate = Any()
    private val reported = mutableMapOf<ScreenId, VideoCompatibility.ReportKey>()
    private fun reportChange(format: VideoFormat, decision: VideoCompatibility.Decision, failed: Boolean) {
        val key = VideoCompatibility.reportKey(format, decision, failed)
        if (reported.put(format.screen, key) != key) report(format, decision, failed)
    }
    private val streams = mutableMapOf<ScreenId, Stream>()
    private var capabilities: OutputCapabilities? = null
    private var host: OutputHost? = null
    private var closed = false
    private var area: CarlinkGeometry? = null
    private var requestedArea: Pair<Viewport, CarlinkCrop>? = null

    fun setCarlinkArea(x: Int, y: Int, width: Int, height: Int, canvasWidth: Int, canvasHeight: Int) = locked {
        requestedArea = Viewport(canvasWidth, canvasHeight) to CarlinkCrop(x, y, width, height)
        streams.values.forEach { configure(it) }
    }
    // Requests to the host are queued under the gate and sent after it is released: the hub can call
    // back into the input while the input thread holds its own lock and waits for this gate.
    private val outbox = ArrayList<ControlEvent>()
    private fun requestKeyFrame(screen: ScreenId) { outbox += ControlEvent.KeyFrameRequest(screen, Origin(id, "transcoder")) }
    private var depth = 0
    /** Runs [block] under the gate; the outermost call sends the queued requests once the gate is free. */
    private fun <T> locked(block: () -> T): T {
        val (result, target, events) = synchronized(gate) {
            depth++
            val result = try { block() } finally { depth-- }
            val events = if (depth == 0) outbox.toList().also { outbox.clear() } else emptyList()
            Triple(result, host, events)
        }
        events.forEach { target?.control(it) }
        return result
    }

    override fun open(host: OutputHost) {
        synchronized(gate) { this.host = host; closed = false }
        inner.open(object : OutputHost by host {
            override fun capabilities(capabilities: OutputCapabilities) {
                val open = locked {
                    if (!closed) {
                        this@TranscodingOutput.capabilities = capabilities
                        streams.values.forEach { configure(it) }
                    }
                    !closed
                }
                if (!open) return
                observeCapabilities(capabilities)
                host.capabilities(capabilities)
            }
            override fun control(event: ControlEvent) {
                synchronized(gate) {
                    if (closed) return
                    if (event is ControlEvent.KeyFrameRequest) {
                        val stream = streams[event.screen]
                        stream?.pipeline?.let { it.requestKeyFrame(); return }
                        if (stream?.decision?.mode == VideoCompatibility.Mode.UNSUPPORTED) return
                    }
                }
                val mapped = synchronized(gate) {
                    val geometry = area
                    if (event is ControlEvent.Touch && event.screen == ScreenId.MAIN && geometry != null)
                        event.copy(contacts = event.contacts.map(geometry::map)) else event
                }
                host.control(mapped)
            }
        })
    }

    override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) = locked {
        if (closed) return@locked
        val existing = streams[screen]
        if (format != null && existing != null && sameFormat(existing.format, format)) {
            // The iPhone repeats an identical configuration with every keyframe. Rebuilding the
            // transcoder for it requests another keyframe and loops; keep the running pipeline.
            if (existing.pipeline != null && !existing.failed) return@locked
            if (existing.decision.mode != VideoCompatibility.Mode.TRANSCODE || existing.failed) {
                inner.onVideoFormat(screen, format)
                return@locked
            }
        }
        streams.remove(screen)?.let { it.revision++; it.pipeline?.close() }
        if (format == null) {
            reported.remove(screen)
            if (screen == ScreenId.MAIN) { area = null; reportGeometry(null, VideoCompatibility.Mode.PASSTHROUGH) }
            inner.onVideoFormat(screen, null)
        } else Stream(format).also { streams[screen] = it; configure(it, force = true) }
    }

    private fun configure(stream: Stream, force: Boolean = false) {
        val geometry = if (carlinkGeometry && stream.format.screen == ScreenId.MAIN) capabilities?.displays?.get(ScreenId.MAIN)?.let { display ->
            val canvas = Viewport(stream.format.width, stream.format.height)
            val crop = requestedArea?.takeIf { it.first == canvas }?.second ?: CarlinkCrop(0, 0, canvas.width, canvas.height)
            val decoders = capabilities!!.decoders.filter {
                (if (it.codecString.startsWith("avc1.")) VideoCodec.H264 else VideoCodec.H265) in deliveryCodecs
            }
            val baseline = VideoCompatibility.decide(stream.format, decoders)
            val codec = baseline.target?.codec ?: stream.format.codec
            val limit = if (codec == VideoCodec.H264) baseline.target?.let {
                org.openxplay.direlay.settings.CarDisplayFit.h264MaxMacroblocks(it.codecString)
            } ?: decoders.filter { VideoCompatibility.accepts(stream.format, it) }.mapNotNull {
                org.openxplay.direlay.settings.CarDisplayFit.h264MaxMacroblocks(it.codecString)
            }.maxOrNull() else null
            CarlinkGeometry.calculate(canvas.width, canvas.height, crop, display.viewport ?: Viewport(display.widthPx, display.heightPx), display.frameSize, limit)
        } else null
        // No capability report yet: preserve the existing output's negotiation behavior.
        val decision = capabilities?.let { caps -> VideoCompatibility.decide(stream.format, caps.decoders.filter {
            (if (it.codecString.startsWith("avc1.")) VideoCodec.H264 else VideoCodec.H265) in deliveryCodecs
        }, geometry) }
            ?: VideoCompatibility.Decision(VideoCompatibility.Mode.PASSTHROUGH)
        if (!force && stream.pipeline != null && decision.mode == VideoCompatibility.Mode.TRANSCODE &&
            decision.target?.copy(geometry = null) == stream.decision.target?.copy(geometry = null) &&
            decision.target?.geometry?.picture == stream.decision.target?.geometry?.picture &&
            decision.target?.geometry?.identity == stream.decision.target?.geometry?.identity) {
            geometry?.let { stream.pipeline?.updateGeometry(it) }
            stream.decision = decision
            if (stream.format.screen == ScreenId.MAIN) { area = geometry; reportGeometry(geometry, decision.mode) }
            reportChange(stream.format, decision, stream.failed)
            return
        }
        if (!force && decision == stream.decision) {
            if (!stream.failed && stream.format.screen == ScreenId.MAIN) { area = geometry; reportGeometry(geometry, decision.mode) }
            reportChange(stream.format, decision, stream.failed)
            return
        }
        stream.revision++
        stream.pipeline?.close(); stream.pipeline = null
        stream.failed = false
        if (stream.format.screen == ScreenId.MAIN) { area = geometry; reportGeometry(geometry, decision.mode) }
        stream.decision = decision
        reportChange(stream.format, decision, false)
        if (decision.mode != VideoCompatibility.Mode.TRANSCODE) {
            inner.onVideoFormat(stream.format.screen, if (geometry?.identity == false) null else stream.format)
            return
        }
        inner.onVideoFormat(stream.format.screen, null)
        val revision = stream.revision
        val listener = object : VideoPipeline.Listener {
            private fun current() = !closed && streams[stream.format.screen] === stream && stream.revision == revision
            override fun format(format: VideoFormat) = locked {
                if (current()) {
                    if (capabilities?.decoders?.any { VideoCompatibility.accepts(format, it) } != true ||
                        geometry?.picture?.let { it.width != format.width || it.height != format.height } == true) {
                        failed(IllegalStateException("encoder_profile_mismatch"))
                    } else inner.onVideoFormat(format.screen, format)
                }
            }
            override fun frame(frame: VideoFrame) = synchronized(gate) {
                if (current()) inner.onVideoFrame(frame)
            }
            override fun inputKeyFrame() = locked {
                if (current()) requestKeyFrame(stream.format.screen)
            }
            override fun failed(error: Exception) = locked {
                if (current()) {
                    stream.revision++
                    stream.pipeline?.close(); stream.pipeline = null
                    stream.failed = true
                    onFailure(stream.format, error)
                    reportChange(stream.format, decision, true)
                    if (stream.format.screen == ScreenId.MAIN) {
                        area = area?.let { it.copy(area = CarlinkCrop(0, 0, it.canvas.width, it.canvas.height), picture = it.canvas) }
                        reportGeometry(area, VideoCompatibility.Mode.PASSTHROUGH)
                    }
                    inner.onVideoFormat(stream.format.screen, stream.format)
                    requestKeyFrame(stream.format.screen)
                }
            }
        }
        try {
            stream.pipeline = factory.create(stream.format, checkNotNull(decision.target), listener)
            stream.pipeline?.start()
            // A new decoder needs an input IDR once; receiver recovery thereafter targets the encoder.
            if (!stream.failed) requestKeyFrame(stream.format.screen)
        } catch (failure: Exception) { listener.failed(failure) }
    }

    override fun onVideoFrame(frame: VideoFrame) = synchronized(gate) {
        if (closed) return@synchronized
        val stream = streams[frame.screen] ?: return@synchronized
        val pipeline = stream.pipeline
        if (pipeline != null) pipeline.offer(frame)
        else if (stream.decision.mode == VideoCompatibility.Mode.PASSTHROUGH || stream.failed) inner.onVideoFrame(frame)
    }

    override fun onSession(state: SessionState, config: SessionConfig) = synchronized(gate) {
        if (state.phase == SessionPhase.ENDED) {
            streams.values.forEach { it.revision++; it.pipeline?.close() }; streams.clear(); reported.clear()
            area = null; requestedArea = null; reportGeometry(null, VideoCompatibility.Mode.PASSTHROUGH)
        }
        inner.onSession(state, config)
    }

    override fun close() {
        synchronized(gate) {
            closed = true
            streams.values.forEach { it.revision++; it.pipeline?.close() }; streams.clear(); reported.clear()
            host = null; capabilities = null; area = null; requestedArea = null
            reportGeometry(null, VideoCompatibility.Mode.PASSTHROUGH)
        }
        observeCapabilities(null)
        inner.close()
    }

    private fun sameFormat(a: VideoFormat, b: VideoFormat): Boolean =
        a.codec == b.codec && a.codecString == b.codecString && a.width == b.width && a.height == b.height && a.config.contentEquals(b.config)
}
