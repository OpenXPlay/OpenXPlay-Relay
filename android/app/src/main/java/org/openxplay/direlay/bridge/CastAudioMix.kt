// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.bridge

import android.util.Log
import java.util.ArrayDeque
import java.util.concurrent.atomic.AtomicReference
import org.openxplay.stream.core.*

/** DLNA still receives one MEDIA AAC stream; all other output paths keep their own audio policy. */
internal class CastAudioMix(
    private val inner: ProjectionOutput,
    private val codecs: CastAudioCodec.Factory = CastAudioCodecs,
) : ProjectionOutput by inner {
    private val current = AtomicReference<CastAudioMixer?>()

    override fun open(host: OutputHost) {
        val mixer = CastAudioMixer(inner, codecs) { Log.w("OpenXPlay DLNA", it) }
        check(current.compareAndSet(null, mixer)) { "output_already_open" }
        try {
            inner.open(host)
            Thread({ try { mixer.run() } finally {
                try { mixer.finish() } finally {
                    try { inner.close() } finally { current.compareAndSet(mixer, null) }
                }
            } }, "cast-audio-mix")
                .apply { isDaemon = true; start() }
        } catch (failure: Exception) {
            current.compareAndSet(mixer, null); mixer.close(); mixer.finish(); inner.close(); throw failure
        }
    }
    // The cast output filters video by session: it must see the session in hub order, before the next frame.
    override fun onSession(state: SessionState, config: SessionConfig) { inner.onSession(state, config); current.get()?.session(state, config) }
    override fun onAudioFormat(stream: Int, format: AudioFormat?) { current.get()?.format(stream, format) }
    override fun onAudioPacket(packet: AudioPacket) { current.get()?.offer(packet) }
    override fun close() { current.get()?.close() }

    companion object { const val MIX_STREAM = 100 }
}

/** The inbox lock protects only bounded input state. PCM, codecs and inner calls belong to the worker. */
internal class CastAudioMixer(
    private val inner: ProjectionOutput,
    private val codecs: CastAudioCodec.Factory,
    private val diagnostic: (String) -> Unit,
) {
    private data class Queued(val packet: AudioPacket, val frames: Long)
    private class Inbox(val format: AudioFormat?) {
        val packets = ArrayDeque<Queued>()
        var frames = 0L
        var dropped = 0L
    }
    private data class Input(val id: Int, val inbox: Inbox, val packets: List<Queued>, val dropped: Long)
    private data class Work(val revision: Long, val session: Pair<SessionState, SessionConfig>?, val inputs: List<Input>)
    private class Source(val inbox: Inbox, val format: AudioFormat) {
        val fifo = PcmFifo()
        var resampler = StereoResampler(format.sampleRate, format.channels)
        var decoder: CastAudioCodec? = null
        var readyAt: Long? = null
        fun reset() {
            runCatching { decoder?.close() }; decoder = null
            fifo.clear(); readyAt = null; resampler = StereoResampler(format.sampleRate, format.channels)
        }
    }
    private val gate = Object()
    @Volatile private var running = true
    private val inboxes = linkedMapOf<Int, Inbox>()
    private var sessionId: SessionId? = null
    private var ended = false
    private var revision = 0L
    private var pendingSession: Pair<SessionState, SessionConfig>? = null
    private var dirty = false
    private var applied = -1L
    private var outputSession: SessionId? = null
    private val sources = linkedMapOf<Int, Source>()
    private val reported = mutableSetOf<String>()
    internal val droppedFrames = mutableMapOf<Int, Long>()
    private var announced = false
    private var encoder: CastAudioCodec? = null
    private var retryEncoder = false
    private var firstEncoderPts: Long? = null
    private var encoderOffset: Long? = null
    private var anchor: Long? = null
    private var tickFrames = 0L
    private var nextTick: Long? = null

    fun session(state: SessionState, config: SessionConfig) = synchronized(gate) {
        if (!running) return@synchronized
        if (sessionId != state.id || state.phase == SessionPhase.ENDED) { inboxes.clear(); revision++ }
        sessionId = state.id; ended = state.phase == SessionPhase.ENDED
        pendingSession = state to config; dirty = true
        gate.notifyAll()
    }
    fun format(stream: Int, format: AudioFormat?) = synchronized(gate) {
        if (!running || ended) return@synchronized
        val old = inboxes[stream]?.format
        if (sameFormat(old, format)) return@synchronized
        inboxes[stream] = Inbox(format); dirty = true
        gate.notifyAll()
    }
    fun offer(packet: AudioPacket) = synchronized(gate) {
        if (!running || ended || packet.session != sessionId || packet.data.isEmpty()) return@synchronized
        val inbox = inboxes[packet.stream] ?: return@synchronized
        val format = inbox.format ?: return@synchronized
        val frames = if (format.codec == AudioCodec.PCM_S16LE && format.sampleRate > 0 && format.channels > 0)
            packet.data.size.toLong() * RATE / format.sampleRate / (format.channels * 2L) else TICK.toLong()
        inbox.packets.addLast(Queued(packet, frames)); inbox.frames += frames
        // Bound the receive side too, including when a device codec temporarily stops accepting input.
        while (inbox.packets.size > 1 && (inbox.frames > CAPACITY || inbox.packets.size > 32)) {
            val removed = inbox.packets.removeFirst().frames
            inbox.frames -= removed; inbox.dropped += removed
        }
        dirty = true
        gate.notifyAll()
    }
    fun close() = synchronized(gate) { running = false; inboxes.clear(); gate.notifyAll() }

    fun run() {
        while (running) {
            step(System.nanoTime())
            synchronized(gate) {
                if (running && !dirty) {
                    val due = nextTick.takeUnless { retryEncoder }
                    if (due == null) gate.wait()
                    else {
                        val remaining = due - System.nanoTime()
                        if (remaining > 0) gate.wait(minOf(5L, (remaining / 1_000_000).coerceAtLeast(1)))
                    }
                }
            }
        }
    }

    /** One worker iteration, also used with a deterministic monotonic clock in fake-codec tests. */
    internal fun step(now: Long) {
        val work = synchronized(gate) {
            if (!running) return
            dirty = false
            Work(revision, pendingSession.also { pendingSession = null }, inboxes.map { (id, inbox) ->
                Input(id, inbox, inbox.packets.toList(), inbox.dropped).also {
                    inbox.packets.clear(); inbox.frames = 0; inbox.dropped = 0
                }
            })
        }
        if (work.revision != applied) {
            finish(); droppedFrames.clear(); applied = work.revision; outputSession = null
        }
        work.session?.let { (state, _) -> outputSession = state.id.takeUnless { state.phase == SessionPhase.ENDED } }
        for (id in sources.keys.toList()) {
            if (work.inputs.none { it.id == id && it.inbox === sources[id]?.inbox }) sources.remove(id)?.reset()
        }
        var received = false
        for (input in work.inputs) {
            val format = input.inbox.format ?: continue
            if (!supported(format)) {
                once("unsupported:${input.id}:${format.role}:${format.codec}:${format.sampleRate}:${format.channels}:${format.config?.contentToString()}",
                    "audio unsupported stream=${input.id} role=${format.role} codec=${format.codec} rate=${format.sampleRate} channels=${format.channels}")
                continue
            }
            val source = sources.getOrPut(input.id) { Source(input.inbox, format) }
            droppedFrames[input.id] = (droppedFrames[input.id] ?: 0) + input.dropped
            for ((packet) in input.packets) {
                received = true
                if (format.codec == AudioCodec.PCM_S16LE) {
                    try { pcm(source, packet.data, now) }
                    catch (_: Exception) { source.reset(); once("pcm:${input.id}", "audio invalid PCM stream=${input.id}") }
                } else {
                    try {
                        val decoder = source.decoder ?: codecs.decoder().also { source.decoder = it }
                        decoder.offer(packet.data, packet.ptsUs)
                        decoder.pump { data, _ -> pcm(source, data, now) }
                    } catch (failure: Exception) { decoderFailed(input.id, source, failure) }
                }
            }
            try { source.decoder?.pump { data, _ -> pcm(source, data, now) } }
            catch (failure: Exception) { decoderFailed(input.id, source, failure) }
            if (source.fifo.droppedFrames > 0) {
                droppedFrames[input.id] = (droppedFrames[input.id] ?: 0) + source.fifo.droppedFrames
                source.fifo.droppedFrames = 0
            }
            if (checkNotNull(droppedFrames[input.id]) > 0) once("overflow:${input.id}",
                "audio queue dropped stream=${input.id} frames=${droppedFrames[input.id]}")
        }
        if (!current(work.revision)) return
        if (sources.isEmpty() || outputSession == null) { stopMix(); return }
        if (!announced) {
            inner.onAudioFormat(CastAudioMix.MIX_STREAM, AudioFormat(CastAudioMix.MIX_STREAM,
                AudioRole.MEDIA, AudioCodec.AAC_LC, RATE, 2, byteArrayOf(0x11, 0x90.toByte())))
            announced = true
        }
        if (anchor == null) { anchor = now; tickFrames = 0; nextTick = now }
        if (received) retryEncoder = false
        try {
            if (!retryEncoder && now >= checkNotNull(nextTick)) {
                val active = encoder ?: codecs.encoder().also { encoder = it }
                // Resume at the current clock tick after a stall instead of emitting a burst of stale audio.
                val elapsedTicks = ((now - anchor!! + 1) * RATE - 1) / (TICK * 1_000_000_000L)
                tickFrames = maxOf(tickFrames, elapsedTicks * TICK)
                val mixed = IntArray(TICK * 2)
                for (source in sources.values) if (source.readyAt?.let { now >= it } == true) source.fifo.mix(mixed)
                val pcm = ByteArray(TICK * 4)
                for (i in mixed.indices) {
                    val sample = mixed[i].coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt())
                    pcm[i * 2] = sample.toByte(); pcm[i * 2 + 1] = (sample shr 8).toByte()
                }
                val pts = anchor!! / 1000 + tickFrames * 1_000_000 / RATE
                if (firstEncoderPts == null) firstEncoderPts = pts
                active.offer(pcm, pts)
                tickFrames += TICK
                nextTick = anchor!! + tickFrames * 1_000_000_000 / RATE
            }
            encoder?.pump { data, pts ->
                if (encoderOffset == null) encoderOffset = checkNotNull(firstEncoderPts) - pts
                if (current(work.revision)) inner.onAudioPacket(AudioPacket(CastAudioMix.MIX_STREAM,
                    checkNotNull(outputSession), data, pts + encoderOffset!!))
            }
        } catch (failure: Exception) {
            releaseEncoder(); retryEncoder = true
            sources.values.forEach { it.reset() }
            once("encoder", "audio encoder failed: ${failure.javaClass.simpleName}; retry on next packet")
        }
    }
    private fun pcm(source: Source, data: ByteArray, now: Long) {
        val pcm = source.resampler.convert(data)
        if (pcm.isEmpty()) return
        if (source.readyAt == null) source.readyAt = now + 40_000_000L
        source.fifo.append(pcm)
    }
    private fun decoderFailed(id: Int, source: Source, failure: Exception) {
        source.reset()
        once("decoder:$id", "audio decoder failed stream=$id: ${failure.javaClass.simpleName}; retry on next packet")
    }
    private fun current(expected: Long): Boolean = synchronized(gate) { running && revision == expected }
    private fun once(key: String, message: String) { if (reported.add(key)) diagnostic(message) }
    private fun releaseEncoder() {
        runCatching { encoder?.close() }; encoder = null; firstEncoderPts = null; encoderOffset = null
    }
    private fun stopMix() {
        releaseEncoder(); retryEncoder = false; anchor = null; nextTick = null
        if (announced) { announced = false; inner.onAudioFormat(CastAudioMix.MIX_STREAM, null) }
    }
    internal fun finish() { sources.values.forEach { it.reset() }; sources.clear(); stopMix() }

    private class PcmFifo {
        private val samples = ShortArray(CAPACITY * 2)
        private var head = 0
        private var size = 0
        var droppedFrames = 0L
        fun clear() { head = 0; size = 0 }
        fun append(pcm: ByteArray) {
            require(pcm.size % 4 == 0)
            for (offset in pcm.indices step 4) {
                if (size == samples.size) { head = (head + 2) % samples.size; size -= 2; droppedFrames++ }
                for (channel in 0..1) {
                    val i = offset + channel * 2
                    samples[(head + size) % samples.size] =
                        ((pcm[i].toInt() and 255) or (pcm[i + 1].toInt() shl 8)).toShort()
                    size++
                }
            }
        }
        fun mix(output: IntArray) {
            for (i in 0 until minOf(size, output.size)) {
                output[i] += samples[head].toInt(); head = (head + 1) % samples.size
                size--
            }
        }
    }
    companion object {
        private const val RATE = 48_000
        private const val TICK = 1024
        private const val CAPACITY = RATE * 300 / 1000
        private fun supported(format: AudioFormat): Boolean = when (format.codec) {
            AudioCodec.PCM_S16LE -> format.sampleRate in 8_000..48_000 && format.channels in 1..2
            AudioCodec.AAC_LC -> format.role == AudioRole.MEDIA && format.sampleRate == RATE && format.channels == 2 &&
                format.config?.contentEquals(byteArrayOf(0x11, 0x90.toByte())) == true
            else -> false
        }
        private fun sameFormat(a: AudioFormat?, b: AudioFormat?): Boolean = a === b || a != null && b != null &&
            a.role == b.role && a.codec == b.codec && a.sampleRate == b.sampleRate && a.channels == b.channels &&
            java.util.Arrays.equals(a.config, b.config)
    }
}
