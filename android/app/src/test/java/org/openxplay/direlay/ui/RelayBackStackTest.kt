// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test

class RelayBackStackTest {
    @Test fun navigationKeepsSectionsAndSnapshotParents() {
        val stack = RelayBackStack().open("settings:network").open("developer").open("about")
        assertEquals("settings", stack.back().current)
        assertEquals("home", stack.back().back().current)
        assertFalse(RelayBackStack().canBack)
        assertEquals(RelayBackStack(), RelayBackStack().back())
        val vehicle = RelayBackStack().open("vehicle").open("snapshot:123")
        assertEquals("vehicle", vehicle.back().current)
        assertEquals(vehicle, vehicle.open("snapshot:123"))
        assertEquals(vehicle, RelayBackStack(vehicle.entries.toList()))
    }
    @Test fun identityBackReturnsToSettingsAndRetainsTheOrigin() {
        val origin = RelayBackStack().open("setup")
        for (entry in listOf("settings/identity", "settings:identity", "developer/identity")) {
            val stack = origin.open("developer/video").open(entry)
            assertEquals(listOf("home", "setup", "settings", "settings/identity"), stack.entries)
            assertEquals("settings", stack.back().current)
            assertEquals(origin, stack.back().back())
            assertEquals(stack, stack.open(entry))
        }
    }

}
