// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test

class AutoStartGateTest {
    private class Memory : AutoStartStorage {
        val values = mutableMapOf<String, String>()
        override fun read(key: String) = values[key]
        override fun write(key: String, value: String) { values[key] = value }
    }
    @Test fun pauseSurvivesProcessRecreationAndResumePersists() {
        val storage = Memory()
        val boot = BootIdentitySource { "count:4" }
        val first = PersistentAutoStartGate(boot, storage)
        assertFalse(first.paused)
        first.pause()
        val second = PersistentAutoStartGate(boot, storage)
        assertTrue(second.paused)
        second.resume()
        assertFalse(second.paused)
        assertFalse(PersistentAutoStartGate(boot, storage).paused)
    }
    @Test fun rebootClearsPauseForCountAndFallbackIdentity() {
        for ((before, after) in listOf("count:4" to "count:5", "time:100000" to "time:200000")) {
            val storage = Memory()
            PersistentAutoStartGate(BootIdentitySource { before }, storage).pause()
            assertFalse(PersistentAutoStartGate(BootIdentitySource { after }, storage).paused)
        }
    }
    @Test fun countWinsAndUnavailableCountUsesBootWallTimeRoundedToTenSeconds() {
        assertEquals("count:3", bootIdentity(3, 127_000, 4_000))
        assertEquals("time:120000", bootIdentity(null, 127_000, 4_000))
        assertEquals("time:120000", bootIdentity(-1, 128_000, 5_000))
        assertEquals("time:130000", bootIdentity(null, 128_000, 2_000))
        val storage = Memory()
        PersistentAutoStartGate(BootIdentitySource { bootIdentity(null, 127_000, 4_000) }, storage).pause()
        assertTrue(PersistentAutoStartGate(BootIdentitySource { bootIdentity(null, 128_000, 5_000) }, storage).paused)
    }
    @Test fun externalStopsPauseOnceAndOwnRestartDoesNotPause() {
        val storage = Memory()
        val boot = BootIdentitySource { "count:4" }
        var stops = 0
        val tracker = SessionStopTracker(boot, storage) { stops++ }
        tracker.observe(false); assertEquals(0, stops)
        tracker.observe(true); tracker.observe(false); tracker.observe(false)
        assertEquals(1, stops)
        tracker.observe(true); tracker.restarting(); tracker.observe(true); tracker.observe(false)
        assertEquals(1, stops)
        tracker.observe(true); tracker.observe(false)
        assertEquals(2, stops)
    }
    @Test fun stopWhileProcessIsDeadIsObservedButAnOldBootIsIgnored() {
        val storage = Memory()
        val boot = BootIdentitySource { "count:4" }
        var stops = 0
        SessionStopTracker(boot, storage) { stops++ }.observe(true)
        val recreated = SessionStopTracker(boot, storage) { stops++ }
        assertFalse(recreated.observed)
        recreated.observe(false)
        assertTrue(recreated.observed)
        assertEquals(1, stops)
        recreated.observe(true)
        SessionStopTracker(BootIdentitySource { "count:5" }, storage) { stops++ }.observe(false)
        assertEquals(1, stops)
    }
    @Test fun restartSuppressionSurvivesRecreationAndIsConsumedByOneStop() {
        val storage = Memory()
        val boot = BootIdentitySource { "count:4" }
        var stops = 0
        val first = SessionStopTracker(boot, storage) { stops++ }
        first.observe(true); first.restarting()
        val next = SessionStopTracker(boot, storage) { stops++ }
        next.observe(false)
        assertEquals(0, stops)
        next.observe(true); next.observe(false)
        assertEquals(1, stops)
    }
    @Test fun processStartGuardRejectsASecondActivityUntilPreparationFinishes() {
        val gate: StartRequestGate = ProcessStartRequestGate()
        assertFalse(gate.pending)
        assertTrue(gate.begin())
        assertTrue(gate.pending)
        assertFalse(gate.begin())
        gate.finish()
        assertFalse(gate.pending)
        assertTrue(gate.begin())
    }
}
