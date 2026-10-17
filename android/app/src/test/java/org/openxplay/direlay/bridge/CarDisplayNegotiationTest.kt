// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.bridge

import com.shilapi.xcertplay.airplay.MediaSink
import com.shilapi.xcertplay.airplay.SessionControl
import com.shilapi.xcertplay.airplay.VideoCodec as DiPlayCodec
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.direlay.relay.CarDisplayPolicy
import org.openxplay.direlay.relay.RelayStatus
import org.openxplay.direlay.settings.DisplayProfile
import org.openxplay.stream.core.*

class CarDisplayNegotiationTest {
    @Test fun inputRecordsMainStartAndPendingReconfigureWithoutAnnouncingASession() {
        DiPlayInput().use { input ->
            val sink = CapturingInputSink()
            assertNull(input.mainScreen.value)
            input.start(configuration, sink)
            assertEquals(configuration.screens[ScreenId.MAIN], input.mainScreen.value)
            val main = ScreenConfig(VideoCodec.H264, 1920, 720, 30.0)
            val changed = SessionConfig(mapOf(ScreenId.MAIN to main, ScreenId.CLUSTER to main.copy(width = 800)))
            assertEquals(ReconfigureResult.RESTART_SESSION, input.reconfigure(changed))
            assertEquals(main, input.mainScreen.value)
            assertTrue(sink.states.isEmpty())
            input.start(configuration, sink)
            assertEquals(configuration.screens[ScreenId.MAIN], input.mainScreen.value)
            input.reconfigure(SessionConfig(emptyMap()))
            assertNull(input.mainScreen.value)
            assertTrue(sink.states.isEmpty())
        }
    }

    @Test fun aCarMatchingThePolicyDefaultIsStillRememberedAsARealDisplay() {
        DiPlayInput().use { input ->
            val policy = CarDisplayPolicy()
            val hub = ProjectionHub(input, policy = policy)
            try {
                hub.start()
                assertNull(policy.lastCarDisplay)
                hub.addOutput(CarOutput("web", 1280, 720))
                assertEquals(DisplayProfile(1280, 720, 160), policy.carDisplay)
                assertEquals(policy.carDisplay, policy.lastCarDisplay)
                assertEquals(1280, input.mainScreen.value?.width)
            } finally { hub.stop() }
        }
    }

    @Test fun localPreviewAndStatusNeverOwnDisplayAndTheFirstCarStillWins() {
        DiPlayInput().use { input ->
            val policy = CarDisplayPolicy()
            val hub = ProjectionHub(input, policy = policy)
            var previewConfigs = 0
            var previewEnabled = true
            try {
                hub.addOutput(RelayStatus())
                hub.start()
                val bridge = StreamBridgeSink(input, object : MediaSink {
                    override fun onVideoConfig(type: Int, codecData: ByteArray) { previewConfigs++ }
                }, SessionControl { true }, localPreview = { previewEnabled })
                bridge.onVideoCodec(110, DiPlayCodec.H264)
                bridge.onVideoConfig(110, avcConfig)
                assertEquals(1, previewConfigs)
                assertNull(policy.carDisplay)
                assertNull(policy.lastCarDisplay)
                assertEquals(1280, input.mainScreen.value?.width)
                val first = CarOutput("web", 1920, 720)
                hub.addOutput(first)
                assertEquals(DisplayProfile(1920, 720, 240), policy.carDisplay)
                assertEquals(1920, input.mainScreen.value?.width)
                hub.addOutput(CarOutput("clink", 800, 480))
                assertEquals(DisplayProfile(1920, 720, 240), policy.carDisplay)
                previewEnabled = false
                bridge.onVideoConfig(110, avcConfig)
                assertEquals(1, previewConfigs)
                assertEquals(DisplayProfile(1920, 720, 240), policy.carDisplay)
                hub.removeOutput("web")
                assertEquals(DisplayProfile(800, 480, 160), policy.carDisplay)
                hub.removeOutput("clink")
                assertNull(policy.carDisplay)
                assertEquals(DisplayProfile(800, 480, 160), policy.lastCarDisplay)
                // Returning to the original configuration cancels the pending restart.
                assertNull(hub.pendingConfiguration)
            } finally { hub.stop() }
        }
    }

    private class CarOutput(override val id: String, val width: Int, val height: Int) : ProjectionOutput {
        override fun open(host: OutputHost) = host.capabilities(OutputCapabilities(
            LatencyClass.INTERACTIVE, listOf(DecoderSupport("avc1.640028", true)),
            mapOf(ScreenId.MAIN to DisplayInfo(width, height, 160, 60.0)),
            5, emptySet(), emptySet(), false))
        override fun onSession(state: SessionState, config: SessionConfig) = Unit
        override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) = Unit
        override fun onVideoFrame(frame: VideoFrame) = Unit
        override fun onAudioFormat(stream: Int, format: AudioFormat?) = Unit
        override fun onAudioPacket(packet: AudioPacket) = Unit
        override fun onMicrophone(stream: Int, request: MicRequest?) = Unit
        override fun close() = Unit
    }
}
