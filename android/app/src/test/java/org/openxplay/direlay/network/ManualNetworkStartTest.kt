// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.network

import org.junit.Assert.*
import org.junit.Test

class ManualNetworkStartTest {
    @Test fun remindersAreClearedOnSuccessOrStopBeforeAnExternalAutomaticLaunch() {
        val unknown = NetworkReadiness(Readiness.UNKNOWN)
        val gate = ManualNetworkStart()
        assertTrue(gate.request(true, unknown))
        gate.observeSession(false, false)
        assertFalse(gate.reminders.isEmpty())
        gate.observeSession(true, false)
        assertFalse(gate.reminders.isEmpty())
        gate.observeSession(true, true)
        assertTrue(gate.reminders.isEmpty())
        gate.observeSession(false, false)
        gate.observeSession(true, false)
        assertTrue(gate.reminders.isEmpty())
        assertTrue(gate.request(true, unknown))
        gate.observeSession(true, false)
        gate.observeSession(false, false)
        assertTrue(gate.reminders.isEmpty())
        assertFalse(gate.request(true, NetworkReadiness(Readiness.PROBLEM, Readiness.UNKNOWN)))
        gate.observeSession(true, false)
        assertNull(gate.pending)
        assertTrue(gate.reminders.isEmpty())
    }

    @Test fun onlyManualProblemsOpenADialogAndUnknownNeverBlocks() {
        for (manual in listOf(false, true)) for (wifi in Readiness.entries) for (hotspot in Readiness.entries) {
            val gate = ManualNetworkStart()
            val state = NetworkReadiness(wifi, hotspot)
            val blocked = manual && state.problems.isNotEmpty()
            assertEquals(!blocked, gate.request(manual, state))
            assertEquals(if (blocked) state else null, gate.pending)
            assertEquals(if (manual && !blocked) state.unknown else emptySet<NetworkRequirement>(), gate.reminders)
        }
    }

    @Test fun settingsReturnRechecksBothProblemsAndStartsOnlyOnceWhenResolved() {
        val gate = ManualNetworkStart()
        assertFalse(gate.request(true, NetworkReadiness(Readiness.PROBLEM, Readiness.PROBLEM)))
        assertFalse(gate.recheck(NetworkReadiness(Readiness.OK, Readiness.PROBLEM)))
        assertEquals(setOf(NetworkRequirement.HOTSPOT), gate.pending!!.problems)
        assertFalse(gate.request(false, NetworkReadiness()))
        assertTrue(gate.recheck(NetworkReadiness(Readiness.OK, Readiness.UNKNOWN)))
        assertNull(gate.pending)
        assertEquals(setOf(NetworkRequirement.HOTSPOT), gate.reminders)
        assertFalse(gate.recheck(NetworkReadiness()))
        assertFalse(gate.accept())
    }

    @Test fun startAnywayAndCancelConsumeThePendingIntent() {
        val gate = ManualNetworkStart()
        val state = NetworkReadiness(Readiness.PROBLEM, Readiness.UNKNOWN)
        assertFalse(gate.request(true, state))
        assertTrue(gate.accept())
        assertNull(gate.pending)
        assertEquals(setOf(NetworkRequirement.HOTSPOT), gate.reminders)
        assertTrue(gate.request(false, state))
        assertTrue(gate.reminders.isEmpty())
        assertFalse(gate.request(true, state))
        gate.cancel()
        assertNull(gate.pending)
        assertTrue(gate.reminders.isEmpty())
        assertFalse(gate.recheck(NetworkReadiness()))
        assertFalse(gate.accept())
    }
}
