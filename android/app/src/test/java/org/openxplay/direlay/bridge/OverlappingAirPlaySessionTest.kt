// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.bridge

import com.shilapi.xcertplay.airplay.*
import java.net.Socket
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.openxplay.stream.core.SessionPhase

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29], manifest = Config.NONE)
class OverlappingAirPlaySessionTest {
    private fun session(engine: CarPlayMediaEngine) = AirPlaySession(
        socket = Socket(),
        config = AirPlayConfig(deviceName = "test", deviceId = "02:00:00:00:00:02", btMac = "02:00:00:00:00:01",
            sourceVersion = "1.0", main = AirPlayDisplayConfig(widthPixels = 800, heightPixels = 480)),
        identity = AirPlayIdentity.generate(), pairings = PairingStore(), mfi = null,
        listener = object : AirPlaySessionListener {}, media = engine,
    ).also { peer ->
        val verifier = AirPlaySession::class.java.getDeclaredField("pairVerify").apply { isAccessible = true }.get(peer)
        PairVerify::class.java.getDeclaredField("sharedSecret").apply { isAccessible = true }
            .set(verifier, ByteArray(32) { it.toByte() })
    }
    private fun screen(engine: CarPlayMediaEngine, peer: AirPlaySession, id: Long): ScreenStream.Listener {
        val port = checkNotNull(engine.onScreen(peer, 110, mapOf("streamConnectionID" to id)))
        val streams = CarPlayMediaEngine::class.java.getDeclaredField("streams").apply { isAccessible = true }
            .get(engine) as Map<*, *>
        val target = streams.values.filterIsInstance<ScreenStream>().single {
            val server = ScreenStream::class.java.getDeclaredField("server").apply { isAccessible = true }.get(it) as java.net.ServerSocket
            server.localPort == port
        }
        return ScreenStream::class.java.getDeclaredField("listener").apply { isAccessible = true }.get(target) as ScreenStream.Listener
    }
    private fun isClosed(peer: AirPlaySession) = (AirPlaySession::class.java.getDeclaredField("closed")
        .apply { isAccessible = true }.get(peer) as java.util.concurrent.atomic.AtomicBoolean).get()
    @Test fun closingTheOldControlMustNotDropFramesFromTheNewOpenControl() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            val engine = CarPlayMediaEngine(bridge)
            val old = session(engine)
            val fresh = session(engine)
            try {
                screen(engine, old, 1L)
                val current = screen(engine, fresh, 2L)
                current.onCodec(VideoCodec.H264)
                current.onConfig(avcConfig)
                current.onFrame(hex("00 00 01 65 88"))
                assertEquals(SessionPhase.ACTIVE, capture.states.last().phase)
                assertEquals(1, capture.frames.size)
                old.close()
                assertFalse("new AirPlay control connection remains open", isClosed(fresh))
                current.onFrame(hex("00 00 01 65 99"))
                assertEquals("new stream was stopped by old session cleanup: phases=" + capture.states.map { it.phase }, 2, capture.frames.size)
                assertEquals(SessionPhase.ACTIVE, capture.states.last().phase)
            } finally { old.close(); fresh.close() }
        }
    }
    @Test fun closingAnOldControlWithoutAMainStreamPreservesNewFrames() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            val engine = CarPlayMediaEngine(bridge)
            val old = session(engine)
            val fresh = session(engine)
            try {
                val current = screen(engine, fresh, 2L)
                current.onCodec(VideoCodec.H264)
                current.onConfig(avcConfig)
                current.onFrame(hex("00 00 01 65 88"))
                old.close()
                current.onFrame(hex("00 00 01 65 99"))
                assertEquals(2, capture.frames.size)
                assertEquals(SessionPhase.ACTIVE, capture.states.last().phase)
            } finally { old.close(); fresh.close() }
        }
    }

    @Test fun closingTheCurrentConnectionStillEndsProjection() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            val engine = CarPlayMediaEngine(bridge)
            val peer = session(engine)
            try {
                val current = screen(engine, peer, 1L)
                current.onCodec(VideoCodec.H264)
                current.onConfig(avcConfig)
                current.onFrame(hex("00 00 01 65 88"))
                peer.close()
                assertEquals(SessionPhase.ENDED, capture.states.last().phase)
                assertNull(capture.formats.last().second)
                current.onFrame(hex("00 00 01 65 99"))
                assertEquals(1, capture.frames.size)
            } finally { peer.close() }
        }
    }

    @Test fun oldTeardownDoesNotClearTheNewFormatOrDropFrames() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            val engine = CarPlayMediaEngine(bridge)
            val old = session(engine)
            val fresh = session(engine)
            try {
                val previous = screen(engine, old, 1L)
                val current = screen(engine, fresh, 2L)
                current.onCodec(VideoCodec.H264)
                current.onConfig(avcConfig)
                current.onFrame(hex("00 00 01 65 88"))
                engine.onTeardown(old, 110)
                previous.onCodec(VideoCodec.H265)
                previous.onConfig(hevcConfig)
                previous.onClosed(null)
                current.onFrame(hex("00 00 01 65 99"))
                assertFalse(isClosed(fresh))
                assertEquals(SessionPhase.ACTIVE, capture.states.last().phase)
                assertEquals(org.openxplay.stream.core.VideoCodec.H264, capture.formats.last().second?.codec)
                assertEquals(2, capture.frames.size)
            } finally { old.close(); fresh.close() }
        }
    }

    @Test fun replacementResetsTheStreamSessionBeforeAcceptingNewPlayback() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            val engine = CarPlayMediaEngine(bridge)
            val old = session(engine)
            val fresh = session(engine)
            try {
                val previous = screen(engine, old, 1L)
                previous.onCodec(VideoCodec.H264)
                previous.onConfig(avcConfig)
                previous.onFrame(hex("00 00 01 65 88"))
                val oldId = capture.states.last().id
                val current = screen(engine, fresh, 2L)
                assertEquals(SessionPhase.CONNECTING, capture.states.last().phase)
                assertNotEquals(oldId, capture.states.last().id)
                assertNull(capture.formats.last().second)
                current.onCodec(VideoCodec.H264)
                current.onConfig(avcConfig)
                current.onFrame(hex("00 00 01 65 99"))
                assertEquals(SessionPhase.ACTIVE, capture.states.last().phase)
                assertNotEquals(oldId, capture.frames.last().session)
                old.close()
                assertEquals(SessionPhase.ACTIVE, capture.states.last().phase)
                assertEquals(2, capture.frames.size)
            } finally { old.close(); fresh.close() }
        }
    }

    @Test fun newPlaybackNeedsItsOwnConfirmationAfterASuccessfulOldSession() {
        val confirmations = mutableListOf<Any>()
        var token = Any()
        val control = object : SessionControl {
            override fun sendTouch(contacts: List<AirPlayContact>) = false
            override fun playbackToken(): Any = token
            override fun reportVideoFrameRendered(token: Any): Boolean { confirmations += token; return true }
        }
        DiPlayInput(remotePlaybackProof = { true }).use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, control)
            val engine = CarPlayMediaEngine(bridge)
            val old = session(engine)
            val fresh = session(engine)
            try {
                val previous = screen(engine, old, 1L)
                previous.onCodec(VideoCodec.H264)
                previous.onConfig(avcConfig)
                val oldToken = token
                val oldStats = org.openxplay.stream.core.OutputStats(origin, org.openxplay.stream.core.ScreenId.MAIN,
                    null, 0, 0, 1, null, session = capture.states.last().id)
                assertTrue(input.playback(oldStats))
                token = Any()
                val current = screen(engine, fresh, 2L)
                current.onCodec(VideoCodec.H264)
                current.onConfig(avcConfig)
                assertFalse(input.playback(oldStats))
                assertTrue(input.playback(oldStats.copy(session = capture.states.last().id)))
                assertEquals(listOf(oldToken, token), confirmations)
            } finally { old.close(); fresh.close() }
        }
    }

    @Test fun reconfiguringTheSameControlKeepsItsProjectionSession() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            val engine = CarPlayMediaEngine(bridge)
            val peer = session(engine)
            try {
                val previous = screen(engine, peer, 1L)
                previous.onCodec(VideoCodec.H264)
                previous.onConfig(avcConfig)
                previous.onFrame(hex("00 00 01 65 88"))
                val id = capture.states.last().id
                val current = screen(engine, peer, 2L)
                previous.onClosed(null)
                current.onCodec(VideoCodec.H264)
                current.onConfig(avcConfig)
                current.onFrame(hex("00 00 01 65 99"))
                assertFalse(isClosed(peer))
                assertEquals(SessionPhase.ACTIVE, capture.states.last().phase)
                assertEquals(id, capture.states.last().id)
                assertFalse(capture.states.any { it.phase == SessionPhase.ENDED })
                assertEquals(2, capture.frames.size)
            } finally { peer.close() }
        }
    }

    @Test fun savedExperimentChoicesAndMasterControlTheActualBridgeLifecycle() {
        for (active in listOf(false, true)) for (bits in 0..7) {
            val choices = org.openxplay.direlay.settings.ExperimentalOptions(active = active,
                screenSessionIsolation = bits and 1 != 0, screenReconnectReset = bits and 2 != 0,
                screenDeliverySerialization = bits and 4 != 0)
            DiPlayInput().use { input ->
                val capture = CapturingInputSink()
                input.start(configuration, capture)
                val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
                val engine = CarPlayMediaEngine(bridge, screenSessionPolicy = choices.toScreenSessionPolicy())
                val old = session(engine)
                val fresh = session(engine)
                try {
                    val previous = screen(engine, old, 1L)
                    previous.onCodec(VideoCodec.H264)
                    previous.onConfig(avcConfig)
                    val before = capture.states.last().id
                    val current = screen(engine, fresh, 2L)
                    current.onCodec(VideoCodec.H264)
                    current.onConfig(avcConfig)
                    assertEquals(choices.toString(), active && choices.screenReconnectReset,
                        before != capture.states.last().id)
                    old.close()
                    val protected = active && (choices.screenSessionIsolation || choices.screenReconnectReset)
                    assertEquals(choices.toString(), if (protected) SessionPhase.ACTIVE else SessionPhase.ENDED,
                        capture.states.last().phase)
                    current.onFrame(hex("00 00 01 65 88"))
                    assertEquals(choices.toString(), if (protected) 1 else 0, capture.frames.size)
                } finally { old.close(); fresh.close() }
            }
        }
    }
}
