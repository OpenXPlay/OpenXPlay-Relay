// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.settings

import android.Manifest
import com.shilapi.xcertplay.orchestration.WirelessHotspotMode
import org.junit.Assert.*
import org.junit.Test

class StartupPermissionsTest {
    @Test fun carlinkRequestsDiscoveryEvenForUsbInput() {
        assertEquals(setOf(Manifest.permission.ACCESS_FINE_LOCATION), StartupPermissions.carlink(30, true).toSet())
        assertEquals(setOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT,
            Manifest.permission.ACCESS_FINE_LOCATION), StartupPermissions.carlink(31, true).toSet())
        assertEquals(setOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT,
            Manifest.permission.NEARBY_WIFI_DEVICES), StartupPermissions.carlink(33, true).toSet())
        assertTrue(StartupPermissions.carlink(37, true).contains("android.permission.ACCESS_LOCAL_NETWORK"))
        assertTrue(StartupPermissions.carlink(33, false).isEmpty())
    }

    private fun required(sdk: Int, wireless: Boolean = true, mode: WirelessHotspotMode = WirelessHotspotMode.WIFI_P2P,
                         location: Boolean = false) = StartupPermissions.required(sdk, wireless, mode, location).toSet()

    @Test fun usbRequestsOnlyMicrophoneNotificationsAndAndroid17LocalNetwork() {
        assertEquals(setOf(Manifest.permission.RECORD_AUDIO), required(26, false))
        assertEquals(setOf(Manifest.permission.RECORD_AUDIO, Manifest.permission.POST_NOTIFICATIONS), required(33, false))
        assertEquals(required(33, false) + "android.permission.ACCESS_LOCAL_NETWORK", required(37, false))
    }

    @Test fun wirelessDiscoveryUsesLocationBeforeAndroid13AndNearbyWifiAfterward() {
        for (sdk in listOf(26, 30, 31, 32)) {
            val permissions = required(sdk)
            assertTrue(Manifest.permission.ACCESS_FINE_LOCATION in permissions)
            assertTrue(Manifest.permission.ACCESS_COARSE_LOCATION in permissions)
            assertEquals(sdk >= 31, Manifest.permission.BLUETOOTH_CONNECT in permissions)
            assertFalse(Manifest.permission.NEARBY_WIFI_DEVICES in permissions)
        }
        for (sdk in listOf(33, 36, 37)) {
            val permissions = required(sdk)
            assertTrue(Manifest.permission.BLUETOOTH_CONNECT in permissions)
            assertTrue(Manifest.permission.NEARBY_WIFI_DEVICES in permissions)
            assertFalse(Manifest.permission.ACCESS_FINE_LOCATION in permissions)
            assertEquals(sdk >= 37, "android.permission.ACCESS_LOCAL_NETWORK" in permissions)
        }
    }

    @Test fun startupNeverAsksForBluetoothAdvertiseSoTheRelayStartsWithoutIt() {
        for (sdk in 26..37) for (wireless in listOf(false, true)) for (mode in WirelessHotspotMode.entries)
            for (location in listOf(false, true)) for (carlink in listOf(false, true)) {
                val permissions = StartupPermissions.required(sdk, wireless, mode, location, carlink)
                assertFalse("sdk $sdk wireless $wireless $mode", Manifest.permission.BLUETOOTH_ADVERTISE in permissions)
            }
    }

    @Test fun existingWifiDoesNotRequestDiscoveryPermissionsUnlessLocationReportingNeedsThem() {
        assertEquals(required(30, false), required(30, mode = WirelessHotspotMode.EXISTING_WIFI))
        val expected = required(33, false) + Manifest.permission.BLUETOOTH_CONNECT
        assertEquals(expected, required(33, mode = WirelessHotspotMode.EXISTING_WIFI))
        assertEquals(expected + setOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION),
            required(33, mode = WirelessHotspotMode.EXISTING_WIFI, location = true))
    }
}
