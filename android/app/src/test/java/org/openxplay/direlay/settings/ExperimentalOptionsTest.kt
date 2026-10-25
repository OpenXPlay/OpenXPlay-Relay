// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.settings

import org.junit.Assert.*
import org.junit.Test

class ExperimentalOptionsTest {
    private val selected = ExperimentalOptions(active = true, rememberSubscription = true, keepScanning = true,
        recordAdvertisements = true, sendBluetoothAddress = true,
            isolateCarlinkNetwork = true, waitForCarlink = true, recoverWirelessNetwork = true, bluetoothAddress = "A0:B1:C2:D3:E4:F5")

    @Test fun defaultsAndMasterOffDisableEveryOptionWithoutLosingChoices() {
        assertEquals(ExperimentalOptions(), ExperimentalOptions().effective())
        val disabled = selected.copy(active = false)
        assertEquals(ExperimentalOptions(), disabled.effective())
        assertTrue(disabled.effective().names().isEmpty())
        assertEquals(7, disabled.optionCount)
        assertEquals(selected.effective(), disabled.copy(active = true).effective())
    }

    @Test fun traceAndAddressValidityGateOnlyTheirOwnOptions() {
        val effective = selected.effective(carlinkTrace = false)
        assertTrue(effective.rememberSubscription); assertTrue(effective.keepScanning)
        assertFalse(effective.recordAdvertisements); assertTrue(effective.sendBluetoothAddress)
        assertEquals("a0:b1:c2:d3:e4:f5", effective.bluetoothAddress)
        val invalid = selected.copy(bluetoothAddress = "bad")
        assertTrue(invalid.invalidAddress)
        assertFalse(invalid.effective().sendBluetoothAddress)
        assertTrue(invalid.effective().recordAdvertisements)
        assertFalse(invalid.copy(active = false).invalidAddress)
        assertFalse(invalid.copy(sendBluetoothAddress = false).invalidAddress)
        assertEquals(listOf("remember_subscription", "keep_scanning", "record_advertisements", "send_bluetooth_address",
            "isolate_carlink_network", "wait_for_carlink", "recover_wireless_network"), selected.effective().names())
    }

    @Test fun addressRequiresSixHexUnicastOctetsAndRejectsHiddenAndZeroAddresses() {
        for (address in listOf("", "00:00:00:00:00:00", "02:00:00:00:00:00", "01:23:45:67:89:ab",
            "ff:ff:ff:ff:ff:ff", "A0:B1:C2:D3:E4", "A0:B1:C2:D3:E4:F5:00", "A:B1:C2:D3:E4:F5",
            "GG:B1:C2:D3:E4:F5", "A0-B1-C2-D3-E4-F5", " A0:B1:C2:D3:E4:F5", "A0:B1:C2:D3:E4:F5\n"))
            assertNull(address, ExperimentalOptions.normalizedBluetoothAddress(address))
        for (address in listOf("00:11:22:33:44:55", "02:11:22:33:44:55", "A0:B1:C2:D3:E4:F5"))
            assertEquals(address.lowercase(), ExperimentalOptions.normalizedBluetoothAddress(address))
    }

    @Test fun connectionExperimentsCanBeSelectedIndependentlyAndNeedTheMasterSwitch() {
        for (isolate in listOf(false, true)) for (wait in listOf(false, true)) for (recover in listOf(false, true)) {
            val choices = ExperimentalOptions(active = true, isolateCarlinkNetwork = isolate,
                waitForCarlink = wait, recoverWirelessNetwork = recover)
            assertEquals(choices, choices.effective(carlinkTrace = false))
            assertEquals(listOf(isolate, wait, recover).count { it }, choices.optionCount)
            assertEquals(ExperimentalOptions(), choices.copy(active = false).effective())
        }
    }
}
