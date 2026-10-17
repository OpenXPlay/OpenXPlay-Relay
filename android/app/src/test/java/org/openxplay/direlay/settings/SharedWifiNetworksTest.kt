// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.settings

import org.junit.Assert.*
import org.junit.Test

class SharedWifiNetworksTest {
    private val networks = listOf(SharedWifiNetwork("Home:家", "password"), SharedWifiNetwork("Car", ""))
    @Test fun orderedPersistenceAndOneTimeMigration() {
        val encoded = SharedWifiNetworks.encode(networks)
        assertEquals(networks, SharedWifiNetworks.decode(encoded))
        assertEquals(networks, SharedWifiNetworks.migrate(encoded, "Legacy", "old-password"))
        assertEquals(listOf(SharedWifiNetwork("Legacy", "old-password")), SharedWifiNetworks.migrate(null, "Legacy", "old-password"))
        assertEquals(emptyList<SharedWifiNetwork>(), SharedWifiNetworks.migrate("", "Legacy", "old-password"))
        assertEquals(emptyList<SharedWifiNetwork>(), SharedWifiNetworks.migrate(null, "", ""))
    }
    @Test fun exactUniqueNamesAndTenEntryLimit() {
        SharedWifiNetworks.validate((1..10).map { SharedWifiNetwork("Wi-Fi $it", "") })
        SharedWifiNetworks.validate(listOf(SharedWifiNetwork("Home", ""), SharedWifiNetwork("home", "")))
        for (invalid in listOf(networks + networks[0], (1..11).map { SharedWifiNetwork("Wi-Fi $it", "") }, listOf(SharedWifiNetwork("x", "short"))))
            assertTrue(runCatching { SharedWifiNetworks.validate(invalid) }.isFailure)
    }
    @Test fun resolvesCurrentWifiAndClearsManualChoiceOnChange() {
        assertEquals(networks[1], resolveSharedWifi("Car", networks))
        assertNull(resolveSharedWifi("car", networks))
        assertNull(resolveSharedWifi(null, networks))
        val selection = SharedWifiSelection()
        selection.use("Car")
        assertEquals(networks[1], selection.resolve(null, networks))
        assertEquals(networks[0], selection.resolve("Home:家", networks))
        assertNull(selection.resolve(null, networks))
        selection.use("Car"); selection.wifiChanged()
        assertNull(selection.resolve(null, networks))
    }
    @Test fun diagnosticsNeverContainPasswords() { assertFalse(networks.toString().contains("password")) }
}
