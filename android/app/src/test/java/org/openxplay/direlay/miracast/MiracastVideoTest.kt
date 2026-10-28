// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.app.Application
import android.graphics.SurfaceTexture
import android.view.Surface
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.stream.core.*
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class MiracastVideoTest {
    private class Codec : MiracastCodec {
        override val name = "fake.decoder"
        val bytes = mutableListOf<ByteArray>()
        val timestamps = mutableListOf<Long>()
        var closed = false
        var fail = false
        var available = true
        var render = true
        var buffered = 0
        override fun offer(annexB: ByteArray, ptsUs: Long): Boolean {
            if (fail) throw IllegalStateException("broken codec")
            if (!available) return false
            bytes += annexB; timestamps += ptsUs; buffered++
            return true
        }
        override fun drain(): Int = if (render) buffered.also { buffered = 0 } else 0
        override fun close() { closed = true }
    }
    private class Fixture(holdOutput: Boolean = false) : AutoCloseable {
        val host = MiracastHost()
        val events = MiracastEvents()
        val codecs = mutableListOf<Codec>()
        val texture = SurfaceTexture(0)
        val surface = Surface(texture)
        var creationFailure: Exception? = null
        val video = MiracastVideo(host, events, "miracast", MiracastCodec.Factory { _, _ ->
            creationFailure?.let { throw it }
            Codec().also { it.render = !holdOutput; codecs += it }
        })
        init {
            video.session(SessionState(SessionId(1), SessionPhase.ACTIVE))
            video.format(avcFormat())
            video.surface(surface)
        }
        override fun close() { video.close(); video.finish(); surface.release(); texture.release() }
    }
    @Test fun decodeWaitsForAKeyframeConvertsAvccAndCountsRenderedFrames() {
        Fixture().use { f ->
            f.video.step(0)
            assertEquals(listOf(ControlEvent.KeyFrameRequest(ScreenId.MAIN, Origin("miracast", "display"))), f.host.controls)
            f.video.offer(frame())
            f.video.step(10)
            assertTrue(f.codecs.isEmpty())
            f.video.offer(frame(true, 1234))
            f.video.step(20)
            val codec = f.codecs.single()
            assertArrayEquals(hex("00 00 00 01 65 80"), codec.bytes.single())
            assertEquals(listOf(1234L), codec.timestamps)
            f.video.step(1_000_000_000)
            assertEquals(1L, f.events.statistics.single().decodedFrames)
            assertEquals(1L, f.events.statistics.single().droppedFrames)
            assertEquals(VideoCodec.H264, f.events.statistics.single().codec)
            assertEquals(listOf("decoder fake.decoder video/avc 640x480"), f.events.diagnostics)
        }
    }
    @Test fun repeatedFormatsDoNotReconfigureButChangedContentAndSurfacesDo() {
        Fixture().use { f ->
            f.video.offer(frame(true))
            f.video.step(0)
            val first = f.codecs.single()
            f.video.format(avcFormat(avcBytes.copyOf(), 60.0))
            f.video.offer(frame(pts = 2))
            f.video.step(100)
            assertSame(first, f.codecs.single())
            assertEquals(2, first.bytes.size)
            f.video.format(avcFormat(avcBytes.also { it[3] = 31 }))
            f.video.offer(frame())
            f.video.step(200)
            assertTrue(first.closed)
            assertEquals(1, f.codecs.size)
            f.video.offer(frame(true, 3))
            f.video.step(300)
            assertEquals(2, f.codecs.size)
            f.video.surface(null)
            f.video.step(400)
            assertTrue(f.codecs.last().closed)
            f.video.surface(f.surface)
            f.video.offer(frame())
            f.video.step(500)
            assertEquals(2, f.codecs.size)
            f.video.offer(frame(true, 4))
            f.video.step(600)
            assertEquals(3, f.codecs.size)
            assertEquals(1, f.host.controls.size)
            assertEquals(3, f.events.diagnostics.size)
            assertTrue(f.events.diagnostics.all { it == "decoder fake.decoder video/avc 640x480" })
        }
    }
    @Test fun decoderFailureReleasesAndRetriesOnlyWithTheNextKeyframe() {
        Fixture().use { f ->
            f.video.offer(frame(true))
            f.video.step(0)
            val first = f.codecs.single()
            first.fail = true
            f.video.offer(frame(pts = 2))
            f.video.offer(frame(pts = 3))
            f.video.step(100)
            assertTrue(first.closed)
            assertEquals(listOf("decoder_failed"), f.events.failures)
            assertEquals("decoder failed: IllegalStateException: broken codec", f.events.diagnostics.last())
            f.video.offer(frame())
            f.video.step(200)
            assertEquals(1, f.codecs.size)
            f.video.offer(frame(true, 5))
            f.video.step(300)
            assertEquals(listOf(5L), f.codecs.last().timestamps)
            assertEquals(1, f.events.diagnostics.count { it.startsWith("decoder failed:") })
            assertEquals(2, f.events.diagnostics.count { it.startsWith("decoder fake.decoder ") })
        }
    }
    @Test fun decoderCreationFailuresReportEachAttemptAndRecoverWithHevcDiagnostics() {
        Fixture().use { f ->
            f.creationFailure = IllegalArgumentException("decoder unavailable")
            f.video.offer(frame(true))
            f.video.step(0)
            f.video.step(1)
            assertTrue(f.codecs.isEmpty())
            assertEquals(listOf("decoder failed: IllegalArgumentException: decoder unavailable"), f.events.diagnostics)
            f.video.offer(frame(true, 2))
            f.video.step(2)
            assertEquals(2, f.events.diagnostics.size)
            assertEquals(listOf("decoder_failed", "decoder_failed"), f.events.failures)
            f.creationFailure = null
            val format = VideoFormat.create(ScreenId.MAIN, VideoCodec.H265, hevcBytes)
            f.video.format(format)
            f.video.offer(VideoFrame(ScreenId.MAIN, SessionId(1), hex("00 00 00 03 26 01 80"), 3, true, 0))
            f.video.step(3)
            assertEquals("decoder fake.decoder video/hevc ${format.width}x${format.height}", f.events.diagnostics.last())
            f.video.step(4)
            assertEquals(3, f.events.diagnostics.size)
        }
    }
    @Test fun decoderDiagnosticsCanReenterFromAnotherThreadWithoutHoldingTheVideoGate() {
        Fixture().use { f ->
            f.events.onDiagnostic = {
                CompletableFuture.runAsync { f.video.videoSize() }.get(2, TimeUnit.SECONDS)
            }
            f.video.offer(frame(true))
            f.video.step(0)
            f.codecs.single().fail = true
            f.video.offer(frame(pts = 2))
            f.video.step(1)
            assertEquals(listOf("decoder fake.decoder video/avc 640x480",
                "decoder failed: IllegalStateException: broken codec"), f.events.diagnostics)
        }
    }
    @Test fun overflowReconfiguresAtTheNextQueuedKeyframeAndRequestsAreRateLimited() {
        Fixture().use { f ->
            f.video.offer(frame(true))
            f.video.step(0)
            val first = f.codecs.single()
            repeat(9) { f.video.offer(frame(it == 5, 10L + it)) }
            f.video.step(100)
            assertTrue(first.closed)
            assertEquals(listOf(15L), f.codecs.last().timestamps)
            f.video.step(999_999_999)
            assertEquals(1, f.host.controls.size)
            f.video.step(1_000_000_000)
            assertEquals(2, f.host.controls.size)
            assertTrue(f.events.statistics.single().droppedFrames >= 5)
        }
    }
    @Test fun endedSessionsAndOtherScreensCannotFeedTheDecoder() {
        Fixture().use { f ->
            f.video.offer(frame(true, screen = ScreenId.CLUSTER))
            f.video.offer(frame(true, session = 99))
            f.video.step(0)
            assertTrue(f.codecs.isEmpty())
            f.video.offer(frame(true))
            f.video.step(1)
            f.video.session(SessionState(SessionId(1), SessionPhase.ENDED))
            assertEquals(Viewport(0, 0), f.video.videoSize())
            f.video.format(avcFormat())
            f.video.offer(frame(true))
            f.video.step(2)
            assertTrue(f.codecs.single().closed)
            f.video.session(SessionState(SessionId(2), SessionPhase.ACTIVE))
            f.video.format(avcFormat())
            f.video.offer(frame(true, session = 1))
            f.video.offer(frame(true, session = 2))
            f.video.step(3)
            assertEquals(2, f.codecs.size)
        }
    }
    @Test fun aBusyCodecKeepsTheInputUntilItCanAcceptItAndCloseRejectsLateFrames() {
        Fixture().use { f ->
            f.video.offer(frame(true))
            f.video.step(0)
            val codec = f.codecs.single()
            codec.available = false
            f.video.offer(frame(pts = 2))
            f.video.step(1)
            assertEquals(listOf(1L), codec.timestamps)
            codec.available = true
            f.video.step(2)
            assertEquals(listOf(1L, 2L), codec.timestamps)
            f.video.close()
            f.video.offer(frame(true, 3))
            f.video.step(3)
            f.video.finish()
            assertTrue(codec.closed)
            assertEquals(2, codec.timestamps.size)
        }
    }
    @Test fun aStaticScreenWithPendingOutputKeepsTheDecoderAfterFiveSeconds() {
        Fixture(holdOutput = true).use { f ->
            f.video.offer(frame(true))
            f.video.step(0)
            val codec = f.codecs.single()
            assertEquals(1, codec.buffered)
            f.video.step(6_000_000_000)
            f.video.step(60_000_000_000)
            assertTrue(f.events.failures.isEmpty())
            assertFalse(codec.closed)
            assertSame(codec, f.codecs.single())
            f.video.offer(frame(pts = 2))
            f.video.step(60_000_000_001)
            assertEquals(listOf(1L, 2L), codec.timestamps)
            codec.render = true
            f.video.step(61_000_000_001)
            assertEquals(2L, f.events.statistics.last().decodedFrames)
        }
    }
    @Test fun aQueuedFrameRefusedForFiveSecondsReleasesTheDecoderAndRecoversAtAKeyframe() {
        Fixture().use { f ->
            f.video.offer(frame(true))
            f.video.step(0)
            val codec = f.codecs.single()
            codec.available = false
            f.video.offer(frame(pts = 2))
            f.video.step(100)
            f.video.step(5_000_000_099)
            assertTrue(f.events.failures.isEmpty())
            f.video.step(5_000_000_100)
            assertEquals(listOf("decoder_failed"), f.events.failures)
            assertTrue(codec.closed)
            f.video.offer(frame(true, 3))
            f.video.step(5_000_000_101)
            assertEquals(listOf(3L), f.codecs.last().timestamps)
        }
    }
    @Test fun acceptingAQueuedFrameResetsTheRefusalTimeout() {
        Fixture().use { f ->
            f.video.offer(frame(true))
            f.video.step(0)
            val codec = f.codecs.single()
            codec.available = false
            f.video.offer(frame(pts = 2))
            f.video.step(1)
            codec.available = true
            f.video.step(4_000_000_001)
            codec.available = false
            f.video.offer(frame(pts = 3))
            f.video.step(4_000_000_002)
            f.video.step(8_000_000_002)
            assertTrue(f.events.failures.isEmpty())
            assertFalse(codec.closed)
        }
    }
    @Test fun hostCallbacksCanReenterFromAnotherThreadWithoutHoldingTheVideoGate() {
        Fixture().use { f ->
            f.host.onControl = {
                CompletableFuture.runAsync { f.video.format(avcFormat()); f.video.offer(frame(true)) }.get(2, TimeUnit.SECONDS)
            }
            f.video.step(0)
            f.video.step(1)
            assertEquals(1, f.codecs.size)
        }
    }
}
