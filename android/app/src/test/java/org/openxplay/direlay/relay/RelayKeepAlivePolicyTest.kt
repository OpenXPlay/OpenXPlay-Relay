// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test

class RelayKeepAlivePolicyTest {
    private var starts = 0
    private var stops = 0
    private var accepted = true
    private val policy = RelayKeepAlivePolicy(start = { starts++; accepted }, stop = { stops++ })

    @Test fun eachOutputCombinationNeedsAnOpenRuntime() {
        for (web in listOf(false, true)) for (carlink in listOf(false, true))
            for (dlna in listOf(false, true)) for (miracast in listOf(false, true)) {
                assertEquals(web || carlink || dlna || miracast, RelayKeepAlivePolicy.needed(true, web, carlink, dlna, miracast))
                assertFalse(RelayKeepAlivePolicy.needed(false, web, carlink, dlna, miracast))
            }
    }

    @Test fun repeatedDecisionsAndVisibleSignalsDoNotChurn() {
        policy.update(false)
        policy.uiVisible()
        assertEquals(0, starts)
        assertEquals(0, stops)
        policy.update(true)
        repeat(3) { policy.update(true); policy.uiVisible() }
        assertEquals(1, starts)
        policy.foregroundStarted()
        repeat(3) { policy.update(true); policy.uiVisible() }
        assertEquals(1, starts)
        policy.update(false)
        repeat(3) { policy.update(false); policy.uiVisible() }
        assertEquals(1, stops)
        assertFalse(policy.pending)
    }

    @Test fun refusedStartStaysPendingUntilTheUiIsVisible() {
        accepted = false
        policy.update(true)
        assertTrue(policy.pending)
        repeat(3) { policy.update(true) }
        assertEquals(1, starts)
        assertEquals(0, stops)
        policy.uiVisible()
        assertEquals(2, starts)
        assertTrue(policy.pending)
        accepted = true
        policy.uiVisible()
        policy.foregroundStarted()
        assertEquals(3, starts)
        assertFalse(policy.pending)
        policy.update(false)
        assertEquals(1, stops)
    }

    @Test fun theNextPolicyChangeRetriesARefusedStart() {
        accepted = false
        policy.update(true)
        policy.update(false)
        assertFalse(policy.pending)
        assertEquals(0, stops)
        accepted = true
        policy.update(true)
        policy.foregroundStarted()
        assertEquals(2, starts)
        assertFalse(policy.pending)
    }

    @Test fun disablingWhilePendingClearsTheStartWithoutStopping() {
        accepted = false
        policy.update(true)
        policy.update(false)
        accepted = true
        policy.uiVisible()
        assertFalse(policy.pending)
        assertEquals(1, starts)
        assertEquals(0, stops)
    }

    @Test fun foregroundRefusalAfterAnAcceptedRequestIsPendingAgain() {
        policy.update(true)
        policy.foregroundRefused()
        assertTrue(policy.pending)
        assertEquals(1, starts)
        assertEquals(0, stops)
        policy.uiVisible()
        policy.foregroundStarted()
        assertEquals(2, starts)
        policy.update(false)
        assertEquals(1, stops)
    }

    @Test fun closeDuringStartStopsOnlyAfterForegroundSuccess() {
        policy.update(true)
        policy.update(false)
        assertEquals(0, stops)
        assertFalse(policy.pending)
        policy.foregroundStarted()
        assertEquals(1, stops)
        policy.foregroundStarted()
        assertEquals(1, stops)
    }

    @Test fun closeDuringStartThenRefusalNeverStopsOrRetries() {
        policy.update(true)
        policy.update(false)
        policy.foregroundRefused()
        policy.uiVisible()
        assertEquals(1, starts)
        assertEquals(0, stops)
        assertFalse(policy.pending)
    }

    @Test fun enablingAgainDuringStartDoesNotStartTwice() {
        policy.update(true)
        policy.update(false)
        policy.update(true)
        policy.uiVisible()
        policy.foregroundStarted()
        assertEquals(1, starts)
        assertEquals(0, stops)
        policy.update(false)
        assertEquals(1, stops)
    }
    @Test fun publishedStateDistinguishesPendingForegroundRefusedAndNotNeeded() {
        assertEquals(org.openxplay.direlay.path.KeepAliveState.NOT_NEEDED, policy.state)
        assertEquals(org.openxplay.direlay.path.KeepAliveState.NOT_NEEDED, RelayKeepAlive.None.state)
        policy.update(true)
        assertEquals(org.openxplay.direlay.path.KeepAliveState.PENDING, policy.state)
        policy.foregroundStarted()
        assertEquals(org.openxplay.direlay.path.KeepAliveState.FOREGROUND, policy.state)
        policy.foregroundRefused()
        assertEquals(org.openxplay.direlay.path.KeepAliveState.REFUSED, policy.state)
        policy.uiVisible()
        assertEquals(org.openxplay.direlay.path.KeepAliveState.PENDING, policy.state)
        policy.update(false)
        assertEquals(org.openxplay.direlay.path.KeepAliveState.NOT_NEEDED, policy.state)
    }
}
