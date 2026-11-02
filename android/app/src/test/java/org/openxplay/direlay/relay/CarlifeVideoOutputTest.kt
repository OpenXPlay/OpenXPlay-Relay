// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.relay

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.direlay.bridge.avcConfig
import org.openxplay.direlay.bridge.hevcConfig
import org.openxplay.direlay.video.VideoCompatibility
import org.openxplay.direlay.video.VideoPipeline
import org.openxplay.stream.core.*

class CarlifeVideoOutputTest {
    private val avc = VideoFormat.create(ScreenId.MAIN, VideoCodec.H264, avcConfig, 30.0)
    private val hevc = VideoFormat.create(ScreenId.MAIN, VideoCodec.H265, hevcConfig, 60.0)
    private val session = SessionState(SessionId(7), SessionPhase.ACTIVE)
    private val configuration = SessionConfig(mapOf(ScreenId.MAIN to ScreenConfig(VideoCodec.H265, hevc.width, hevc.height, 60.0)))
    private fun frame(codec: VideoCodec) = VideoFrame(ScreenId.MAIN, session.id,
        if (codec == VideoCodec.H264) byteArrayOf(0, 0, 0, 2, 0x65, 1) else byteArrayOf(0, 0, 0, 2, 0x26, 1),
        125_000, true, 999_000)
    private val capabilities get() = OutputCapabilities(LatencyClass.INTERACTIVE,
        listOf(DecoderSupport("avc1.640028", true)),
        mapOf(ScreenId.MAIN to DisplayInfo(avc.width, avc.height, 160, 60.0, frameSize = Viewport(avc.width, avc.height))),
        2, setOf(KeyCode.BACK), emptySet(), false)

    private class Destination(private val caps: OutputCapabilities) : ProjectionOutput {
        override val id = "carlife"
        lateinit var host: OutputHost
        val formats = mutableListOf<VideoFormat?>()
        val frames = mutableListOf<VideoFrame>()
        var closed = false
        override fun open(host: OutputHost) { this.host = host; host.capabilities(caps) }
        override fun onSession(state: SessionState, config: SessionConfig) {}
        override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) { formats += format }
        override fun onVideoFrame(frame: VideoFrame) { frames += frame }
        override fun onAudioFormat(stream: Int, format: AudioFormat?) {}
        override fun onAudioPacket(packet: AudioPacket) {}
        override fun onMicrophone(stream: Int, request: MicRequest?) {}
        override fun close() { closed = true }
    }
    private class Host : OutputHost {
        val controls = mutableListOf<ControlEvent>()
        val capabilities = mutableListOf<OutputCapabilities>()
        override fun capabilities(capabilities: OutputCapabilities) { this.capabilities += capabilities }
        override fun control(event: ControlEvent) { controls += event }
        override fun stats(stats: OutputStats) {}
    }
    private class Pipeline(val listener: VideoPipeline.Listener, val target: VideoCompatibility.Target) : VideoPipeline {
        var started = false
        var closed = false
        var keyFrames = 0
        val frames = mutableListOf<VideoFrame>()
        override fun start() { started = true }
        override fun offer(frame: VideoFrame) { frames += frame }
        override fun requestKeyFrame() { keyFrames++ }
        override fun close() { closed = true }
    }

    @Test fun hevcUsesTheAvcPipelineAndKeepsVideoClockAndScaledTouchCoordinates() {
        val destination = Destination(capabilities)
        val host = Host()
        val pipelines = mutableListOf<Pipeline>()
        val logs = mutableListOf<Pair<String, String>>()
        val output = carlifeVideoOutput(destination, VideoPipeline.Factory { source, target, listener ->
            assertSame(hevc, source)
            assertEquals(VideoCodec.H264, target.codec)
            assertEquals(Viewport(avc.width, avc.height), target.geometry!!.picture)
            Pipeline(listener, target).also { pipelines += it }
        }) { event, detail -> logs += event to detail }
        output.open(host)
        output.onSession(session, configuration)
        output.onVideoFormat(ScreenId.MAIN, hevc)
        val pipeline = pipelines.single()
        assertTrue(pipeline.started)
        assertTrue(destination.formats.all { it == null })
        val original = frame(VideoCodec.H265)
        output.onVideoFrame(original)
        assertSame(original, pipeline.frames.single())
        assertTrue(destination.frames.isEmpty())
        pipeline.listener.format(avc)
        val encoded = frame(VideoCodec.H264)
        pipeline.listener.frame(encoded)
        assertSame(avc, destination.formats.last())
        assertSame(encoded, destination.frames.single())
        assertEquals(original.ptsUs, destination.frames.single().ptsUs)
        assertEquals(original.session, destination.frames.single().session)

        val origin = Origin("carlife", "receiver")
        val contact = TouchContact(0, .75, .25, true)
        destination.host.control(ControlEvent.Touch(ScreenId.MAIN, listOf(contact), origin))
        val touch = host.controls.filterIsInstance<ControlEvent.Touch>().single()
        assertEquals(origin, touch.origin)
        assertEquals(listOf(contact), touch.contacts)
        destination.host.control(ControlEvent.Touch(ScreenId.MAIN, emptyList(), origin))
        assertTrue(host.controls.filterIsInstance<ControlEvent.Touch>().last().contacts.isEmpty())
        val upstreamRequests = host.controls.size
        destination.host.control(ControlEvent.KeyFrameRequest(ScreenId.MAIN, origin))
        assertEquals(1, pipeline.keyFrames)
        assertEquals(upstreamRequests, host.controls.size)

        assertTrue(logs.any { it.first == "video_decision" && "source=H265" in it.second && "target=H264" in it.second && "decision=TRANSCODE" in it.second })
        assertTrue(logs.any { it.first == "oxp_session" && "session=7" in it.second && "codec=H265" in it.second })
        assertTrue(logs.any { it.first == "delivered_format" && "codec=H264" in it.second })
        val before = logs.size
        repeat(5) { output.onSession(session, configuration); destination.host.capabilities(capabilities); output.onVideoFormat(ScreenId.MAIN, hevc) }
        assertEquals(before, logs.size)
        assertEquals(1, pipelines.size)
        output.onSession(session.copy(phase = SessionPhase.ENDED), configuration)
        assertTrue(pipeline.closed)
        pipeline.listener.frame(encoded)
        assertEquals(1, destination.frames.size)
        output.close()
        assertTrue(destination.closed)
        assertTrue(logs.any { it.first == "capabilities" && it.second == "none" })
    }

    @Test fun compatibleAvcPassesThroughAndFailureReportsTheRequiredConversion() {
        val direct = Destination(capabilities)
        val passthrough = carlifeVideoOutput(direct, VideoPipeline.Factory { _, _, _ -> error("AVC must not create a pipeline") })
        passthrough.open(Host())
        passthrough.onSession(session, configuration)
        passthrough.onVideoFormat(ScreenId.MAIN, avc)
        val avcFrame = frame(VideoCodec.H264)
        passthrough.onVideoFrame(avcFrame)
        assertSame(avcFrame, direct.frames.single())
        passthrough.close()

        val logs = mutableListOf<Pair<String, String>>()
        val failed = carlifeVideoOutput(Destination(capabilities), VideoPipeline.Factory { _, _, _ ->
            throw IllegalStateException("encoder_unavailable")
        }) { event, detail -> logs += event to detail }
        failed.open(Host())
        failed.onVideoFormat(ScreenId.MAIN, hevc)
        assertTrue(logs.any { it.first == "video_failed" && "error=IllegalStateException" in it.second && "encoder_unavailable" in it.second })
        assertTrue(logs.any { it.first == "video_decision" && "failed=true" in it.second && "source=H265" in it.second })
        failed.close()
    }
}
