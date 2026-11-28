// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-27
package org.openxplay.direlay.relay

import org.openxplay.direlay.bridge.CastAudioCodec
import org.openxplay.direlay.bridge.CastAudioCodecs
import org.openxplay.stream.core.*

/** Adapts raw CarPlay AAC to the CarLife output's PCM contract. */
internal class CarlifeAudioOutput(
    private val destination: ProjectionOutput,
    private val codecs: CastAudioCodec.Factory = CastAudioCodecs,
    private val onClosed: () -> Unit = {},
    private val diagnostic: (String, String) -> Unit = { _, _ -> },
) : ProjectionOutput by destination {
    private companion object {
        const val QUEUE_PACKETS = 32
        const val QUEUE_BYTES = 256 * 1024
        const val PUMP_INTERVAL_MS = 5L
    }

    // Each announcement is a distinct generation, even when the format is identical.
    private class Announcement(val stream: Int, val format: AudioFormat?, val sessionEpoch: Long) {
        val decode = format?.let {
            it.role == AudioRole.MEDIA && it.codec == AudioCodec.AAC_LC && it.sampleRate == 48_000 && it.channels == 2
        } == true
        fun facts() = "stream=$stream codec=${format?.codec} rate=${format?.sampleRate} channels=${format?.channels}"
    }
    private class Decoder(val announcement: Announcement, val codec: CastAudioCodec) {
        var session: SessionId? = null
    }
    private class Queued(val packet: AudioPacket, val announcement: Announcement?, val sessionEpoch: Long)
    private class Work(val format: Announcement?, val packet: Queued?)

    // Only mailbox operations and the destination's nonblocking enqueue callbacks use this lock.
    private val gate = Object()
    private val formats = mutableMapOf<Int, Announcement>()
    // Keep format changes outside the packet limit so an overrun cannot discard a stream end.
    private val pendingFormats = ArrayDeque<Announcement>()
    private val packets = ArrayDeque<Queued>()
    private var bytes = 0
    private var dropped = 0L
    private var session: SessionState? = null
    private var sessionEpoch = 0L
    private var stopped = false
    private val worker = Thread(::run, "relay-carlife-audio").apply { isDaemon = true }
    private var started = false
    private val decoders = mutableMapOf<Int, Decoder>() // Worker-owned.

    override fun onAudioFormat(stream: Int, format: AudioFormat?) {
        synchronized(gate) {
            if (stopped || session?.phase == SessionPhase.ENDED) return
            val announcement = Announcement(stream, format, sessionEpoch)
            formats[stream] = announcement
            pendingFormats.addLast(announcement)
            wake()
        }
    }

    override fun onAudioPacket(packet: AudioPacket) {
        synchronized(gate) {
            if (stopped || session?.let { it.phase == SessionPhase.ENDED || it.id != packet.session } == true) return
            val size = packet.data.size
            if (size > QUEUE_BYTES) {
                dropped++
                return
            }
            while (packets.size == QUEUE_PACKETS || bytes + size > QUEUE_BYTES) {
                pollPacket()
                dropped++
            }
            packets.addLast(Queued(packet, formats[packet.stream], sessionEpoch))
            bytes += size
            wake()
        }
    }

    override fun onSession(state: SessionState, config: SessionConfig) {
        synchronized(gate) {
            if (stopped) return
            if (session?.id != state.id || state.phase == SessionPhase.ENDED) clearAudio()
            session = state
            // Video frames must see the new session immediately.
            destination.onSession(state, config)
            gate.notifyAll()
        }
    }

    override fun close() {
        synchronized(gate) {
            if (stopped) return
            stopped = true
            clearAudio()
            wake()
        }
    }

    private fun clearAudio() {
        sessionEpoch++
        formats.clear()
        pendingFormats.clear()
        dropped += packets.size
        packets.clear()
        bytes = 0
    }

    private fun wake() {
        if (!started) {
            worker.start()
            started = true
        }
        gate.notifyAll()
    }

    private fun pollPacket(): Queued? = packets.removeFirstOrNull()?.also { bytes -= it.packet.data.size }

    private fun takeWork(): Work? = synchronized(gate) {
        if (!stopped && pendingFormats.isEmpty() && packets.isEmpty()) {
            gate.wait(if (decoders.isEmpty()) 0L else PUMP_INTERVAL_MS)
        }
        if (stopped) return null
        val format = pendingFormats.removeFirstOrNull()
        Work(format, if (format == null) pollPacket() else null)
    }

    private fun run() {
        try {
            while (true) {
                val work = takeWork() ?: break
                work.format?.let(::applyFormat)
                work.packet?.let(::acceptPacket)
                // MediaCodec may produce output after the final input call returns.
                pumpDecoders()
            }
        } finally {
            decoders.values.forEach(::closeDecoder)
            report("audio_queue_summary", "droppedPackets=${synchronized(gate) { dropped }}")
            try { destination.close() } finally { onClosed() }
        }
    }

    private fun current(announcement: Announcement): Boolean =
        !stopped && formats[announcement.stream] === announcement

    private fun applyFormat(announcement: Announcement) {
        if (!synchronized(gate) { !stopped && sessionEpoch == announcement.sessionEpoch }) return
        val stream = announcement.stream
        decoders.remove(stream)?.let(::closeDecoder)
        if (!announcement.decode) {
            sendFormat(announcement, announcement.format)
            return
        }
        try {
            decoders[stream] = Decoder(announcement, codecs.decoder())
            report("audio_decoder_started", announcement.facts())
        } catch (failure: Exception) {
            failDecoder(announcement, failure)
            return
        }
        sendFormat(announcement, AudioFormat(stream, AudioRole.MEDIA, AudioCodec.PCM_S16LE, 48_000, 2))
    }

    private fun sendFormat(announcement: Announcement, format: AudioFormat?) {
        synchronized(gate) {
            if (!stopped && sessionEpoch == announcement.sessionEpoch) {
                destination.onAudioFormat(announcement.stream, format)
            }
        }
    }

    private fun acceptPacket(queued: Queued) {
        val packet = queued.packet
        synchronized(gate) {
            if (stopped || sessionEpoch != queued.sessionEpoch || formats[packet.stream] !== queued.announcement) {
                dropped++
                return
            }
            if (queued.announcement?.decode != true) {
                destination.onAudioPacket(packet)
                return
            }
        }
        val decoder = decoders[packet.stream]?.takeIf { it.announcement === queued.announcement } ?: return
        try {
            decoder.codec.offer(packet.data, packet.ptsUs)
            decoder.session = packet.session
        } catch (failure: Exception) {
            failDecoder(decoder.announcement, failure)
        }
    }

    private fun pumpDecoders() {
        for (decoder in decoders.values.toList()) {
            val announcement = decoder.announcement
            if (!synchronized(gate) { current(announcement) }) {
                decoders.remove(announcement.stream)
                closeDecoder(decoder)
                continue
            }
            val sourceSession = decoder.session ?: continue
            try {
                decoder.codec.pump { pcm, pts ->
                    synchronized(gate) {
                        if (current(announcement)) {
                            destination.onAudioPacket(AudioPacket(announcement.stream, sourceSession, pcm, pts))
                        }
                    }
                }
            } catch (failure: Exception) {
                failDecoder(announcement, failure)
            }
        }
    }

    private fun failDecoder(announcement: Announcement, failure: Exception) {
        report("audio_decoder_failed", "${announcement.facts()} error=${failure.javaClass.simpleName}")
        decoders.remove(announcement.stream)?.let(::closeDecoder)
        sendFormat(announcement, null)
    }

    private fun closeDecoder(decoder: Decoder) {
        val failure = runCatching { decoder.codec.close() }.exceptionOrNull()
        val result = failure?.let { " error=${it.javaClass.simpleName}" }.orEmpty()
        report("audio_decoder_closed", decoder.announcement.facts() + result)
    }

    private fun report(event: String, detail: String) {
        runCatching { diagnostic(event, detail) }
    }
}
