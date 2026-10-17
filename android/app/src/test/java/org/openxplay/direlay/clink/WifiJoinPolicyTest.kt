// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.clink

import org.junit.Assert.*
import org.junit.Test

class WifiJoinPolicyTest {
    @Test fun prefersP2pCredentialsAndFallsBackOnlyOnSupportedAndroid() {
        assertEquals(WifiJoinPolicy.Method.UNSUPPORTED, WifiJoinPolicy.first(28, true))
        assertEquals(WifiJoinPolicy.Method.P2P_CREDENTIALS, WifiJoinPolicy.first(29, true))
        assertEquals(WifiJoinPolicy.Method.NETWORK_SPECIFIER, WifiJoinPolicy.first(33, false))
        assertEquals(WifiJoinPolicy.Method.NETWORK_SPECIFIER,
            WifiJoinPolicy.fallback(29, WifiJoinPolicy.Method.P2P_CREDENTIALS))
        assertEquals(WifiJoinPolicy.Method.UNSUPPORTED,
            WifiJoinPolicy.fallback(29, WifiJoinPolicy.Method.NETWORK_SPECIFIER))
    }
    @Test fun reservationAlwaysBypassesP2pOnSupportedApis() {
        for (sdk in listOf(28, 29, 30, 33, 36)) {
            for (available in listOf(false, true)) {
                for (reserved in listOf(false, true)) {
                    val reservation = P2pReservation { reserved }
                    val actual = WifiJoinPolicy.first(sdk, available && !reservation.reservedByCarPlay())
                    val expected = when {
                        sdk < 29 -> WifiJoinPolicy.Method.UNSUPPORTED
                        available && !reserved -> WifiJoinPolicy.Method.P2P_CREDENTIALS
                        else -> WifiJoinPolicy.Method.NETWORK_SPECIFIER
                    }
                    assertEquals("sdk=$sdk available=$available reserved=$reserved", expected, actual)
                }
            }
        }
        assertEquals(WifiJoinPolicy.Method.UNSUPPORTED,
            WifiJoinPolicy.fallback(28, WifiJoinPolicy.Method.P2P_CREDENTIALS))
    }
    @Test fun softApReplyAlwaysUsesSpecifierAndP2pReplyKeepsExistingPolicy() {
        for (sdk in listOf(28, 29, 33, 36)) {
            for (available in listOf(false, true)) {
                assertEquals(if (sdk < 29) WifiJoinPolicy.Method.UNSUPPORTED else WifiJoinPolicy.Method.NETWORK_SPECIFIER,
                    WifiJoinPolicy.first(sdk, available, 1002))
                assertEquals(WifiJoinPolicy.first(sdk, available), WifiJoinPolicy.first(sdk, available, 1001))
            }
        }
    }
    @Test fun mayRemoveOnlyObservedHeadUnitClientGroup() {
        for (formed in listOf(false, true)) {
            for (owner in listOf(false, true)) {
                for (network in listOf(null, "", "iPhone-group", "head-unit")) {
                    assertEquals(formed && !owner && network == "head-unit",
                        WifiJoinPolicy.mayRemoveGroup(formed, owner, network, "head-unit"))
                }
            }
        }
    }
    @Test fun endpointsUseObservedAddressesAndNeverSupplyDefaultPeer() {
        assertEquals("10.71.4.9" to "10.71.4.18", WifiJoinPolicy.endpoints("10.71.4.9", "10.71.4.18"))
        for (invalid in listOf(null, "", "192.168.49.999", "0.0.0.0", "127.0.0.1", "224.0.0.1", "169.254.4.3", "::1")) {
            assertNull(WifiJoinPolicy.endpoints(invalid, "10.71.4.18"))
            assertNull(WifiJoinPolicy.endpoints("10.71.4.9", invalid))
        }
        assertNull(WifiJoinPolicy.endpoints("10.71.4.9", "10.71.4.9"))
    }
}
