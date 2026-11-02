// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.logs

import org.junit.Assert.*
import org.junit.Test

class LogcatProbeTest {
    private fun line(priority: Char, message: String, tag: String = LogcatProbe.TAG) =
        "2026-11-01 05:05:00.000 +0000 1 2 $priority $tag: $message"

    @Test fun allPrioritiesReturnedCompleteTheProbeWithoutClaimingHistoricalCoverage() {
        var now = 0L
        val probe = LogcatProbe({ now }, { "random-test-nonce" })
        val markers = linkedMapOf<Char, String>()
        probe.start { priority, message -> markers[priority] = message; 42 }
        assertEquals(listOf('V', 'D', 'I', 'W', 'E'), markers.keys.toList())
        markers.forEach { (priority, message) -> probe.observe(line(priority, message)) }
        assertEquals(LogcatProbe.State.PENDING, probe.result().state)
        now = 5_000
        val result = probe.result()
        assertFalse(result.incomplete)
        assertEquals(markers.keys, result.seen)
        assertTrue(result.summary().contains("all priorities"))
        assertTrue(result.summary().contains("older records and car connectivity are not assessed"))
    }
    @Test fun errorOnlyIsIncompleteAndNamesInfoAndWarn() {
        var now = 0L
        val probe = LogcatProbe({ now }, { "nonce" })
        var error = ""
        probe.start { priority, message -> if (priority == 'E') error = message; 10 }
        probe.observe(line('E', error))
        now = 5_000
        assertTrue(probe.result().incomplete)
        assertEquals(setOf('E'), probe.result().seen)
        assertTrue(probe.result().summary().contains("logcat returned E only; missing VERBOSE, DEBUG, INFO, WARN"))
        assertTrue(probe.result().summary().contains("I=10, W=10"))
    }
    @Test fun noneIsIncompleteOnlyAfterTheBoundedWindow() {
        var now = 0L
        val probe = LogcatProbe({ now })
        assertEquals(LogcatProbe.State.NOT_STARTED, probe.result().state)
        assertFalse(probe.result().incomplete)
        probe.start { _, _ -> 1 }
        now = 4_999
        assertFalse(probe.result().incomplete)
        now++
        assertTrue(probe.result().incomplete)
        assertTrue(probe.result().summary().contains("no probe markers"))
    }
    @Test fun markersAreOneShotAndRequireTheNonceTagPriorityAndWindow() {
        var now = 0L
        val probe = LogcatProbe({ now }, { "nonce" })
        val markers = linkedMapOf<Char, String>()
        probe.start { priority, message -> markers[priority] = message; 1 }
        probe.start { _, _ -> fail("Must not emit twice"); 0 }
        probe.observe(line('I', markers.getValue('I'), "AnotherTag"))
        probe.observe(line('I', "nonce=older-nonce priority=I"))
        probe.observe(line('W', markers.getValue('I')))
        probe.observe(line('I', "prefix " + markers.getValue('I')))
        now = 5_000
        probe.observe(line('I', markers.getValue('I')))
        assertTrue(probe.result().seen.isEmpty())
    }
    @Test fun aFailedWriteDoesNotPreventTheOtherMarkers() {
        var now = 0L
        val probe = LogcatProbe({ now })
        val tried = mutableListOf<Char>()
        probe.start { priority, _ -> tried += priority; if (priority == 'I') error("unavailable") else 12 }
        now = 5_000
        assertEquals(listOf('V', 'D', 'I', 'W', 'E'), tried)
        assertNull(probe.result().writes['I'])
        assertTrue(probe.result().summary().contains("I=null"))
    }
}
