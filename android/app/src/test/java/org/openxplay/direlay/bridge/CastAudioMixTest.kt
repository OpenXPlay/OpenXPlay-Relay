// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.bridge

import android.app.Application
import java.util.ArrayDeque
import java.util.concurrent.CompletableFuture
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.stream.core.*
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class CastAudioMixTest {
    private data class Frame(val data: ByteArray, val pts: Long)
    private class Codec(private val encode: Boolean, private val decoded: (ByteArray) -> ByteArray) : CastAudioCodec {
        val inputs = mutableListOf<Frame>()
        private val pending = ArrayDeque<Frame>()
        var fail = false
        var closed = false
        var primingUs = 0L
        var onOffer: () -> Unit = {}
        override fun offer(data: ByteArray, ptsUs: Long) {
            check(!fail) { "codec failed" }
            onOffer()
            val frame = Frame(data.copyOf(), ptsUs)
            inputs += frame; pending += frame
        }
        override fun pump(output: (ByteArray, Long) -> Unit) {
            check(!fail) { "codec failed" }
            while (pending.isNotEmpty()) {
                val frame = pending.removeFirst()
                output(if (encode) byteArrayOf(0x55, inputs.indexOf(frame).toByte()) else decoded(frame.data), frame.pts + primingUs)
            }
        }
        override fun close() { closed = true; pending.clear() }
    }
    private class Codecs : CastAudioCodec.Factory {
        val decoders = mutableListOf<Codec>()
        val encoders = mutableListOf<Codec>()
        var decoded: (ByteArray) -> ByteArray = { it }
        var failEncoder = false
        var failDecoder = false
        var primingUs = 0L
        var onDecoderOffer: () -> Unit = {}
        override fun decoder(): CastAudioCodec {
            check(!failDecoder) { "decoder unavailable" }
            return Codec(false, decoded).also { it.onOffer = onDecoderOffer; decoders += it }
        }
        override fun encoder(): CastAudioCodec {
            check(!failEncoder) { "encoder unavailable" }
            return Codec(true, decoded).also { it.primingUs = primingUs; encoders += it }
        }
    }
    private class Capture : ProjectionOutput {
        override val id = "cast"
        var onCall: () -> Unit = {}
        val sessions = mutableListOf<SessionState>()
        val formats = mutableListOf<Pair<Int, AudioFormat?>>()
        val packets = mutableListOf<AudioPacket>()
        val videos = mutableListOf<VideoFrame>()
        val videoFormats = mutableListOf<VideoFormat?>()
        val closed = CountDownLatch(1)
        val sessionReady = CountDownLatch(1)
        val formatReady = CountDownLatch(1)
        override fun open(host: OutputHost) { onCall() }
        override fun onSession(state: SessionState, config: SessionConfig) { onCall(); sessions += state; sessionReady.countDown() }
        override fun onAudioFormat(stream: Int, format: AudioFormat?) { onCall(); formats += stream to format; formatReady.countDown() }
        override fun onAudioPacket(packet: AudioPacket) { onCall(); packets += packet }
        override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) { onCall(); videoFormats += format }
        override fun onVideoFrame(frame: VideoFrame) { onCall(); videos += frame }
        override fun onMicrophone(stream: Int, request: MicRequest?) = Unit
        override fun close() { onCall(); closed.countDown() }
    }
    private class Fixture : AutoCloseable {
        val inner = Capture()
        val codecs = Codecs()
        val diagnostics = mutableListOf<String>()
        val mixer = CastAudioMixer(inner, codecs) { diagnostics += it }
        init { mixer.session(SessionState(SessionId(1), SessionPhase.ACTIVE), configuration); mixer.step(0) }
        fun format(stream: Int = 0, role: AudioRole = AudioRole.MEDIA, codec: AudioCodec = AudioCodec.AAC_LC,
            rate: Int = 48_000, channels: Int = 2) = mixer.format(stream, AudioFormat(stream, role, codec, rate, channels,
                if (codec == AudioCodec.AAC_LC) byteArrayOf(0x11, 0x90.toByte()) else null))
        fun offer(data: ByteArray, stream: Int = 0, session: Long = 1) = mixer.offer(AudioPacket(stream, SessionId(session), data, 123_000))
        fun tick(index: Long) = mixer.step(index * 1024 * 1_000_000_000 / 48_000)
        override fun close() { mixer.close(); mixer.finish() }
    }
    private fun pcm(frames: Int, left: Int, right: Int = left): ByteArray = ByteArray(frames * 4).also { data ->
        for (i in data.indices step 4) {
            data[i] = left.toByte(); data[i + 1] = (left shr 8).toByte()
            data[i + 2] = right.toByte(); data[i + 3] = (right shr 8).toByte()
        }
    }

    @Test fun mediaAloneIsDecodedBufferedAndEncodedAsOnlyTheSyntheticStream() {
        Fixture().use { f ->
            val raw = byteArrayOf(0x21, 0x43)
            val decoded = pcm(2048, 1200, -1400)
            f.codecs.decoded = { assertArrayEquals(raw, it); decoded }
            f.codecs.primingUs = -21_333
            f.format(); f.offer(raw)
            assertTrue(f.codecs.decoders.isEmpty()); assertTrue(f.inner.formats.isEmpty())
            f.tick(0); f.tick(1); f.tick(2); f.tick(3)
            assertArrayEquals(raw, f.codecs.decoders.single().inputs.single().data)
            val encoded = f.codecs.encoders.single().inputs
            assertArrayEquals(ByteArray(4096), encoded[0].data)
            assertArrayEquals(ByteArray(4096), encoded[1].data)
            assertArrayEquals(decoded.copyOfRange(0, 4096), encoded[2].data)
            assertArrayEquals(decoded.copyOfRange(4096, 8192), encoded[3].data)
            val (stream, format) = f.inner.formats.single()
            assertEquals(CastAudioMix.MIX_STREAM, stream)
            assertEquals(stream, format!!.stream); assertEquals(AudioRole.MEDIA, format.role)
            assertEquals(AudioCodec.AAC_LC, format.codec); assertEquals(48_000, format.sampleRate); assertEquals(2, format.channels)
            assertArrayEquals(byteArrayOf(0x11, 0x90.toByte()), format.config)
            assertEquals(4, f.inner.packets.size)
            for ((index, packet) in f.inner.packets.withIndex()) {
                assertEquals(CastAudioMix.MIX_STREAM, packet.stream); assertEquals(SessionId(1), packet.session)
                assertArrayEquals(byteArrayOf(0x55, index.toByte()), packet.data)
                assertEquals(index * 1024L * 1_000_000 / 48_000, packet.ptsUs)
            }
        }
    }
    @Test fun mediaAndAlertSumWithSaturationInBothChannels() {
        Fixture().use { f ->
            f.format(); f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE)
            f.offer(pcm(2048, 30_000, -30_000)); f.offer(pcm(2048, 10_000, -10_000), 1)
            f.tick(0); f.tick(1); f.tick(2)
            assertArrayEquals(pcm(1024, 32_767, -32_768), f.codecs.encoders.single().inputs[2].data)
            assertEquals(1, f.inner.formats.size)
        }
    }
    @Test fun saturationIsAppliedAfterSummingAllStreams() {
        Fixture().use { f ->
            f.format(); f.offer(pcm(2048, 30_000, -30_000))
            f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE); f.offer(pcm(2048, 10_000, -10_000), 1)
            f.format(2, AudioRole.VOICE, AudioCodec.PCM_S16LE); f.offer(pcm(2048, -10_000, 10_000), 2)
            f.tick(0); f.tick(2)
            assertArrayEquals(pcm(1024, 30_000, -30_000), f.codecs.encoders.single().inputs.last().data)
        }
    }
    @Test fun aNewAlertHasItsOwnStartBufferWhileMediaKeepsPlaying() {
        Fixture().use { f ->
            f.format(); f.offer(pcm(4096, 1000)); f.tick(0); f.tick(1); f.tick(2)
            f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE); f.offer(pcm(2048, 2000), 1)
            f.tick(3); f.tick(4)
            assertArrayEquals(pcm(1024, 1000), f.codecs.encoders.single().inputs.last().data)
            f.tick(5)
            assertArrayEquals(pcm(1024, 3000), f.codecs.encoders.single().inputs.last().data)
        }
    }
    @Test fun everyPcmRoleIsAcceptedAndMixedWithoutDucking() {
        Fixture().use { f ->
            for ((stream, role) in AudioRole.entries.withIndex()) {
                f.format(stream, role, AudioCodec.PCM_S16LE); f.offer(pcm(2048, 1000), stream)
            }
            f.tick(0); f.tick(1); f.tick(2)
            assertTrue(f.codecs.decoders.isEmpty())
            assertArrayEquals(pcm(1024, 5000), f.codecs.encoders.single().inputs[2].data)
        }
    }
    @Test fun sixteenKhzMonoResamplesAcrossPacketsAndShortAlertsStillPlay() {
        Fixture().use { f ->
            f.format(2, AudioRole.VOICE, AudioCodec.PCM_S16LE, 16_000, 1)
            val mono = ByteArray(960 * 2).also { bytes ->
                for (i in 0 until 960) { bytes[i * 2] = i.toByte(); bytes[i * 2 + 1] = (i shr 8).toByte() }
            }
            f.offer(mono.copyOfRange(0, 640), 2); f.offer(mono.copyOfRange(640, mono.size), 2)
            f.tick(0); f.tick(1); f.tick(2)
            val encoded = f.codecs.encoders.single().inputs[2].data
            val expected = ByteArray(4096)
            for (i in 0 until 1024) {
                val value = i / 3
                for (channel in 0..1) { expected[i * 4 + channel * 2] = value.toByte(); expected[i * 4 + channel * 2 + 1] = (value shr 8).toByte() }
            }
            assertArrayEquals(expected, encoded)
            f.mixer.format(2, null); f.mixer.step(70_000_000)
            f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE)
            f.offer(pcm(200, 900), 1); f.mixer.step(80_000_000)
            f.mixer.step(123_000_000)
            assertArrayEquals(pcm(200, 900) + ByteArray((1024 - 200) * 4), f.codecs.encoders.last().inputs.last().data)
        }
    }
    @Test fun allSupportedPcmRatesAndChannelCountsPlay() {
        for (rate in listOf(8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000)) for (channels in 1..2) {
            Fixture().use { f ->
                f.format(1, AudioRole.TELEPHONY, AudioCodec.PCM_S16LE, rate, channels)
                val frames = rate / 10
                val bytes = if (channels == 2) pcm(frames, 1234) else ByteArray(frames * 2).also { mono ->
                    for (i in mono.indices step 2) { mono[i] = 0xd2.toByte(); mono[i + 1] = 4 }
                }
                f.offer(bytes, 1); f.tick(0); f.tick(2)
                assertArrayEquals("rate=$rate channels=$channels", pcm(1024, 1234), f.codecs.encoders.single().inputs.last().data)
            }
        }
    }
    @Test fun pcmFifoCapsAt300msAndKeepsTheNewestFrames() {
        Fixture().use { f ->
            f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE)
            f.offer(pcm(2000, 111) + pcm(14_400, 222), 1)
            f.tick(0)
            assertEquals(2000L, f.mixer.droppedFrames[1] ?: 0)
            f.tick(2)
            assertArrayEquals(pcm(1024, 222), f.codecs.encoders.single().inputs.last().data)
        }
    }
    @Test fun receiveQueueAlsoDropsOldestBeforeTheWorkerRuns() {
        Fixture().use { f ->
            f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE)
            repeat(20) { f.offer(pcm(1024, it + 1), 1) }
            f.tick(0); f.tick(2)
            assertEquals(6 * 1024L, f.mixer.droppedFrames[1] ?: 0)
            assertArrayEquals(pcm(1024, 7), f.codecs.encoders.single().inputs.last().data)
        }
    }
    @Test fun sessionChangeAndEndedClearQueuedPcmAndCodecsAndFenceLatePackets() {
        Fixture().use { f ->
            f.format(); f.offer(pcm(4096, 9000)); f.tick(0)
            val oldDecoder = f.codecs.decoders.single(); val oldEncoder = f.codecs.encoders.single()
            f.offer(pcm(2048, 8000))
            f.mixer.session(SessionState(SessionId(2), SessionPhase.ACTIVE), configuration)
            f.format(1, AudioRole.TELEPHONY, AudioCodec.PCM_S16LE)
            f.offer(pcm(2048, 32000), 1, session = 1)
            f.offer(pcm(2048, 400), 1, session = 2)
            f.mixer.step(100_000_000); f.mixer.step(143_000_000)
            assertTrue(oldDecoder.closed); assertTrue(oldEncoder.closed)
            assertArrayEquals(pcm(1024, 400), f.codecs.encoders.last().inputs.last().data)
            assertEquals(SessionId(2), f.inner.packets.last().session)
            f.mixer.session(SessionState(SessionId(2), SessionPhase.ENDED), configuration)
            f.offer(pcm(2048, 500), 1, session = 2)
            f.mixer.step(200_000_000)
            val count = f.inner.packets.size
            f.mixer.step(300_000_000)
            assertEquals(count, f.inner.packets.size)
            assertEquals(CastAudioMix.MIX_STREAM to null, f.inner.formats.last())
            assertTrue(f.codecs.encoders.all { it.closed })
        }
    }
    @Test fun lastStreamEndingSendsNullAndIdleCarDirectDoesNotStartACodec() {
        Fixture().use { f ->
            f.tick(0); f.tick(10)
            assertTrue(f.codecs.encoders.isEmpty()); assertTrue(f.inner.formats.isEmpty())
            f.format(); f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE); f.tick(11)
            f.mixer.format(0, null); f.tick(12)
            assertEquals(1, f.inner.formats.size)
            f.mixer.format(1, null); f.tick(13)
            assertEquals(CastAudioMix.MIX_STREAM to null, f.inner.formats.last())
            val count = f.inner.packets.size
            f.tick(14); f.tick(15)
            assertEquals(count, f.inner.packets.size)
            assertEquals(2, f.inner.formats.size)
            assertTrue(f.codecs.encoders.single().closed)
        }
    }
    @Test fun aSessionChangeDuringDecodeDiscardsTheOldWorkerSnapshot() {
        Fixture().use { f ->
            f.codecs.onDecoderOffer = { f.mixer.session(SessionState(SessionId(2), SessionPhase.ACTIVE), configuration) }
            f.format(); f.offer(pcm(4096, 9000)); f.tick(0)
            assertTrue(f.inner.formats.isEmpty()); assertTrue(f.inner.packets.isEmpty())
            f.format(1, AudioRole.TELEPHONY, AudioCodec.PCM_S16LE); f.offer(pcm(2048, 400), 1, session = 2)
            f.tick(1); f.tick(3)
            assertTrue(f.codecs.decoders.single().closed)
            assertArrayEquals(pcm(1024, 400), f.codecs.encoders.single().inputs.last().data)
            assertTrue(f.inner.packets.all { it.session == SessionId(2) })
        }
    }
    @Test fun unsupportedFormatsAreIgnoredWithOneDiagnosticPerShape() {
        Fixture().use { f ->
            val unsupported = listOf(
                AudioFormat(0, AudioRole.MEDIA, AudioCodec.OPUS, 48000, 2),
                AudioFormat(0, AudioRole.MEDIA, AudioCodec.AAC_LC, 44100, 2, byteArrayOf(0x11, 0x90.toByte())),
                AudioFormat(0, AudioRole.MEDIA, AudioCodec.AAC_LC, 48000, 2, byteArrayOf(1)),
                AudioFormat(0, AudioRole.ALERT, AudioCodec.AAC_LC, 48000, 2, byteArrayOf(0x11, 0x90.toByte())),
                AudioFormat(0, AudioRole.ALERT, AudioCodec.PCM_S16LE, 96000, 2),
                AudioFormat(0, AudioRole.ALERT, AudioCodec.PCM_S16LE, 48000, 3))
            for ((index, format) in unsupported.withIndex()) repeat(2) { iteration ->
                f.mixer.format(0, format); f.offer(byteArrayOf(1, 2, 3, 4)); f.mixer.step(index * 2L + iteration)
            }
            assertEquals(unsupported.size, f.diagnostics.size)
            assertTrue(f.inner.formats.isEmpty()); assertTrue(f.inner.packets.isEmpty())
            assertTrue(f.codecs.encoders.isEmpty()); assertTrue(f.codecs.decoders.isEmpty())
        }
    }
    @Test fun decoderFailureReleasesAndRetriesOnNextPacketWithoutStoppingTheMix() {
        Fixture().use { f ->
            f.format(); f.offer(pcm(2048, 10)); f.tick(0)
            val failed = f.codecs.decoders.single(); failed.fail = true
            f.tick(1)
            assertTrue(failed.closed); assertEquals(1, f.diagnostics.size)
            f.tick(2); assertEquals(1, f.codecs.decoders.size)
            f.offer(pcm(2048, 500)); f.tick(3); f.tick(5)
            assertEquals(2, f.codecs.decoders.size)
            assertArrayEquals(pcm(1024, 500), f.codecs.encoders.single().inputs.last().data)
            assertEquals(1, f.diagnostics.size)
        }
    }
    @Test fun encoderFailureReleasesAndRetriesOnlyOnNextPacket() {
        Fixture().use { f ->
            f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE); f.offer(pcm(4096, 10), 1); f.tick(0)
            val failed = f.codecs.encoders.single(); failed.fail = true
            f.tick(1)
            assertTrue(failed.closed); assertEquals(1, f.diagnostics.size)
            f.tick(2); f.tick(3); assertEquals(1, f.codecs.encoders.size)
            f.offer(pcm(2048, 600), 1); f.tick(4); f.tick(6)
            assertEquals(2, f.codecs.encoders.size)
            assertArrayEquals(pcm(1024, 600), f.codecs.encoders.last().inputs.last().data)
            assertEquals(1, f.diagnostics.size)
        }
    }
    @Test fun aDelayedWorkerResumesOnTheMonotonicTickWithoutACatchupBurst() {
        Fixture().use { f ->
            f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE)
            f.tick(0); f.tick(10)
            assertEquals(2, f.inner.packets.size)
            assertEquals(10 * 1024L * 1_000_000 / 48_000, f.inner.packets.last().ptsUs)
            f.tick(10)
            assertEquals(2, f.inner.packets.size)
            f.tick(11)
            assertEquals(3, f.inner.packets.size)
        }
    }
    @Test fun unavailableCodecsWaitForAnotherPacketAndDiagnoseOnlyOnce() {
        Fixture().use { f ->
            f.codecs.failDecoder = true; f.codecs.failEncoder = true
            f.format(); f.offer(pcm(2048, 700)); f.tick(0); f.tick(1)
            assertEquals(2, f.diagnostics.size)
            f.offer(pcm(2048, 700)); f.tick(2)
            assertEquals(2, f.diagnostics.size)
            f.codecs.failDecoder = false; f.codecs.failEncoder = false
            f.tick(3)
            assertTrue(f.codecs.encoders.isEmpty()); assertTrue(f.codecs.decoders.isEmpty())
            f.offer(pcm(2048, 700)); f.tick(4); f.tick(6)
            assertArrayEquals(pcm(1024, 700), f.codecs.encoders.single().inputs.last().data)
        }
    }
    @Test fun formatReplacementClearsOnlyThatStreamsPcmAndResampler() {
        Fixture().use { f ->
            f.format(); f.offer(pcm(4096, 1000))
            f.format(1, AudioRole.ALERT, AudioCodec.PCM_S16LE); f.offer(pcm(4096, 9000), 1); f.tick(0)
            f.format(1, AudioRole.VOICE, AudioCodec.PCM_S16LE, 16000, 1)
            f.tick(2)
            assertArrayEquals(pcm(1024, 1000), f.codecs.encoders.single().inputs.last().data)
            assertFalse(f.codecs.decoders.single().closed)
        }
    }
    @Test fun callbacksOnlyEnqueueAndInnerCallsNeverHoldTheInboxLock() {
        Fixture().use { f ->
            f.inner.onCall = {
                CompletableFuture.runAsync {
                    f.mixer.format(2, AudioFormat(2, AudioRole.OTHER, AudioCodec.PCM_S16LE, 48000, 2))
                }.get(1, TimeUnit.SECONDS)
            }
            f.mixer.session(SessionState(SessionId(1), SessionPhase.ACTIVE), configuration)
            f.format(); f.offer(pcm(2048, 1))
            assertTrue(f.inner.sessions.isEmpty()); assertTrue(f.inner.formats.isEmpty()); assertTrue(f.codecs.decoders.isEmpty())
            f.tick(0); f.tick(2)
            f.mixer.session(SessionState(SessionId(1), SessionPhase.ENDED), configuration); f.tick(3)
            assertEquals(CastAudioMix.MIX_STREAM to null, f.inner.formats.last())
        }
    }
    @Test fun wrapperDelegatesVideoAndOwnsOneWorkerPerOpen() {
        val inner = Capture(); val codecs = Codecs(); val wrapper = CastAudioMix(inner, codecs)
        val host = object : OutputHost {
            override fun capabilities(capabilities: OutputCapabilities) = Unit
            override fun control(event: ControlEvent) = Unit
            override fun stats(stats: OutputStats) = Unit
        }
        try {
            wrapper.open(host)
            assertEquals(inner.id, wrapper.id)
            // The cast output drops video of an unknown session: the session must arrive first, on the caller.
            val caller = Thread.currentThread()
            inner.onCall = { assertSame(caller, Thread.currentThread()) }
            wrapper.onSession(SessionState(SessionId(1), SessionPhase.ACTIVE), configuration)
            assertEquals(1, inner.sessions.size)
            inner.onCall = {}
            val frame = VideoFrame(ScreenId.MAIN, SessionId(1), byteArrayOf(1), 0, true, 0)
            wrapper.onVideoFormat(ScreenId.MAIN, null); wrapper.onVideoFrame(frame)
            assertSame(frame, inner.videos.single()); assertNull(inner.videoFormats.single())
            wrapper.close()
            assertTrue(inner.closed.await(2, TimeUnit.SECONDS))
            assertTrue(codecs.encoders.isEmpty()); assertTrue(codecs.decoders.isEmpty())
        } finally { wrapper.close() }
    }
    @Test fun aFormatOnlyAnnouncementWakesTheIdleWorkerAndCloseReleasesItsCodec() {
        val inner = Capture(); val codecs = Codecs(); val wrapper = CastAudioMix(inner, codecs)
        val host = object : OutputHost {
            override fun capabilities(capabilities: OutputCapabilities) = Unit
            override fun control(event: ControlEvent) = Unit
            override fun stats(stats: OutputStats) = Unit
        }
        try {
            wrapper.open(host)
            wrapper.onSession(SessionState(SessionId(1), SessionPhase.ACTIVE), configuration)
            assertTrue(inner.sessionReady.await(2, TimeUnit.SECONDS))
            wrapper.onAudioFormat(1, AudioFormat(1, AudioRole.ALERT, AudioCodec.PCM_S16LE, 48000, 2))
            assertTrue(inner.formatReady.await(2, TimeUnit.SECONDS))
            wrapper.close()
            assertTrue(inner.closed.await(2, TimeUnit.SECONDS))
            assertEquals(CastAudioMix.MIX_STREAM, inner.formats.first().first)
            assertEquals(CastAudioMix.MIX_STREAM to null, inner.formats.last())
            assertTrue(codecs.encoders.all { it.closed })
        } finally { wrapper.close() }
    }
}
