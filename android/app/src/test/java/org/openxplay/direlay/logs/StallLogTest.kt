// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class StallLogTest {
    @get:Rule val temporary = TemporaryFolder()

    @Test fun reportsGrowAfterThirtySecondsUpToTenMinutes() {
        assertEquals(30_000L, nextStallReport(0))
        assertEquals(60_000L, nextStallReport(30_000))
        assertEquals(960_000L, nextStallReport(480_000))
        assertEquals(1_560_000L, nextStallReport(960_000))
    }

    @Test fun aWaitLogsItsStartGrowingReminderChangesAndEnd() {
        var now = 0L
        val lines = mutableListOf<String>()
        val waits = WaitLog({ now }) { event, detail -> lines += "$event $detail" }
        waits.begin("network", "problems=wifi")
        waits.begin("network", "problems=wifi")
        now = 29_999; waits.tick("network", "problems=wifi")
        now = 30_000; waits.tick("network", "problems=wifi")
        now = 45_000; waits.tick("network", "problems=hotspot")
        now = 59_000; waits.tick("network", "problems=hotspot")
        assertEquals("network:59s", waits.summary())
        now = 61_000; waits.end("network", "ready")
        waits.end("network", "ready")
        waits.tick("network")
        assertEquals(listOf("carplay_wait for=network problems=wifi",
            "carplay_waiting for=network waited=30000 problems=wifi",
            "carplay_waiting for=network waited=45000 problems=hotspot",
            "carplay_wait_end for=network waited=61000 outcome=ready"), lines)
        assertFalse(waits.active("network"))
        assertEquals("", waits.summary())
    }

    @Test fun aSteadyPathReportsUntilItChangesOrGoesLive() {
        val lines = mutableListOf<String>()
        val path = SteadyStateLog { waited, value -> lines += "$waited $value" }
        path.observe("I CP·", false, 0)
        path.observe("I CP·", false, 29_000)
        path.observe("I CP·", false, 30_000)
        path.observe("I CP·", false, 59_000)
        path.observe("I CP·", false, 60_000)
        path.observe("I CP~", false, 61_000)
        path.observe("I CP~", false, 90_000)
        path.observe("P LIVE✓", true, 100_000)
        path.observe("P LIVE✓", true, 1_000_000)
        assertEquals(listOf("30000 I CP·", "60000 I CP·"), lines)
    }

    @Test fun processPausesAreToldFromDeviceSleep() {
        val asleep = ProcessStallDetector()
        assertTrue(asleep.observe(1_000, 1_000, 100).isEmpty())
        // Deep sleep stops uptime as well; a frozen or starved process loses both clocks alike.
        assertEquals(ProcessStallDetector.Finding("process_paused", "gap=60000 uptime_gap=1000 cause=device_sleep"),
            asleep.observe(61_000, 2_000, 60_100).single())
        val frozen = ProcessStallDetector()
        frozen.observe(0, 0, 0)
        assertEquals("gap=60000 uptime_gap=60000 cause=process_stopped", frozen.observe(60_000, 60_000, 60_000).single().detail)
        // The runtime tick missed the same pause: no block is reported while it catches up.
        assertTrue(frozen.observe(61_000, 61_000, 61_000).isEmpty())
        assertTrue(frozen.observe(62_000, 62_000, 200).isEmpty())
    }

    @Test fun aBlockedRuntimeIsReportedWithItsStackAndGrowingGaps() {
        val detector = ProcessStallDetector()
        val findings = mutableListOf<ProcessStallDetector.Finding>()
        for (second in 0..200) {
            val now = second * 1_000L
            findings += detector.observe(now, now, if (second in 10..150) now - 10_000 else 300)
        }
        assertEquals(listOf("runtime_blocked for=6000", "runtime_blocked for=30000", "runtime_blocked for=60000",
            "runtime_blocked for=120000", "runtime_unblocked after=141000"), findings.map { "${it.event} ${it.detail}" })
        assertEquals(listOf("runtime", "runtime", "runtime", "runtime", null), findings.map { it.thread })
    }

    @Test fun aFrozenMainThreadIsReportedOnItsOwn() {
        val detector = ProcessStallDetector()
        val findings = (0..20).flatMap { second ->
            val now = second * 1_000L
            detector.observe(now, now, 300, mainAge = if (second in 3..12) (second - 3) * 1_000L else 0)
        }
        assertEquals(listOf(ProcessStallDetector.Finding("main_blocked", "for=6000", "main"),
            ProcessStallDetector.Finding("main_unblocked", "after=10000")), findings)
    }

    @Test fun recorderNotesParseAsTimestampedLogLines() {
        val line = LogRecorder.note(1_761_728_612_345L, "logcat ended exit=1")
        assertEquals(1_761_728_612_345L, LogLine.timestamp(line))
        assertEquals("logcat ended exit=1", LogLine.message(line))
    }

    @Test fun theHeartbeatFileRoundTripsAndToleratesDamage() {
        val file = temporary.root.resolve("diagnostics/heartbeat.properties")
        val heartbeat = HeartbeatFile(file)
        assertNull(heartbeat.read())
        heartbeat.write(mapOf("pid" to "12498", "path" to "R MODE✓ | I CP·"))
        assertEquals(mapOf("pid" to "12498", "path" to "R MODE✓ | I CP·"), heartbeat.read())
        heartbeat.write(mapOf("pid" to "13108"))
        assertEquals(mapOf("pid" to "13108"), heartbeat.read())
        heartbeat.delete()
        assertNull(heartbeat.read())
        assertEquals("user_requested", exitReasonName(10))
        assertEquals("freezer", exitReasonName(14))
        assertEquals("cached", importanceName(400))
        assertEquals("rare", standbyBucketName(40))
        assertEquals("pid=1 at=1970-01-01T00:00:01Z reason=signaled status=9 importance=cached pss_kb=2 rss_kb=3 description=a b",
            ProcessExit(1, 1_000, "signaled", 9, "cached", 2, 3, "a\nb").line())
    }
}
