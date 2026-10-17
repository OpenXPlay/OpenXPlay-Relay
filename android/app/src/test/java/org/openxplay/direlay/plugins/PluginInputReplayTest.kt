// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.plugins

import com.shilapi.xcertplay.airplay.SessionControl
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.bridge.DiPlayInput
import org.openxplay.stream.core.*

class PluginInputReplayTest {
    @Test fun earlyRecordContextIsReplayedAfterSessionAnnouncementAndClearedOnEnd() {
        val updates = mutableListOf<ContextUpdate>()
        val contexts = mutableListOf<ContextUpdate>()
        val sink = object : InputSink {
            override fun session(state: SessionState) { contexts.clear() }
            override fun context(update: ContextUpdate) { contexts.add(update); updates.add(update) }
            override fun videoFormat(screen: ScreenId, format: VideoFormat?) = Unit
            override fun videoFrame(frame: VideoFrame) = Unit
            override fun audioFormat(stream: Int, format: AudioFormat?) = Unit
            override fun audioPacket(packet: AudioPacket) = Unit
            override fun microphone(stream: Int, request: MicRequest?) = Unit
        }
        DiPlayInput().use { input ->
            input.start(SessionConfig(emptyMap()), sink)
            val active = ContextUpdate.Session(SessionContext(CarPlayState.CONNECTED))
            input.context(active)
            val session = input.begin(SessionControl { false })
            input.announce(session)
            assertEquals(listOf(active), contexts)
            assertEquals(listOf(active, active), updates)
            input.end(session)
            input.context(active)
            input.resume(session) {}
            assertEquals(listOf(active), contexts)
            input.end(session)
            val replacement = input.begin(SessionControl { false })
            input.announce(replacement)
            assertTrue(contexts.isEmpty())
        }
    }
}
