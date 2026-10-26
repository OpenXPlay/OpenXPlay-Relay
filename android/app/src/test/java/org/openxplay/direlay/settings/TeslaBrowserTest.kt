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
        assertFalse(TeslaAddressPolicy.valid("169.254.88.1", setOf("169.254.88.1")))
    }

    @Test fun defaultsKeepLinkLocalAndReferenceRangeChoices() {
        assertEquals("169.254.88.1", TeslaAddressPolicy.DEFAULT_VIRTUAL)
        assertEquals("169.254.88.1", TeslaAddressPolicy.DEFAULT_PUBLIC)
        assertTrue(TeslaAddressPolicy.valid(TeslaAddressPolicy.DEFAULT_VIRTUAL))
        assertTrue(TeslaAddressPolicy.valid(TeslaAddressPolicy.DEFAULT_PUBLIC))
        assertEquals("100.88.0.1", TeslaAddressPolicy.DEFAULT_MIRROR)
        assertEquals("3.3.3.1", TeslaAddressPolicy.DEFAULT_ANDROID)
        assertTrue(TeslaAddressPolicy.valid(TeslaAddressPolicy.DEFAULT_MIRROR))
        assertTrue(TeslaAddressPolicy.valid(TeslaAddressPolicy.DEFAULT_ANDROID))
    }

    @Test fun acceptsOnlyTheRfc3927UsableLinkLocalRange() {
        for (address in listOf("169.254.0.0", "169.254.0.1", "169.254.0.255",
            "169.254.255.0", "169.254.255.254", "169.254.255.255"))
            assertFalse(address, TeslaAddressPolicy.valid(address))
        for (address in listOf("169.254.1.0", "169.254.1.255", "169.254.88.1", "169.254.254.0", "169.254.254.255"))
            assertTrue(address, TeslaAddressPolicy.valid(address))
    }

    @Test fun acceptsAddressesJustOutsideExcludedRanges() {
        for (address in listOf("100.88.0.1", "3.3.3.1", "1.0.0.1", "169.253.255.255", "169.255.0.0",
            "172.15.255.255", "172.32.0.0", "192.167.255.255", "192.169.0.0", "223.255.255.255",
            "240.0.0.1", "255.255.255.254")) assertTrue(address, TeslaAddressPolicy.valid(address, setOf("192.168.43.1")))
    }

    @Test fun nonLinkLocalAddressesRemainValidButWarnStartingAtAndroid13() {
        for (address in listOf("100.88.0.1", "3.3.3.1", "169.253.255.255", "169.255.0.0")) {
            assertTrue(address, TeslaAddressPolicy.valid(address))
            for ((sdk, warning) in mapOf(28 to false, 32 to false, 33 to true, 34 to true, 35 to true, 36 to true))
                assertEquals("sdk=$sdk address=$address", warning, TeslaAddressPolicy.needsAndroid13Warning(sdk, address))
        }
    }

    @Test fun linkLocalAndInvalidAddressesDoNotShowTheAndroid13Warning() {
        for (sdk in listOf(28, 32, 33, 34, 35, 36))
            for (address in listOf("169.254.1.0", "169.254.88.1", "169.254.254.255",
                "", "relay.example.org", "169.254.0.1", "169.254.255.0", "169.254.256.1", "10.0.0.1"))
                assertFalse("sdk=$sdk address=$address", TeslaAddressPolicy.needsAndroid13Warning(sdk, address))
    }

    @Test fun activationCoversEveryModeBrowserOutputTransportNetworkAndNameCombination() {
        for (mode in TeslaBrowserMode.entries) for (wireless in listOf(false, true))
            for (network in WirelessNetworkChoice.entries) for (name in listOf(false, true))
                for ((carlink, both) in listOf(false to false, false to true, true to true)) {
                    val expected = when {
                        mode == TeslaBrowserMode.OFF -> TeslaBrowserActivation.OFF
                        !wireless -> TeslaBrowserActivation.NEEDS_WIRELESS
                        network == WirelessNetworkChoice.EXISTING_WIFI -> TeslaBrowserActivation.NEEDS_PHONE_NETWORK
                        mode.usesTrustedName && !name -> TeslaBrowserActivation.NEEDS_NAME
                        else -> TeslaBrowserActivation.READY
                    }
                    assertEquals("$mode carlink=$carlink both=$both wireless=$wireless network=$network name=$name", expected,
                        teslaBrowserActivation(mode, wireless, network, name, browserOutput = true))
                }
    }

    @Test fun carlinkOnlyNeedsBrowserBeforeTransportNetworkOrNameChecksUnlessOff() {
        val expectedByMode = TeslaBrowserMode.entries.associateWith {
            if (it == TeslaBrowserMode.OFF) TeslaBrowserActivation.OFF else TeslaBrowserActivation.NEEDS_BROWSER
        }
        for ((mode, expected) in expectedByMode) for (wireless in listOf(false, true))
            for (network in WirelessNetworkChoice.entries) for (name in listOf(false, true)) {
                assertEquals("$mode wireless=$wireless network=$network name=$name", expected,
                    teslaBrowserActivation(mode, wireless, network, name, browserOutput = false))
            }
    }

    @Test fun aPublicNameOverrideExistsOnlyForActiveTrustedNameModes() {
        for (mode in TeslaBrowserMode.entries) {
            assertNull(teslaPublishedAddress(mode, null))
            val expected = if (mode in listOf(TeslaBrowserMode.VIRTUAL_NAME, TeslaBrowserMode.TESLAMIRROR)) "100.88.0.2" else null
            assertEquals(expected, teslaPublishedAddress(mode, "100.88.0.2"))
            assertEquals(expected != null, mode.usesTrustedName)
        }
    }
}
