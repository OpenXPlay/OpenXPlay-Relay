// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import org.openxplay.stream.core.*

internal fun hex(value: String): ByteArray = value.trim().split(Regex("\\s+")).map { it.toInt(16).toByte() }.toByteArray()
internal val avcBytes get() = hex("01 42 c0 1e ff e1 00 09 67 42 c0 1e f4 05 01 ec 80 01 00 04 68 ce 3c 80")
internal val hevcBytes get() = hex("""
    01 01 60 00 00 00 90 00 00 00 00 00 78 f0 00 fc
    fd f8 f8 00 00 0f 03 a0 00 01 00 17 40 01 0c 01
    ff ff 01 60 00 00 03 00 90 00 00 03 00 00 03 00
    78 f0 24 a1 00 01 00 1e 42 01 01 01 60 00 00 03
    00 90 00 00 03 00 00 03 00 78 a0 03 c0 80 11 07
    cb 97 e4 93 6b 20 a2 00 01 00 06 44 01 c1 71 80 89
""")
internal fun avcFormat(bytes: ByteArray = avcBytes, fps: Double = 30.0): VideoFormat =
    VideoFormat.create(ScreenId.MAIN, VideoCodec.H264, bytes, fps)
internal fun frame(key: Boolean = false, pts: Long = 1, session: Long = 1, screen: ScreenId = ScreenId.MAIN) =
    VideoFrame(screen, SessionId(session), hex(if (key) "00 00 00 02 65 80" else "00 00 00 02 41 80"), pts, key, 0)
internal class MiracastHost : OutputHost {
    val capabilities = mutableListOf<OutputCapabilities>()
    val controls = mutableListOf<ControlEvent>()
    val statistics = mutableListOf<OutputStats>()
    var onControl: () -> Unit = {}
    override fun capabilities(capabilities: OutputCapabilities) { this.capabilities += capabilities }
    override fun control(event: ControlEvent) { controls += event; onControl() }
    override fun stats(stats: OutputStats) { statistics += stats }
}
internal class MiracastEvents : MiracastListener {
    val states = mutableListOf<Pair<MiracastState, MiracastDisplay?>>()
    val failures = mutableListOf<String>()
    val diagnostics = mutableListOf<String>()
    val statistics = mutableListOf<MiracastStatistics>()
    var onDiagnostic: (String) -> Unit = {}
    override fun state(state: MiracastState, display: MiracastDisplay?) { states += state to display }
    override fun failure(code: String, detail: String) { failures += code }
    override fun diagnostic(message: String) { diagnostics += message; onDiagnostic(message) }
    override fun statistics(statistics: MiracastStatistics) { this.statistics += statistics }
}
