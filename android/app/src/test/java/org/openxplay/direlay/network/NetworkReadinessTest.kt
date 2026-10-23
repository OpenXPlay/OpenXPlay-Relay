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
            for (choice in WirelessNetworkChoice.entries) for (wifi in states) for (hotspot in states) {
                val actual = NetworkReadiness.read(carlink, wireless, choice == WirelessNetworkChoice.PHONE_HOTSPOT,
                    NetworkReadings(wifi, hotspot), miracast = miracast)
                assertEquals(expected(carlink || miracast, wifi), actual.wifi)
                assertEquals(expected(wireless && choice == WirelessNetworkChoice.PHONE_HOTSPOT, hotspot), actual.hotspot)
            }
    }

    @Test fun unreadableStateIsUnknownAndIrrelevantReadersAreNeverCalled() {
        var reads = 0
        val unreadable = object : NetworkStateReader {
            override fun wifiEnabled(): Boolean? { reads++; error("not readable") }
            override fun hotspotEnabled(): Boolean? { reads++; throw SecurityException() }
        }
        assertEquals(NetworkReadiness(), NetworkReadiness.read(false, false, true, unreadable))
        assertEquals(0, reads)
        assertEquals(NetworkReadiness(Readiness.UNKNOWN, Readiness.UNKNOWN), NetworkReadiness.read(true, true, true, unreadable))
        assertEquals(2, reads)
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
}
