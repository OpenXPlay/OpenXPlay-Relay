// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.settings

import org.junit.Assert.*
import org.junit.Test

class TeslaBrowserTest {
    @Test fun rejectsEveryExcludedRangeAndMalformedOrAssignedAddresses() {
        for (address in listOf("", "3.3.3", "3.3.3.1.2", "256.3.3.1", "-1.3.3.1", "3.3.3.1 ",
            "relay.example.org", "3.3.3.01", "3..3.1", "::1", "0.0.0.1", "0.255.255.255",
            "10.0.0.1", "10.255.255.255", "127.0.0.1", "127.255.255.255", "169.254.0.0", "169.254.255.255",
            "172.16.0.0", "172.31.255.255", "192.168.0.0", "192.168.255.255", "224.0.0.0", "239.255.255.255",
            "255.255.255.255")) assertFalse(address, TeslaAddressPolicy.valid(address))
        assertFalse(TeslaAddressPolicy.valid("3.3.3.1", setOf("3.3.3.1", "192.168.43.1")))
        assertFalse(TeslaAddressPolicy.valid("100.88.0.1", setOf("100.88.0.1")))
    }

    @Test fun acceptsBothDefaultsAndAddressesJustOutsideExcludedRanges() {
        assertEquals("100.88.0.1", TeslaAddressPolicy.DEFAULT_VIRTUAL)
        assertEquals("3.3.3.1", TeslaAddressPolicy.DEFAULT_PUBLIC)
        for (address in listOf("100.88.0.1", "3.3.3.1", "1.0.0.1", "169.253.255.255", "169.255.0.0",
            "172.15.255.255", "172.32.0.0", "192.167.255.255", "192.169.0.0", "223.255.255.255",
            "240.0.0.1", "255.255.255.254")) assertTrue(address, TeslaAddressPolicy.valid(address, setOf("192.168.43.1")))
    }

    @Test fun activationCoversEveryModeTransportNetworkAndNameCombination() {
        for (mode in TeslaBrowserMode.entries) for (wireless in listOf(false, true))
            for (network in WirelessNetworkChoice.entries) for (name in listOf(false, true)) {
                val expected = when {
                    mode == TeslaBrowserMode.OFF -> TeslaBrowserActivation.OFF
                    !wireless -> TeslaBrowserActivation.NEEDS_WIRELESS
                    network == WirelessNetworkChoice.EXISTING_WIFI -> TeslaBrowserActivation.NEEDS_PHONE_NETWORK
                    mode == TeslaBrowserMode.VIRTUAL_NAME && !name -> TeslaBrowserActivation.NEEDS_NAME
                    else -> TeslaBrowserActivation.READY
                }
                assertEquals("$mode wireless=$wireless network=$network name=$name", expected,
                    teslaBrowserActivation(mode, wireless, network, name))
            }
    }

    @Test fun aPublicNameOverrideExistsOnlyForAnActiveVirtualNameAddress() {
        for (mode in TeslaBrowserMode.entries) assertNull(teslaPublishedAddress(mode, null))
        assertNull(teslaPublishedAddress(TeslaBrowserMode.OFF, "100.88.0.1"))
        assertNull(teslaPublishedAddress(TeslaBrowserMode.PUBLIC_ADDRESS, "3.3.3.1"))
        assertEquals("100.88.0.2", teslaPublishedAddress(TeslaBrowserMode.VIRTUAL_NAME, "100.88.0.2"))
    }
}
