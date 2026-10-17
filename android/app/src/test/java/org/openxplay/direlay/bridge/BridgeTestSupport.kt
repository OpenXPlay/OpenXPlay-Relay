// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.bridge

import org.openxplay.stream.core.*

internal fun hex(value: String): ByteArray = value.trim().split(Regex("\\s+")).map { it.toInt(16).toByte() }.toByteArray()
internal val avcConfig get() = hex("01 42 c0 1e ff e1 00 09 67 42 c0 1e f4 05 01 ec 80 01 00 04 68 ce 3c 80")
internal val hevcConfig get() = hex("""
    01 01 60 00 00 00 90 00 00 00 00 00 78 f0 00 fc
    fd f8 f8 00 00 0f 03 a0 00 01 00 17 40 01 0c 01
    ff ff 01 60 00 00 03 00 90 00 00 03 00 00 03 00
    78 f0 24 a1 00 01 00 1e 42 01 01 01 60 00 00 03
    00 90 00 00 03 00 00 03 00 78 a0 03 c0 80 11 07
    cb 97 e4 93 6b 20 a2 00 01 00 06 44 01 c1 71 80 89
""")
internal val configuration = SessionConfig(mapOf(ScreenId.MAIN to ScreenConfig(VideoCodec.H264, 640, 480, 60.0)))
internal val origin = Origin("test", "browser")

internal class CapturingInputSink : InputSink {
    val states = mutableListOf<SessionState>()
    val formats = mutableListOf<Pair<ScreenId, VideoFormat?>>()
    val frames = mutableListOf<VideoFrame>()
    override fun session(state: SessionState) { states.add(state) }
    override fun videoFormat(screen: ScreenId, format: VideoFormat?) { formats.add(screen to format) }
    override fun videoFrame(frame: VideoFrame) { frames.add(frame) }
    override fun audioFormat(stream: Int, format: AudioFormat?) = Unit
    override fun audioPacket(packet: AudioPacket) = Unit
    override fun microphone(stream: Int, request: MicRequest?) = Unit
}
