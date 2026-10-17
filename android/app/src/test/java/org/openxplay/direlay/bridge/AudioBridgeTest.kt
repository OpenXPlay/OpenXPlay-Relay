// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.bridge

import com.shilapi.xcertplay.airplay.MediaSink
import com.shilapi.xcertplay.airplay.AudioStreamId
import com.shilapi.xcertplay.airplay.AudioCodecKind
import com.shilapi.xcertplay.airplay.MicrophoneConfig
import com.shilapi.xcertplay.airplay.SessionControl
import com.shilapi.xcertplay.airplay.AudioFormat as CarPlayAudioFormat
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.stream.core.*
import java.net.InetAddress
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.CopyOnWriteArrayList

class AudioBridgeTest {
    private class Capture : InputSink {
        val audio = CopyOnWriteArrayList<Pair<Int, AudioFormat?>>()
        val mic = CopyOnWriteArrayList<Pair<Int, MicRequest?>>()
        val packets = CopyOnWriteArrayList<AudioPacket>()
        val packetReady = CountDownLatch(1)
        override fun inputCapabilities(capabilities: InputCapabilities) = Unit
        override fun session(state: SessionState) = Unit
        override fun videoFormat(screen: ScreenId, format: VideoFormat?) = Unit
        override fun videoFrame(frame: VideoFrame) = Unit
        override fun audioFormat(stream: Int, format: AudioFormat?) { audio.add(stream to format) }
        override fun audioPacket(packet: AudioPacket) { packets.add(packet); packetReady.countDown() }
        override fun microphone(stream: Int, request: MicRequest?) { mic.add(stream to request) }
    }
    @Test fun canonicalAacStripsRtpAndRelayIsSilentByDefault() {
        DiPlayInput().use { input ->
            val capture = Capture(); input.start(configuration, capture)
            val local = CopyOnWriteArrayList<String>()
            val delegate = object : MediaSink {
                override fun onAudioStarted(id: AudioStreamId, format: CarPlayAudioFormat, firstSample: Int) { local.add("start") }
                override fun onAudioRtp(id: AudioStreamId, format: CarPlayAudioFormat, rtp: ByteArray, sample: Int) { local.add("packet") }
                override fun onAudioStopped(id: AudioStreamId) { local.add("stop") }
            }
            val bridge = StreamBridgeSink(input, delegate, SessionControl { true })
            val id = AudioStreamId(102, "media")
            val format = CarPlayAudioFormat(AudioCodecKind.AAC_LC, 48000, 2, 102)
            bridge.onAudioStarted(id, format, 0)
            val before = System.nanoTime() / 1000
            bridge.onAudioRtp(id, format, ByteArray(12) + hex("21 43 65"), 0)
            assertTrue(capture.packetReady.await(2, TimeUnit.SECONDS))
            val after = System.nanoTime() / 1000
            assertArrayEquals(hex("21 43 65"), capture.packets.single().data)
            assertTrue(capture.packets.single().ptsUs in before..after)
            assertTrue(local.isEmpty())
            bridge.onScreenStreamActive(110, false)
            assertEquals(0 to null, capture.audio.last())
            assertEquals(2, capture.audio.size)
            bridge.onAudioStopped(id)
            assertEquals(2, capture.audio.size)
        }
    }
    @Test fun voiceLpcmIsSwappedOnTheWorkerAndStoppedExplicitly() {
        DiPlayInput().use { input ->
            val capture = Capture(); input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            val id = AudioStreamId(101, "speechrecognition")
            val format = CarPlayAudioFormat(AudioCodecKind.LPCM, 16000, 1, 101)
            bridge.onAudioStarted(id, format, 40)
            bridge.onAudioRtp(id, format, ByteArray(12) + hex("12 34 80 00"), 40)
            assertTrue(capture.packetReady.await(2, TimeUnit.SECONDS))
            assertArrayEquals(hex("34 12 00 80"), capture.packets.single().data)
            assertEquals(AudioCodec.PCM_S16LE, capture.audio.first().second!!.codec)
            assertEquals(AudioRole.VOICE, capture.audio.first().second!!.role)
            assertEquals(2, capture.packets.single().stream)
            bridge.onAudioStopped(id)
            assertEquals(2 to null, capture.audio.last())
        }
    }

    @Test fun localAudioDelegatesOnWorkerAndStopsWhenSettingIsDisabled() {
        DiPlayInput().use { input ->
            input.start(configuration, Capture())
            val calls = CopyOnWriteArrayList<String>()
            val packetReady = CountDownLatch(1); val stopped = CountDownLatch(1)
            val caller = Thread.currentThread()
            val delegate = object : MediaSink {
                override fun onAudioStarted(id: AudioStreamId, format: CarPlayAudioFormat, firstSample: Int) {
                    assertNotSame(caller, Thread.currentThread()); calls.add("start")
                }
                override fun onAudioRtp(id: AudioStreamId, format: CarPlayAudioFormat, rtp: ByteArray, sample: Int) {
                    calls.add("packet"); packetReady.countDown()
                }
                override fun onAudioStopped(id: AudioStreamId) { calls.add("stop"); stopped.countDown() }
            }
            val enabled = java.util.concurrent.atomic.AtomicBoolean(true)
            val bridge = StreamBridgeSink(input, delegate, SessionControl { true }, playAudioLocally = enabled::get)
            val id = AudioStreamId(102, "media"); val format = CarPlayAudioFormat(AudioCodecKind.AAC_LC, 48000, 2, 102)
            bridge.onAudioStarted(id, format, 0)
            bridge.onAudioRtp(id, format, ByteArray(13), 0)
            assertTrue(packetReady.await(2, TimeUnit.SECONDS))
            enabled.set(false)
            assertTrue(stopped.await(2, TimeUnit.SECONDS))
            assertEquals(listOf("start", "packet", "stop"), calls)
        }
    }
    @Test fun thisDeviceMicrophoneDelegatesWithoutRequestingHeadUnitCapture() {
        DiPlayInput().use { input ->
            val capture = Capture(); input.start(configuration, capture)
            val started = CountDownLatch(1); val stopped = CountDownLatch(1)
            val config = MicrophoneConfig("telephony", 16000, 1, 102, 20,
                InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)), 7000, ByteArray(32))
            val caller = Thread.currentThread()
            val delegate = object : MediaSink {
                override fun onMicrophoneStarted(id: AudioStreamId, config: MicrophoneConfig) {
                    assertNotSame(caller, Thread.currentThread()); started.countDown()
                }
                override fun onMicrophoneStopped(id: AudioStreamId) { stopped.countDown() }
            }
            val bridge = StreamBridgeSink(input, delegate, SessionControl { true },
                microphoneSource = { org.openxplay.direlay.settings.MicrophoneSource.THIS_DEVICE })
            val id = AudioStreamId(102, "telephony")
            bridge.onMicrophoneStarted(id, config)
            assertTrue(started.await(2, TimeUnit.SECONDS))
            assertTrue(capture.mic.isEmpty())
            assertFalse(input.control(ControlEvent.MicPacket(0, ByteArray(640), 0, origin)))
            bridge.onScreenStreamActive(110, false)
            assertTrue(stopped.await(2, TimeUnit.SECONDS))
        }
    }

    @Test fun micControlAndSessionReplacementEndEveryStartedStreamExactlyOnce() {
        DiPlayInput().use { input ->
            input.microphoneAvailable(true)
            assertTrue(input.capabilities.microphone)
            val capture = Capture(); input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            val accepted = mutableListOf<ByteArray>(); var closed = false
            bridge.microphoneUplinkFactory = { _, _ -> object : MicrophoneUplink {
                override fun start() = Unit
                override fun offer(pcm: ByteArray): Boolean { accepted.add(pcm.copyOf()); return true }
                override fun close() { closed = true }
            } }
            val id = AudioStreamId(102, "speechrecognition")
            val config = MicrophoneConfig("speechrecognition", 16000, 1, 102, 20,
                InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)), 7000, ByteArray(32))
            bridge.onMicrophoneStarted(id, config)
            assertEquals(MicRequest(0, AudioCodec.PCM_S16LE, 16000, 1, 20), capture.mic.single().second)
            assertFalse(input.control(ControlEvent.MicPacket(1, ByteArray(640), 0, origin)))
            assertTrue(input.control(ControlEvent.MicPacket(0, ByteArray(640), 0, origin)))
            assertEquals(1, accepted.size)
            input.microphoneAvailable(false)
            assertTrue(input.capabilities.microphone)
            bridge.onAudioStarted(AudioStreamId(100, "media"), CarPlayAudioFormat(AudioCodecKind.AAC_LC, 48000, 2, 100), 0)
            StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            assertTrue(closed)
            assertFalse(input.capabilities.microphone)
            assertEquals(0 to null, capture.mic.last())
            assertEquals(1 to null, capture.audio.last())
            assertFalse(input.control(ControlEvent.MicPacket(0, ByteArray(640), 0, origin)))
            bridge.onMicrophoneStopped(id)
            assertEquals(2, capture.mic.size)
            input.microphoneAvailable(false); assertFalse(input.capabilities.microphone)
        }
    }
}
