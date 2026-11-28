// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-27
package org.openxplay.direlay.relay

import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.direlay.bridge.CastAudioCodec
import org.openxplay.stream.core.*

class CarlifeAudioOutputTest {
    private companion object {
        const val STREAM = 7
        val SESSION = SessionId(1)
        val AAC = AudioFormat(STREAM, AudioRole.MEDIA, AudioCodec.AAC_LC, 48_000, 2,
            byteArrayOf(0x11, 0x90.toByte()))
        val PCM = byteArrayOf(1, 0, 2, 0)
    }

    private sealed interface Event {
        data class Format(val stream: Int, val value: AudioFormat?) : Event
        data class Packet(val value: AudioPacket) : Event
    }

    private class Capture : ProjectionOutput {
        override val id = "carlife"
        private val events = LinkedBlockingQueue<Event>()
        val closed = CountDownLatch(1)
        fun next(): Event = checkNotNull(events.poll(5, TimeUnit.SECONDS)) { "No audio callback" }
        override fun open(host: OutputHost) {}
        override fun onSession(state: SessionState, config: SessionConfig) {}
        override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) {}
        override fun onVideoFrame(frame: VideoFrame) {}
        override fun onAudioFormat(stream: Int, format: AudioFormat?) { events.add(Event.Format(stream, format)) }
        override fun onAudioPacket(packet: AudioPacket) { events.add(Event.Packet(packet)) }
        override fun onMicrophone(stream: Int, request: MicRequest?) {}
        override fun close() { closed.countDown() }
    }

    private class FakeDecoder(private val beforeOutput: () -> Unit) : CastAudioCodec {
        private var pendingPts: Long? = null
        val inputs = mutableListOf<ByteArray>()
        var closed = false
            private set

        override fun offer(data: ByteArray, ptsUs: Long) {
            inputs += data
            pendingPts = ptsUs
        }

        override fun pump(output: (ByteArray, Long) -> Unit) {
            val pts = pendingPts ?: return
            beforeOutput()
            pendingPts = null
            output(PCM, pts + 100)
        }

        override fun close() { closed = true }
    }

    private class Fixture(beforeOutput: () -> Unit = {}) : AutoCloseable {
        val capture = Capture()
        val decoder = FakeDecoder(beforeOutput)
        var decoderStarts = 0
            private set
        val output = CarlifeAudioOutput(capture, object : CastAudioCodec.Factory {
            override fun decoder(): CastAudioCodec {
                decoderStarts++
                return decoder
            }
            override fun encoder(): CastAudioCodec = error("CarLife must not encode audio")
        })

        init { output.onSession(SessionState(SESSION, SessionPhase.ACTIVE), SessionConfig(emptyMap())) }

        override fun close() {
            output.close()
            assertTrue(capture.closed.await(5, TimeUnit.SECONDS), "Destination did not close")
        }
    }

    @Test fun decodesMediaAacToStereoPcmAndUsesTheDecoderTimestamp() {
        Fixture().use { f ->
            val packet = AudioPacket(STREAM, SESSION, byteArrayOf(1, 2), 12_000)
            f.output.onAudioFormat(STREAM, AAC)
            f.output.onAudioPacket(packet)

            val event = f.capture.next() as Event.Format
            val format = checkNotNull(event.value)
            assertEquals(STREAM, event.stream)
            assertEquals(STREAM, format.stream)
            assertEquals(AudioRole.MEDIA, format.role)
            assertEquals(AudioCodec.PCM_S16LE, format.codec)
            assertEquals(48_000, format.sampleRate)
            assertEquals(2, format.channels)
            assertNull(format.config)

            val decoded = (f.capture.next() as Event.Packet).value
            assertEquals(STREAM, decoded.stream)
            assertEquals(SESSION, decoded.session)
            assertEquals(12_100L, decoded.ptsUs)
            assertArrayEquals(PCM, decoded.data)
            assertSame(packet.data, f.decoder.inputs.single())
        }
    }

    @Test fun passesPcmFormatsAndPacketsThroughForEveryRole() {
        Fixture().use { f ->
            for (role in AudioRole.entries) {
                val format = AudioFormat(STREAM, role, AudioCodec.PCM_S16LE, 16_000, 1)
                val packet = AudioPacket(STREAM, SESSION, PCM, 0)
                f.output.onAudioFormat(STREAM, format)
                f.output.onAudioPacket(packet)

                assertEquals(Event.Format(STREAM, format), f.capture.next())
                assertEquals(Event.Packet(packet), f.capture.next())
            }
            assertEquals(0, f.decoderStarts)
        }
    }

    @Test fun nullFormatClosesTheDecoderAndEndsTheStream() {
        Fixture().use { f ->
            f.output.onAudioFormat(STREAM, AAC)
            assertEquals(STREAM, (f.capture.next() as Event.Format).stream)
            f.output.onAudioFormat(STREAM, null)

            assertEquals(Event.Format(STREAM, null), f.capture.next())
            assertTrue(f.decoder.closed)
        }
    }

    @Test fun dropsQueuedAndInFlightAudioAfterFormatChanges() {
        val pumping = CountDownLatch(1)
        val resume = CountDownLatch(1)
        Fixture {
            pumping.countDown()
            check(resume.await(5, TimeUnit.SECONDS))
        }.use { f ->
            f.output.onAudioFormat(STREAM, AAC)
            f.capture.next() as Event.Format
            try {
                f.output.onAudioPacket(AudioPacket(STREAM, SESSION, byteArrayOf(1), 0))
                assertTrue(pumping.await(5, TimeUnit.SECONDS))
                f.output.onAudioPacket(AudioPacket(STREAM, SESSION, byteArrayOf(2), 20_000))

                val replacement = AudioFormat(STREAM, AudioRole.MEDIA, AudioCodec.PCM_S16LE, 48_000, 2)
                val latest = AudioPacket(STREAM, SESSION, PCM, 40_000)
                f.output.onAudioFormat(STREAM, replacement)
                f.output.onAudioPacket(latest)
                resume.countDown()

                assertEquals(Event.Format(STREAM, replacement), f.capture.next())
                assertEquals(Event.Packet(latest), f.capture.next())
                assertEquals(1, f.decoder.inputs.size)
                assertTrue(f.decoder.closed)
            } finally {
                resume.countDown()
            }
        }
    }
}
