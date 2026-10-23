// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.carlinkdebug

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.clink.trace.*

class CarlinkSummaryTraceTest {
    private var now = 0L
    private val select = CarlinkSummaryTrace.Selection { now }
    private fun video(name: String) = ClinkTraceEvent.Video(0, 1, name)
    private fun frame(service: String, category: Int = 1, method: Int = 24) =
        ClinkTraceEvent.Frame(0, 1, "RX", 1, service, category, method, 0, 1, 0, emptyMap())
    private fun media(session: Long = 1) = ClinkTraceEvent.Media(0, session, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    @Test fun baselineDropsHotEventsAndKeepsControlAndIdentity() {
        for (event in listOf(video("live"), video("reanchor"), video("clock"), video("codec_mismatch"),
            frame("uibc"), frame("media", 4, 9), frame("sensor", 2), frame("control", 7), frame("control", 1, 1),
            frame("control", 1, 12), ClinkTraceEvent.Input(0, 1, "touch", emptyList()))) assertNull(event.type, select.select(event))
        for (event in listOf(video("format"), video("freeze"), frame("control"), frame("control", method = 4),
            frame("control", method = 5), frame("control", method = 15),
            ClinkTraceEvent.Link(0, 1, "ble_advertisement"), ClinkTraceEvent.Link(0, 1, "ble_reply_name"),
            ClinkTraceEvent.Link(0, 1, "head_unit_certificate"), ClinkTraceEvent.Error(0, 1, "failed", "IOException", "test"),
            ClinkTraceEvent.Socket(0, 1, "rtsp", "CONNECTED", "192.168.1.1", 1),
            ClinkTraceEvent.Negotiation(0, 1, "mode", emptyList(), true, 1, "display"),
            ClinkTraceEvent.Rtsp(0, 1, "RX", "RTSP/1.0 200 OK", emptyMap(), "wfd_video_formats: caps"))) assertNotNull(event.type, select.select(event))
    }
    @Test fun keyframesAreLimitedWithSuppressedCount() {
        assertEquals(0L, select.select(video("keyframe_request")))
        now = 1; assertNull(select.select(video("keyframe_request")))
        now = 999; assertNull(select.select(video("keyframe_request")))
        now = 1000; assertEquals(2L, select.select(video("keyframe_request")))
        now = 2000; assertEquals(0L, select.select(video("keyframe_request")))
        now = 2001; assertNull(select.select(video("keyframe_request")))
        assertEquals(1L, select.takeSuppressed()); assertEquals(0L, select.takeSuppressed())
    }
    @Test fun mediaHasTenSecondLimitAndResetsOnSessionChange() {
        assertEquals(0L, select.select(media()))
        now = 9999; assertNull(select.select(media()))
        select.select(ClinkTraceEvent.Link(0, 0, "gatt_disconnect_started"))
        assertNull(select.select(media()))
        now = 10_000; assertEquals(0L, select.select(media()))
        now = 10_001; assertEquals(0L, select.select(media(2)))
    }
}
