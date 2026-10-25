// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.core.SessionPhase

class AutoStartTest {
    private fun original(v: AutoStartSnapshot, f: HostFlags): Boolean =
        v.networkReady && !f.importOnly && !f.guideOpen && !v.networkStartPending && v.canStart && f.resumed &&
            !f.requestingPermissions && !f.networkRefreshPending && !v.restarting && !v.starting && !v.running &&
            !v.failed && !v.paused && v.observed && !v.trackerRunning &&
            v.phase !in setOf(SessionPhase.CONNECTING, SessionPhase.ACTIVE, SessionPhase.RECONNECTING) &&
            (v.automatic || f.bootAutoStart)

    @Test fun everyFlagCombinationMatchesTheActivityCondition() {
        for (mask in 0 until (1 shl 18)) {
            fun flag(bit: Int) = mask and (1 shl bit) != 0
            val flags = HostFlags(resumed = flag(0), guideOpen = flag(1), requestingPermissions = flag(2),
                bootAutoStart = flag(3), importOnly = flag(4), networkRefreshPending = flag(5))
            val value = AutoStartSnapshot(flag(6), flag(7), flag(8), flag(9), flag(10), flag(11), flag(12),
                flag(13), flag(14), flag(15), if (flag(16)) SessionPhase.ACTIVE else null, flag(17))
            assertEquals("mask=$mask", original(value, flags), shouldAutoStart(value, flags))
        }
    }

    @Test fun activePhasesBlockAndMaterialFlagsAddNoNewConditions() {
        val ready = AutoStartSnapshot(true, false, true, false, false, false, false, false, true, false, null, true)
        val flags = HostFlags(resumed = true)
        assertTrue(shouldAutoStart(ready, flags))
        assertTrue(shouldAutoStart(ready.copy(automatic = false), flags.copy(bootAutoStart = true)))
        assertFalse(shouldAutoStart(ready.copy(automatic = false), flags))
        for (phase in SessionPhase.entries) assertEquals(
            phase !in setOf(SessionPhase.CONNECTING, SessionPhase.ACTIVE, SessionPhase.RECONNECTING),
            shouldAutoStart(ready.copy(phase = phase), flags))
        assertTrue(shouldAutoStart(ready, flags.copy(identityReady = true, materialChecked = true, wifi = "Car")))
    }
}
