// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.network

import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.WirelessNetworkChoice

class NetworkReadinessTest {
    @Test fun everyOutputTransportNetworkAndRadioStateCombination() {
        val states = listOf(true, false, null)
        fun expected(required: Boolean, state: Boolean?) = when {
            !required || state == true -> Readiness.OK
            state == false -> Readiness.PROBLEM
            else -> Readiness.UNKNOWN
        }
        for (carlink in listOf(false, true)) for (miracast in listOf(false, true)) for (wireless in listOf(false, true))
            for (choice in WirelessNetworkChoice.entries) for (wifi in states) for (hotspot in states) for (bluetooth in states) {
                val actual = NetworkReadiness.read(carlink, wireless, choice == WirelessNetworkChoice.PHONE_HOTSPOT,
                    NetworkReadings(wifi, hotspot, bluetooth = bluetooth), miracast = miracast,
                    wifiDirect = choice == WirelessNetworkChoice.WIFI_DIRECT, sharedWifi = choice == WirelessNetworkChoice.EXISTING_WIFI)
                assertEquals(expected(carlink || miracast || wireless && choice != WirelessNetworkChoice.PHONE_HOTSPOT, wifi), actual.wifi)
                assertEquals(expected(wireless || carlink, bluetooth), actual.bluetooth)
                assertEquals(expected(wireless && choice == WirelessNetworkChoice.PHONE_HOTSPOT, hotspot), actual.hotspot)
            }
    }

    @Test fun unreadableStateIsUnknownAndIrrelevantReadersAreNeverCalled() {
        var reads = 0
        val unreadable = object : NetworkStateReader {
            override fun bluetoothEnabled(): Boolean? { reads++; throw SecurityException() }
            override fun wifiEnabled(): Boolean? { reads++; error("not readable") }
            override fun hotspotEnabled(): Boolean? { reads++; throw SecurityException() }
        }
        assertEquals(NetworkReadiness(), NetworkReadiness.read(false, false, true, unreadable))
        assertEquals(0, reads)
        assertEquals(NetworkReadiness(Readiness.UNKNOWN, Readiness.UNKNOWN, Readiness.UNKNOWN), NetworkReadiness.read(true, true, true, unreadable))
        assertEquals(3, reads)
    }

    @Test fun onlyWifiTethersCountAndUnknownInterfaceNamesStayUnknown() {
        assertNull(wifiTetheringEnabled(null))
        assertEquals(false, wifiTetheringEnabled(emptyList()))
        assertEquals(false, wifiTetheringEnabled(listOf("rndis0", "bt-pan", "usb0")))
        for (name in listOf("wlan0", "wlan1", "ap0", "softap0", "swlan0", "wifi0"))
            assertEquals(true, wifiTetheringEnabled(listOf("usb0", name)))
        assertNull(wifiTetheringEnabled(listOf("vendor_tether")))
        assertNull(wifiTetheringEnabled(listOf("usb0", "vendor_tether")))
    }
    @Test fun sharedWifiRequiresASavedNetworkAndOnlyForWireless() {
        val radios = NetworkReadings(wifi = true, bluetooth = true)
        val missing = NetworkReadiness.read(false, true, false, radios, sharedWifi = true, sharedWifiJoined = false)
        assertEquals(setOf(NetworkRequirement.SHARED_WIFI), missing.problems)
        assertTrue(NetworkReadiness.read(false, true, false, radios, sharedWifi = true, sharedWifiJoined = true).problems.isEmpty())
        assertTrue(NetworkReadiness.read(false, false, false, radios, sharedWifi = true, sharedWifiJoined = false).problems.isEmpty())
        assertTrue(NetworkReadiness.read(false, true, false, radios, sharedWifiJoined = false).problems.isEmpty())
    }
}
