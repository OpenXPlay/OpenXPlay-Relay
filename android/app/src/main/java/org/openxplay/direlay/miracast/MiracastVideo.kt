// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.view.Surface
import org.openxplay.stream.core.*
import org.openxplay.stream.core.codec.Nal

/** Hub and surface callbacks only change bounded state. The worker exclusively owns the codec. */
internal class MiracastVideo(
    private val host: OutputHost,
    private val listener: MiracastListener,
    private val id: String,
    private val factory: MiracastCodec.Factory,
) {
    private val gate = Object()
    private val frames = MiracastFrameQueue()
    @Volatile private var running = true
    private var format: VideoFormat? = null
    private var surface: Surface? = null
    private var session: SessionId? = null
    private var ended = false
    private var revision = 0L
    private var requested = false
    private var codec: MiracastCodec? = null
    private var configured = -1L
    private var decoded = 0L
    private var refused: VideoFrame? = null
    private var refusedAt = 0L
    private var statisticsAt: Long? = null
    private val limiter = KeyframeLimiter(1_000_000_000L)

    private fun reset() { revision++; frames.clear(); requested = true; gate.notifyAll() }
    fun session(state: SessionState) = synchronized(gate) {
        if (!running) return@synchronized
        if (session != state.id || state.phase == SessionPhase.ENDED) { format = null; reset() }
        session = state.id
        ended = state.phase == SessionPhase.ENDED
    }
    fun format(value: VideoFormat?) = synchronized(gate) {
        if (!running || ended || MiracastPolicy.sameFormat(format, value)) return@synchronized
        format = value
        reset()
    }
    fun surface(value: Surface?) = synchronized(gate) {
        if (!running || surface === value) return@synchronized
        surface = value
        reset()
    }
    fun videoSize(): Viewport = synchronized(gate) { Viewport(format?.width ?: 0, format?.height ?: 0) }
    fun offer(frame: VideoFrame) = synchronized(gate) {
        if (!running || ended || frame.screen != ScreenId.MAIN || format == null || surface == null ||
            session?.let { it != frame.session } == true) return@synchronized
        if (frames.offer(frame)) { revision++; requested = true }
        gate.notifyAll()
    }
    fun close() = synchronized(gate) { running = false; frames.clear(); gate.notifyAll() }

    fun run() {
        try {
            while (running) {
                step(System.nanoTime())
                synchronized(gate) { if (running) gate.wait(10) }
            }
        } finally { release() }
    }
    private data class Work(val revision: Long, val format: VideoFormat?, val surface: Surface?,
        val frame: VideoFrame?, val request: Boolean, val queued: Int)

    /** Deterministic worker iteration for fake-codec tests; production calls this on miracast-video only. */
    internal fun step(now: Long) {
        val work = synchronized(gate) {
            if (!running) return
            Work(revision, format, surface, frames.peek(), requested, frames.size).also { requested = false }
        }
        if (configured != work.revision) { release(); configured = work.revision }
        val ready = work.format != null && work.surface != null
        val requests = if (ready && (work.request || codec == null))
            limiter.request(ControlEvent.KeyFrameRequest(ScreenId.MAIN, Origin(id, "display")), now)
        else limiter.poll(now)
        if (ready) requests.forEach(host::control)
        if (ready) {
            try {
                val frame = work.frame
                if (codec == null && frame?.keyFrame == true) {
                    val format = checkNotNull(work.format)
                    val created = factory.create(format, checkNotNull(work.surface))
                    codec = created
                    listener.diagnostic("decoder ${created.name} ${MiracastCodecs.mime(format.codec)} ${format.width}x${format.height}")
                }
                val active = codec
                if (active != null) {
                    decoded += active.drain()
                    if (frame == null) refused = null
                    else if (active.offer(Nal.avccToAnnexB(frame.data), frame.ptsUs)) {
                        synchronized(gate) { if (revision == work.revision && frames.peek() === frame) frames.remove() }
                        refused = null
                    } else {
                        if (refused !== frame) { refused = frame; refusedAt = now }
                        check(now - refusedAt < 5_000_000_000L) { "decoder_stalled" }
                    }
                    decoded += active.drain()
                }
            } catch (failure: Exception) {
                release()
                synchronized(gate) {
                    if (revision == work.revision) {
                        if (frames.peek() === work.frame) frames.discard()
                        frames.recover(); requested = true
                    }
                }
                listener.failure("decoder_failed", failure.javaClass.simpleName)
                listener.diagnostic("decoder failed: ${failure.javaClass.simpleName}: ${failure.message.orEmpty()}")
            }
        }
        val last = statisticsAt
        if (last == null) statisticsAt = now
        else if (now - last >= 1_000_000_000L) {
            statisticsAt = now
            val stats = synchronized(gate) {
                MiracastStatistics(decoded, frames.dropped, format?.codec, format?.width ?: 0, format?.height ?: 0)
            }
            listener.statistics(stats)
            host.stats(OutputStats(Origin(id, "display"), ScreenId.MAIN, null, work.queued, stats.droppedFrames, decoded, null))
        }
    }
    private fun release() { codec?.let { runCatching { it.close() } }; codec = null; refused = null }
    internal fun finish() { release() }
}
