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

    @Test fun homeIsAlwaysTheUniqueRootIncludingAfterRestoration() {
        val stack = RelayBackStack().open("setup").open("settings:network").open("vehicle")
        assertEquals(listOf("home"), stack.open("home").entries)
        assertEquals(listOf("home"), stack.open("home#arrival").entries)
        assertEquals(listOf("home"), RelayBackStack(emptyList()).entries)
        assertEquals(listOf("home", "vehicle"), RelayBackStack(listOf("vehicle")).entries)
        assertEquals(listOf("home", "settings", "vehicle"),
            RelayBackStack(listOf("home", "setup", "home", "settings", "vehicle", "snapshot:1", "vehicle")).entries)
    }

    @Test fun reopeningAnyPagePopsToItsExistingInstance() {
        val vehicle = RelayBackStack().open("vehicle")
        assertEquals(vehicle, vehicle.open("settings:car").open("snapshot:1").open("vehicle"))
        val snapshot = vehicle.open("snapshot:1")
        assertEquals(snapshot, snapshot.open("snapshot:2").open("settings").open("snapshot:1"))
        assertEquals(listOf("home", "settings", "settings/connection#iphone"),
            RelayBackStack().open("settings:connection").open("settings:sharedWifi").open("settings:iphone").entries)
    }

    @Test fun setupStepsSwitchWithinOnePageAndCompletionResetsHome() {
        var stack = RelayBackStack().open("setup:car").open("settings:identity").open("setup:network")
        assertEquals(listOf("home", "setup:network"), stack.entries)
        stack = stack.open("setup:prepare").open("setup:start")
        assertEquals(listOf("home", "setup:start"), stack.entries)
        assertEquals(listOf("home"), stack.open("home").entries)
        assertEquals(listOf("home"), stack.back().entries)
        assertEquals("start", relaySetupStep("start", "prepare", "prepare"))
        assertEquals("car", relaySetupStep("start", "prepare", "car"))
        assertEquals("start", relaySetupStep("start", "prepare", null))
        assertEquals("network", relaySetupBack("prepare", "prepare", "network"))
        assertEquals("car", relaySetupBack("prepare", "car", "network"))
    }

    @Test fun settingsBranchesStillReplaceSiblingsAndKeepTheirOrigin() {
        val origin = RelayBackStack().open("setup:prepare")
        val branch = origin.open("settings:sharedWifi").open("developer/video").open("experimental").open("settings/audio")
        assertEquals(origin.entries + listOf("settings", "settings/audio"), branch.entries)
        assertEquals(origin, branch.back().back())
    }

    @Test fun predictiveBackCannotOverwriteANewerLaunchDestination() {
        val origin = RelayBackStack().open("settings:network").entries
        val incoming = RelayBackStack(origin).open("vehicle").entries
        assertEquals(incoming, commitRelayBack(origin, incoming))
        assertEquals(RelayBackStack(origin).back().entries, commitRelayBack(origin, origin))
        assertEquals(listOf("home"), commitRelayBack(origin, listOf("home")))
    }

    @Test fun manyOpenBackAndRestoreSequencesPreserveAllInvariants() {
        val routes = listOf("home", "vehicle", "snapshot:1", "snapshot:2", "setup", "setup:car", "setup:network",
            "settings", "settings:iphone", "settings:connection", "settings:sharedWifi", "settings:car", "settings/audio",
            "developer", "developer/video", "developer/design", "experimental", "publicName", "about", "settings/tesla")
        repeat(80) { seed ->
            val random = java.util.Random(seed.toLong())
            var stack = RelayBackStack()
            repeat(250) {
                stack = if (random.nextInt(4) == 0) stack.back() else stack.open(routes[random.nextInt(routes.size)])
                assertEquals("home", stack.entries.first())
                assertEquals(1, stack.entries.count { it == "home" })
                val pages = stack.entries.map(::relayPageKey)
                assertEquals(pages.distinct(), pages)
                assertEquals(stack, RelayBackStack(stack.entries))
                var exit = stack
                repeat(stack.entries.size) { exit = exit.back() }
                assertEquals(listOf("home"), exit.entries)
            }
        }
    }

}
