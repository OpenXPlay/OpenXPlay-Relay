// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test
import java.time.ZoneId

class StepTrailTest {
    @Test fun localStartRelativeTimesAndReset() {
        val trail = StepTrail(ZoneId.of("Asia/Shanghai"))
        trail.record("link_start", true, 123, 5_000_000_000)
        trail.record("ble_mtu mtu=512 status=0", false, 999999, 6_234_000_000)
        assertEquals(listOf("08:00:00.123 link_start", "+1.234s ble_mtu mtu=512 status=0"), trail.snapshot())
        trail.record("link_start", true, 1000, 9_000_000_000)
        assertEquals(listOf("08:00:01.000 link_start"), trail.snapshot())
        trail.clear()
        assertTrue(trail.snapshot().isEmpty())
        trail.record("ble_scan_started", false, 2000, 10_000_000_000)
        trail.record("ble_peer_found", false, 2000, 10_001_000_000)
        assertEquals(listOf("08:00:02.000 ble_scan_started", "+0.001s ble_peer_found"), trail.snapshot())
    }
    @Test fun capsLinesAndCharactersAndPreservesFirstLineAndOneMarker() {
        val trail = StepTrail(ZoneId.of("UTC"))
        trail.record("link_start", true, 0, 0)
        repeat(79) { trail.record("ble_write status=$it", false, 0, 0) }
        assertEquals(80, trail.snapshot().size)
        assertFalse(trail.snapshot().any { it.contains("omitted") })
        trail.record("ble_write status=79", false, 0, 0)
        assertEquals("… 2 earlier lines omitted", trail.snapshot()[1])
        repeat(20) { trail.record("x".repeat(400), false, 0, 0) }
        val lines = trail.snapshot()
        assertEquals(80, lines.size)
        assertEquals("00:00:00.000 link_start", lines[0])
        assertEquals("… 22 earlier lines omitted", lines[1])
        assertEquals(1, lines.count { it.contains("omitted") })
        assertTrue(lines.all { it.length <= 200 })
        assertEquals(200, lines.last().length)
        trail.record("link_start", true, 0, 0)
        assertEquals(listOf("00:00:00.000 link_start"), trail.snapshot())
    }
    @Test fun snapshotsAreImmutableAndIndependent() {
        val trail = StepTrail(ZoneId.of("UTC"))
        trail.record("link_start", true, 0, 0)
        val snapshot = trail.snapshot()
        trail.record("link_stop", false, 0, 1)
        trail.clear()
        assertEquals(listOf("00:00:00.000 link_start"), snapshot)
        try {
            (snapshot as MutableList<String>).add("changed")
            fail("Snapshot must be immutable")
        } catch (_: UnsupportedOperationException) { }
    }
}
