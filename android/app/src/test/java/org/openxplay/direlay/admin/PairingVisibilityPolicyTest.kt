// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.admin

import android.Manifest
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.admin.PairingVisibilityPolicy.Route

class PairingVisibilityPolicyTest {
    private val advertise = Manifest.permission.BLUETOOTH_ADVERTISE
    private val connect = Manifest.permission.BLUETOOTH_CONNECT

    @Test fun onlyAndroid12AndLaterNeedNearbyDevicesForTheDialog() {
        for (sdk in listOf(26, 29, 30)) {
            assertTrue(PairingVisibilityPolicy.dialogPermissions(sdk).isEmpty())
            assertTrue(PairingVisibilityPolicy.dialogAllowed(sdk) { false })
        }
        for (sdk in listOf(31, 33, 37)) {
            assertEquals(listOf(advertise, connect), PairingVisibilityPolicy.dialogPermissions(sdk))
            assertFalse(PairingVisibilityPolicy.dialogAllowed(sdk) { false })
            assertTrue(PairingVisibilityPolicy.dialogAllowed(sdk) { true })
        }
    }

    @Test fun asksOnlyForWhatIsStillMissing() {
        assertEquals("wireless CarPlay already holds BLUETOOTH_CONNECT", listOf(advertise),
            PairingVisibilityPolicy.missing(33) { it == connect })
        assertFalse(PairingVisibilityPolicy.dialogAllowed(33) { it == connect })
        assertEquals(listOf(connect), PairingVisibilityPolicy.missing(31) { it == advertise })
        assertTrue(PairingVisibilityPolicy.missing(30) { false }.isEmpty())
    }

    @Test fun dialogAsksForTheWindowCappedToTheSystemLimit() {
        assertEquals(300, PairingVisibilityPolicy.dialogSeconds())
        assertEquals(PairingVisibilityPolicy.DIALOG_MAX_SECONDS, PairingVisibilityPolicy.dialogSeconds(PairingWindow.SECONDS))
        assertEquals(120, PairingVisibilityPolicy.dialogSeconds(120))
        assertEquals(1, PairingVisibilityPolicy.dialogSeconds(0))
    }

    @Test fun dialogResultBecomesTheVisibleSeconds() {
        assertEquals(300, PairingVisibilityPolicy.visibleSeconds(300, 300))
        assertEquals("a system that shortens the request reports it", 120, PairingVisibilityPolicy.visibleSeconds(120, 300))
        assertEquals("RESULT_OK keeps the requested time", 300, PairingVisibilityPolicy.visibleSeconds(-1, 300))
        assertEquals("RESULT_CANCELED means declined", 0, PairingVisibilityPolicy.visibleSeconds(0, 300))
        assertEquals(0, PairingVisibilityPolicy.visibleSeconds(-7, 300))
    }

    @Test fun backgroundRequestsUseANotificationAndNeverFailSilently() {
        assertEquals(Route.SCREEN, PairingVisibilityPolicy.route(foreground = true, notificationsEnabled = false))
        assertEquals(Route.SCREEN, PairingVisibilityPolicy.route(foreground = true, notificationsEnabled = true))
        assertEquals(Route.NOTIFICATION, PairingVisibilityPolicy.route(foreground = false, notificationsEnabled = true))
        assertEquals(Route.NONE, PairingVisibilityPolicy.route(foreground = false, notificationsEnabled = false))
    }
}
