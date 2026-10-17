// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay

import com.shilapi.xcertplay.airplay.AudioCodecKind
import com.shilapi.xcertplay.airplay.AudioFormat
import com.shilapi.xcertplay.airplay.AudioStreamId
import com.shilapi.xcertplay.airplay.MediaSink
import com.shilapi.xcertplay.airplay.MicrophoneConfig
import com.shilapi.xcertplay.airplay.VideoCodec
import java.lang.reflect.Proxy
import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Test

class CountingMediaSinkTest {
    @Test
    fun delegatesEveryCallAndCountsBothFrameOverloadsByScreenType() {
        val calls = mutableListOf<Pair<String, List<Any?>>>()
        val original = Proxy.newProxyInstance(
            MediaSink::class.java.classLoader,
            arrayOf(MediaSink::class.java),
        ) { _, method, args ->
            calls.add(method.name to (args?.toList() ?: emptyList()))
            null
        } as MediaSink
        val sink = CountingMediaSink(original)
        val bytes = byteArrayOf(1, 2, 3)
        val audioId = AudioStreamId(103, "media")
        val audioFormat = AudioFormat(AudioCodecKind.LPCM, 48_000, 2, 96)
        val microphoneConfig = MicrophoneConfig(
            audioType = "speech",
            sampleRate = 16_000,
            channels = 1,
            payloadType = 97,
            frameMillis = 20,
            host = InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)),
            port = 7_000,
            key = bytes,
        )
        val recoveryHandler: () -> Unit = {}
        val diagnosticHandler: (String) -> Unit = {}
        assertEquals(emptyMap<Int, Long>(), sink.frameCounts)

        sink.onVideoCodec(110, VideoCodec.H264)
        sink.onVideoConfig(110, bytes)
        sink.setVideoRecoveryHandler(110, recoveryHandler)
        sink.setVideoDiagnosticHandler(110, diagnosticHandler)
        sink.onScreenStreamActive(110, true)
        sink.onAudioStarted(audioId, audioFormat, 42)
        sink.onAudioRtp(audioId, audioFormat, bytes, 43)
        sink.onAudioStopped(audioId)
        sink.onMicrophoneStarted(audioId, microphoneConfig)
        sink.onMicrophoneStopped(audioId)
        sink.onIapMessage(bytes)
        assertEquals(emptyMap<Int, Long>(), sink.frameCounts)

        sink.onVideoFrame(110, bytes)
        sink.onVideoFrame(110, bytes, 123L, 456L)
        sink.onVideoFrame(111, bytes, 789L, 987L)
        assertEquals(mapOf(110 to 2L, 111 to 1L), sink.frameCounts)
        assertEquals(
            listOf(
                "onVideoCodec" to listOf(110, VideoCodec.H264),
                "onVideoConfig" to listOf(110, bytes),
                "setVideoRecoveryHandler" to listOf(110, recoveryHandler),
                "setVideoDiagnosticHandler" to listOf(110, diagnosticHandler),
                "onScreenStreamActive" to listOf(110, true),
                "onAudioStarted" to listOf(audioId, audioFormat, 42),
                "onAudioRtp" to listOf(audioId, audioFormat, bytes, 43),
                "onAudioStopped" to listOf(audioId),
                "onMicrophoneStarted" to listOf(audioId, microphoneConfig),
                "onMicrophoneStopped" to listOf(audioId),
                "onIapMessage" to listOf(bytes),
                "onVideoFrame" to listOf(110, bytes),
                "onVideoFrame" to listOf(110, bytes, 123L, 456L),
                "onVideoFrame" to listOf(111, bytes, 789L, 987L),
            ),
            calls,
        )

        var fallbackCalls = 0
        val fallback = CountingMediaSink(object : MediaSink {
            override fun onVideoFrame(type: Int, naluBytes: ByteArray) {
                fallbackCalls++
            }
        })
        fallback.onVideoFrame(110, bytes, 123L, 456L)
        assertEquals(1, fallbackCalls)
        assertEquals(mapOf(110 to 1L), fallback.frameCounts)
    }
}
