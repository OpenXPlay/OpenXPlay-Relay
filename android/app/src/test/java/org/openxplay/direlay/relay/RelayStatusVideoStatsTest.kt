// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.core.*

class RelayStatusVideoStatsTest {
    @Test fun bytesCountOnlyActiveMainScreenFramesWhileEnabled() {
        val status = RelayStatus()
        fun session(id: Long, phase: SessionPhase = SessionPhase.ACTIVE) =
            status.onSession(SessionState(SessionId(id), phase), SessionConfig(emptyMap()))
        fun frame(id: Long = 1, screen: ScreenId = ScreenId.MAIN) =
            status.onVideoFrame(VideoFrame(screen, SessionId(id), ByteArray(100), 0, true, 0))
        session(1); frame()
        assertEquals(0L, status.videoBytes); assertEquals(1L, status.mainFrames)
        status.countVideoBytes = true
        frame(); frame(2); frame(screen = ScreenId(999))
        assertEquals(100L, status.videoBytes)
        status.countVideoBytes = false; frame()
        assertEquals(100L, status.videoBytes)
        session(2); assertEquals(0L, status.videoBytes)
        status.countVideoBytes = true; session(2, SessionPhase.ENDED); frame(2)
        assertEquals(0L, status.videoBytes)
    }
    @Test fun silentBrowserReportsExpireAfterFifteenSeconds() {
        var now = 0L
        val status = RelayStatus { now }
        status.stats("web", OutputStats(Origin("web", "one"), ScreenId.MAIN, null, 0, 0, 1, null))
        now = 14_999_999_999L
        assertEquals(1, status.webMeasurements().size)
        now = 15_000_000_001L
        assertTrue(status.webMeasurements().isEmpty())
    }
    @Test fun clientReportsAreBoundedAndDoNotOverwriteEachOther() {
        val status = RelayStatus()
        repeat(100) { index -> status.stats("web", OutputStats(Origin("web", "$index"), ScreenId.MAIN, null, 0, 0, index.toLong(), null)) }
        assertEquals(ReceiverDiagnostics.MAX_CLIENTS, status.webMeasurements().size)
        assertEquals((92L..99L).toList(), status.webMeasurements().map { it.rendered })
        status.close(); assertTrue(status.webMeasurements().isEmpty())
    }
}
