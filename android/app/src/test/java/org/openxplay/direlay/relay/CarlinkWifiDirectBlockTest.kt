// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.relay

import org.junit.Assert.*
import org.junit.Test

class CarlinkWifiDirectBlockTest {
    @Test fun wifiDirectSessionBlocksCarlinkOnlyWhileItRuns() {
        val block = CarlinkWifiDirectBlock()
        assertFalse(block.sessionChanged(false))
        block.carPlayStarting(wifiDirect = true)
        assertFalse(block.blocked(carlink = true, startConflict = false, running = false))
        // The service comes up after the start request: re-evaluate once, now blocked.
        assertTrue(block.sessionChanged(true))
        assertFalse(block.sessionChanged(true))
        assertTrue(block.blocked(carlink = true, startConflict = false, running = true))
        assertFalse(block.blocked(carlink = false, startConflict = false, running = true))
        // The session stops: re-evaluate once, and Carlink may start without a settings change.
        assertTrue(block.sessionChanged(false))
        assertFalse(block.sessionChanged(false))
        assertFalse(block.blocked(carlink = true, startConflict = false, running = false))
    }

    @Test fun hotspotSessionNeverBlocksButANextStartOnWifiDirectDoes() {
        val block = CarlinkWifiDirectBlock()
        block.carPlayStarting(wifiDirect = false)
        assertTrue(block.sessionChanged(true))
        assertFalse(block.blocked(carlink = true, startConflict = false, running = true))
        assertTrue(block.blocked(carlink = true, startConflict = true, running = false))
    }
}
