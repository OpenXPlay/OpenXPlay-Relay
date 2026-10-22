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
    @Test fun boundariesFollowCurrentWindowWidth() {
        assertEquals(WindowLayout.COMPACT, windowLayout(599))
        assertEquals(WindowLayout.MEDIUM, windowLayout(600))
        assertEquals(WindowLayout.MEDIUM, windowLayout(839))
        assertEquals(WindowLayout.EXPANDED, windowLayout(840))
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

    @Test fun shortWindowsScrollActionsRegardlessOfWidthOrOrientation() {
        for (width in listOf(320, 360, 599, 600, 820, 840, 1200)) {
            for (height in listOf(240, 360, 479)) assertTrue(homeWindow(width, height).inlineActions)
            for (height in listOf(480, 820, 1200)) assertFalse(homeWindow(width, height).inlineActions)
            assertEquals(windowLayout(width), homeWindow(width, 360).layout)
        }
        assertEquals(HomeWindow(WindowLayout.MEDIUM, true), homeWindow(820, 360))
        assertEquals(HomeWindow(WindowLayout.COMPACT, true), homeWindow(360, 360))
    }
}
