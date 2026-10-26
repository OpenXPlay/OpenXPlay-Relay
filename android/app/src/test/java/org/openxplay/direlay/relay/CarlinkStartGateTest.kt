// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
package org.openxplay.direlay.relay

import java.io.IOException
import org.junit.Assert.*
import org.junit.Test

class CarlinkStartGateTest {
    @Test fun disabledExperimentDoesNotWaitForAnOutput() {
        val gate = CarlinkStartGate()
        gate.request(100, false)
        assertTrue(gate.ready(100, false, false))
        assertNull(gate.untilMillis)
    }

    @Test fun onlyAReadyOutputReleasesTheWaitBeforeItsDeadline() {
        val gate = CarlinkStartGate()
        gate.request(100, true)
        assertFalse(gate.ready(100, false, true))
        assertFalse(gate.ready(60_099, false, true))
        assertTrue(gate.ready(60_099, true, true))
        assertNull(gate.untilMillis)
    }

    @Test fun deadlineAndDisabledOutputFailInsteadOfStartingTheIphone() {
        val gate = CarlinkStartGate()
        gate.request(100, true)
        assertThrows(IOException::class.java) { gate.ready(60_100, false, true) }
        gate.request(100, true)
        assertThrows(IOException::class.java) { gate.ready(200, true, false) }
    }

    @Test fun cancellationAndANewStartDoNotReuseTheOldWait() {
        val gate = CarlinkStartGate()
        gate.request(100, true)
        gate.cancel()
        assertTrue(gate.ready(200, false, true))
        gate.request(80_000, true)
        assertFalse(gate.ready(80_001, false, true))
        assertEquals(140_000L, gate.untilMillis)
    }
}
