// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.bridge

import com.shilapi.xcertplay.airplay.*
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.stream.core.ScreenId
import org.openxplay.stream.core.SessionPhase
import org.openxplay.stream.core.VideoCodec as StreamCodec
import java.lang.reflect.Proxy
import java.net.InetAddress

class StreamBridgeSinkTest {
    @Test fun readsTheAppliedRateWhenCreatingEachFormatInsteadOfAtSinkConstruction() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            var fps = 30.0
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true }, fpsHint = { fps })
            fps = 45.0
            bridge.onVideoCodec(110, VideoCodec.H264)
            bridge.onVideoConfig(110, avcConfig)
            assertEquals(45.0, capture.formats.mapNotNull { it.second }.last().fpsHint)
            fps = 60.0
            bridge.onVideoCodec(111, VideoCodec.H265)
            bridge.onVideoConfig(111, hevcConfig)
            assertEquals(60.0, capture.formats.mapNotNull { it.second }.last().fpsHint)
        }
    }

    @Test fun anEngineThatNeverSawAnIphoneLeavesNoSessionBehind() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            // CarPlay started and stopped without an iPhone: the decorated sink gets no callbacks.
            StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            assertTrue(capture.states.isEmpty())
            val next = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            next.onScreenStreamActive(110, true)
            assertEquals(listOf(SessionPhase.CONNECTING), capture.states.map { it.phase })
        }
    }
    @Test fun convertsFramesAndPreservesClocksForBothScreensAndCodecs() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true }, fpsHint = { 60.0 })
            assertTrue(capture.states.isEmpty())
            bridge.onVideoCodec(110, VideoCodec.H264)
            assertEquals(listOf(SessionPhase.CONNECTING), capture.states.map { it.phase })
            bridge.onVideoConfig(110, avcConfig)
            bridge.onVideoFrame(110, hex("00 00 01 65 88 00 00 00 01 41 99"), 1_234_567, 9_876_543)
            bridge.onVideoFrame(110, hex("00 00 00 01 41 88"), 2_000_000, 3_000_000)
            bridge.onVideoCodec(111, VideoCodec.H265)
            bridge.onVideoConfig(111, hevcConfig)
            bridge.onVideoFrame(111, hex("00 00 01 26 01 88"), 4_000_000, 5_000_000)
            assertEquals(listOf(SessionPhase.CONNECTING, SessionPhase.ACTIVE), capture.states.map { it.phase })
            val frame = capture.frames[0]
            assertEquals(ScreenId.MAIN, frame.screen)
            assertArrayEquals(hex("00 00 00 02 65 88 00 00 00 02 41 99"), frame.data)
            assertTrue(frame.keyFrame)
            // Video timestamps move into the relay's clock (anchored at arrival) and keep the phone's spacing.
            assertEquals(9876L, frame.ptsUs)
            assertEquals(9876L + (2_000_000 - 1_234_567) / 1000, capture.frames[1].ptsUs)
            assertEquals(9_876_543L, frame.receivedAtNanos)
            assertEquals(capture.states.first().id, frame.session)
            assertFalse(capture.frames[1].keyFrame)
            assertEquals(ScreenId.CLUSTER, capture.frames[2].screen)
            assertTrue(capture.frames[2].keyFrame)
            val formats = capture.formats.mapNotNull { it.second }
            assertEquals(listOf(StreamCodec.H264, StreamCodec.H265), formats.map { it.codec })
            assertEquals(60.0, formats[0].fpsHint)
            assertEquals(640, formats[0].width)
            assertEquals(480, formats[0].height)
        }
    }

    @Test fun replacementAndMainStopFenceAllOldCallbacks() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val first = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            first.onVideoCodec(110, VideoCodec.H264)
            first.onVideoConfig(110, avcConfig)
            val second = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            // The replacement fences the old session at once and is announced with its first stream.
            assertEquals(listOf(SessionPhase.CONNECTING, SessionPhase.ACTIVE, SessionPhase.ENDED), capture.states.map { it.phase })
            val before = capture.formats.size
            first.onVideoConfig(110, avcConfig)
            first.onVideoFrame(110, hex("00 00 01 65 88"), 0, 0)
            first.onScreenStreamActive(110, false)
            assertEquals(before, capture.formats.size)
            assertTrue(capture.frames.isEmpty())
            second.onVideoCodec(111, VideoCodec.H265)
            assertEquals(SessionPhase.CONNECTING, capture.states[3].phase)
            assertTrue(capture.states[3].id.value > capture.states.first().id.value)
            second.onVideoConfig(111, hevcConfig)
            second.onScreenStreamActive(111, false)
            assertEquals(ScreenId.CLUSTER to null, capture.formats.last())
            assertEquals(SessionPhase.ACTIVE, capture.states.last().phase)
            second.onScreenStreamActive(110, false)
            assertEquals(SessionPhase.ENDED, capture.states.last().phase)
            val stopped = capture.states.size to capture.formats.size
            second.onVideoConfig(111, hevcConfig)
            second.onScreenStreamActive(110, false)
            assertEquals(stopped, capture.states.size to capture.formats.size)
        }
    }

    @Test fun mainStreamCanReconnectWithinItsDecoratedEngine() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            bridge.onVideoCodec(110, VideoCodec.H264)
            bridge.onVideoConfig(110, avcConfig)
            bridge.onScreenStreamActive(110, false)
            val oldId = capture.states.last().id
            bridge.setVideoRecoveryHandler(110) {}
            bridge.onScreenStreamActive(110, true)
            bridge.onVideoCodec(110, VideoCodec.H264)
            bridge.onVideoConfig(110, avcConfig)
            bridge.onVideoFrame(110, hex("00 00 01 65 88"), 1000, 2000)
            assertEquals(listOf(SessionPhase.CONNECTING, SessionPhase.ACTIVE, SessionPhase.ENDED,
                SessionPhase.CONNECTING, SessionPhase.ACTIVE), capture.states.map { it.phase })
            assertTrue(capture.frames.single().session.value > oldId.value)
            input.stop()
            val count = capture.states.size
            bridge.onScreenStreamActive(110, true)
            bridge.setVideoRecoveryHandler(110) {}
            bridge.onVideoConfig(110, avcConfig)
            assertEquals(count, capture.states.size)
        }
    }

    @Test fun configurationIsOwnedAndHubReceivesConvertedFrames() {
        DiPlayInput().use { input ->
            val hub = org.openxplay.stream.core.ProjectionHub(input)
            val output = org.openxplay.stream.core.RecordingOutput()
            hub.addOutput(output)
            hub.start()
            try {
                val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
                val config = avcConfig
                bridge.onVideoCodec(110, VideoCodec.H264)
                bridge.onVideoConfig(110, config)
                config.fill(0)
                assertArrayEquals(avcConfig, output.videoFormats.last().second!!.config)
                bridge.onVideoFrame(110, hex("00 00 01 65 88"), 1000, 2000)
                assertEquals(1, output.frames.size)
                val late = org.openxplay.stream.core.RecordingOutput("late")
                hub.addOutput(late)
                assertEquals(SessionPhase.ACTIVE, late.sessions.last().first.phase)
                assertEquals(StreamCodec.H264, late.videoFormats.last().second!!.codec)
                hub.stop()
                bridge.onVideoFrame(110, hex("00 00 01 65 88"), 2000, 3000)
                assertEquals(1, output.frames.size)
            } finally { hub.stop() }
        }
    }

    @Test fun malformedConfigurationAndFramesAreReportedAndNotPublished() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            val failures = mutableListOf<Int>()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true },
                reportInvalid = { type, _ -> failures.add(type) })
            bridge.onVideoConfig(110, byteArrayOf(1))
            bridge.onVideoCodec(110, VideoCodec.H264)
            bridge.onVideoConfig(110, byteArrayOf(1))
            bridge.onVideoFrame(110, hex("00 00 01 65 88"), 0, 0)
            assertEquals(SessionPhase.CONNECTING, capture.states.last().phase)
            assertTrue(capture.frames.isEmpty())
            bridge.onVideoConfig(110, avcConfig)
            bridge.onVideoFrame(110, byteArrayOf(1, 2, 3), 0, 0)
            bridge.onVideoConfig(110, byteArrayOf())
            bridge.onVideoFrame(110, hex("00 00 01 65 88"), 0, 0)
            assertEquals(listOf(110, 110, 110), failures)
            assertEquals(1, capture.frames.size)
            assertNotNull(capture.formats.last().second)
            bridge.onVideoConfig(110, avcConfig)
            bridge.onVideoFrame(110, hex("00 00 01 65 88"), 0, 0)
            assertEquals(2, capture.frames.size)
        }
    }

    @Test fun emptyAndMislabelledConfigurationsWaitAndCorrectThePreviewCodec() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            val failures = mutableListOf<Int>()
            val codecs = mutableListOf<VideoCodec>()
            val delegate = object : MediaSink {
                override fun onVideoCodec(type: Int, codec: VideoCodec) { codecs.add(codec) }
            }
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, delegate, SessionControl { true }, localPreview = { true },
                reportInvalid = { type, _ -> failures.add(type) })
            bridge.onVideoCodec(110, VideoCodec.H265)
            val count = capture.formats.size
            bridge.onVideoConfig(110, byteArrayOf())
            assertEquals(count, capture.formats.size)
            assertTrue(failures.isEmpty())
            bridge.onVideoConfig(110, avcConfig)
            assertEquals(StreamCodec.H264, capture.formats.last().second!!.codec)
            assertEquals(VideoCodec.H264, codecs.last())
            bridge.onVideoCodec(111, VideoCodec.H264)
            bridge.onVideoConfig(111, hevcConfig)
            assertEquals(StreamCodec.H265, capture.formats.last().second!!.codec)
            assertEquals(VideoCodec.H265, codecs.last())
            assertTrue(failures.isEmpty())
        }
    }

    @Test fun rejectsConfigurationsWithNonCanonicalNalLengths() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            val failures = mutableListOf<Int>()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true },
                reportInvalid = { type, _ -> failures.add(type) })
            bridge.onVideoCodec(110, VideoCodec.H264)
            bridge.onVideoConfig(110, avcConfig.also { it[4] = 0xfd.toByte() })
            bridge.onVideoFrame(110, hex("00 00 01 65 88"), 1000, 2000)
            assertEquals(listOf(110), failures)
            assertTrue(capture.frames.isEmpty())
            assertEquals(SessionPhase.CONNECTING, capture.states.last().phase)
        }
    }

    @Test fun forwardsUntranslatedCallbacksAndEnablesPreviewMidSession() {
        DiPlayInput().use { input ->
            val calls = mutableListOf<Pair<String, List<Any?>>>()
            val delegate = Proxy.newProxyInstance(MediaSink::class.java.classLoader, arrayOf(MediaSink::class.java)) { _, method, args ->
                calls.add(method.name to args.orEmpty().toList()); null
            } as MediaSink
            var preview = false
            input.start(configuration, CapturingInputSink())
            val bridge = StreamBridgeSink(input, delegate, SessionControl { true }, localPreview = { preview })
            val bytes = hex("00 00 01 65 88")
            bridge.onVideoCodec(110, VideoCodec.H264)
            bridge.onVideoConfig(110, avcConfig)
            bridge.onVideoFrame(110, bytes, 1000, 2000)
            assertTrue(calls.isEmpty())
            val audio = AudioStreamId(103, "media")
            val format = AudioFormat(AudioCodecKind.LPCM, 48_000, 2, 96)
            val mic = MicrophoneConfig("speech", 16_000, 1, 97, 20,
                InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)), 7000, bytes)
            val recovery: () -> Unit = {}
            val diagnostic: (String) -> Unit = {}
            bridge.setVideoRecoveryHandler(110, recovery)
            bridge.setVideoDiagnosticHandler(110, diagnostic)
            bridge.onScreenStreamActive(110, true)
            bridge.onAudioStarted(audio, format, 42)
            bridge.onAudioRtp(audio, format, bytes, 43)
            bridge.onAudioStopped(audio)
            bridge.onMicrophoneStarted(audio, mic)
            bridge.onMicrophoneStopped(audio)
            bridge.onIapMessage(bytes)
            assertEquals(listOf(
                "setVideoRecoveryHandler" to listOf(110, recovery),
                "setVideoDiagnosticHandler" to listOf(110, diagnostic),
                "onScreenStreamActive" to listOf(110, true),
                "onIapMessage" to listOf(bytes),
            ), calls)
            calls.clear()
            preview = true
            bridge.onVideoFrame(110, bytes, 1000, 2000)
            assertEquals(listOf("onVideoCodec", "onVideoConfig", "onVideoFrame"), calls.map { it.first })
            assertArrayEquals(avcConfig, calls[1].second[1] as ByteArray)
            assertEquals(listOf(110, bytes, 1000L, 2000L), calls[2].second)
            calls.clear()
            bridge.onVideoFrame(110, bytes)
            assertEquals(listOf("onVideoFrame"), calls.map { it.first })
            assertEquals(listOf(110, bytes), calls[0].second)
            calls.clear()
            preview = false
            bridge.onVideoFrame(110, bytes, 1000, 2000)
            assertTrue(calls.isEmpty())
            preview = true
            bridge.onVideoFrame(110, bytes, 1000, 2000)
            assertEquals(listOf("onVideoCodec", "onVideoConfig", "onVideoFrame"), calls.map { it.first })
        }
    }
}
