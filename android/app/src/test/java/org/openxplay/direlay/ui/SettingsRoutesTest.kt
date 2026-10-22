// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import org.junit.Assert.*
import org.junit.Test

class SettingsRoutesTest {
    @Test fun everyLegacyFieldLinkOpensItsPageAndParents() {
        val routes = mapOf(
            "" to "settings", "output" to "settings/output#output", "car" to "settings/car",
            "picture" to "settings/picture", "audio" to "settings/audio",
            "connection" to "settings/connection#connection", "iphone" to "settings/connection#iphone",
            "network" to "settings/connection/network", "sharedWifi" to "settings/connection/network#sharedWifi",
            "startup" to "settings/startup", "admin" to "settings/access", "access" to "settings/access",
            "publicName" to "settings/access/publicName", "identity" to "settings/identity",
            "about" to "settings/about", "developer" to "developer")
        routes.forEach { (key, expected) ->
            val stack = RelayBackStack().open("settings:$key")
            assertEquals(key, expected, stack.current)
            assertEquals(key, listOf("home") + relayRouteParents(expected) + expected, stack.entries)
            assertEquals(stack, stack.open("settings:$key"))
        }
        assertEquals("settings", relayRoute("settings:unknown"))
    }

    @Test fun identityAliasesResolveToATopLevelSettingsPage() {
        assertEquals(listOf("settings"), relayRouteParents("settings/identity"))
        for (entry in listOf("settings:identity", "developer/identity")) {
            assertEquals("settings/identity", relayRoute(entry))
            assertEquals(listOf("home", "settings", "settings/identity"), RelayBackStack().open(entry).entries)
        }
    }

    @Test fun backWalksOneMenuLevelAtATimeAndRetainsTheOrigin() {
        var stack = RelayBackStack().open("setup").open("settings:sharedWifi")
        for (route in listOf("settings/connection/network#sharedWifi", "settings/connection", "settings", "setup", "home")) {
            assertEquals(route, stack.current)
            stack = stack.back()
        }
        stack = RelayBackStack().open("developer/carlink")
        assertEquals(listOf("home", "settings", "developer", "developer/carlink"), stack.entries)
        assertEquals("developer", stack.back().current)
        assertEquals("settings", stack.back().back().current)
    }

    @Test fun aliasesKeepTheirParentsAndSiblingsDoNotDuplicateMenus() {
        val tls = RelayBackStack().open("publicName")
        assertEquals(listOf("home", "settings", "settings/access", "settings/access/publicName"), tls.entries)
        assertEquals(listOf("home", "settings", "settings/about"), RelayBackStack().open("about").entries)
        val network = RelayBackStack().open("settings:iphone").open("settings/connection/network")
        assertEquals(listOf("home", "settings", "settings/connection#iphone", "settings/connection/network"), network.entries)
        assertEquals("settings/connection#iphone", network.back().current)
        assertEquals(listOf("home", "settings", "developer", "developer/video"), network.open("developer/video").entries)
        assertEquals(listOf("home", "settings"), network.open("settings").entries)
        assertEquals("vehicle", network.open("vehicle").current)
    }
    @Test fun settingsSessionsFollowTheActualStack() {
        assertFalse(settingsSessionActive(listOf("home", "setup")))
        assertTrue(settingsSessionActive(RelayBackStack().open("settings:iphone").open("vehicle").entries))
        assertTrue(settingsSessionActive(RelayBackStack().open("settings/identity").entries))
    }

    @Test fun diagnosticsAndIdentityStatusPollingFollowTheDestination() {
        val diagnostics = setOf("developer", "developer/diagnostics", "settings/identity")
        val routes = diagnostics + listOf("home", "setup", "vehicle", "settings", "settings/output", "settings/car",
            "settings/picture", "settings/audio", "settings/connection", "settings/connection/network", "settings/startup",
            "settings/access", "settings/access/publicName", "settings/about", "developer/carlink", "developer/video",
            "developer/connection", "developer/identity")
        for (route in routes) {
            assertEquals(route, route in diagnostics, showsDeveloperDiagnostics(route))
            assertEquals(route, route == "settings", showsIdentityStatus(route))
        }
    }

    @Test fun errorPagesFollowTheSettingsMenuOrder() {
        assertEquals("settings", firstSettingsErrorPage(emptySet()))
        val order = listOf("settings/output", "settings/connection/network", "settings/identity",
            "developer/video", "developer/connection")
        order.forEachIndexed { index, page ->
            assertEquals(page, firstSettingsErrorPage(order.drop(index).reversed().toSet()))
        }
    }
}
