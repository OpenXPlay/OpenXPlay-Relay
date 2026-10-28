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

    @Test fun newLaboratoryOptionsDefaultOnAndPromotedFeaturesSurviveMasterOff() {
        val defaults = ExperimentalOptions()
        assertTrue(defaults.active); assertFalse(defaults.sendBluetoothAddress)
        assertEquals(8, defaults.optionCount)
        assertEquals(ExperimentalOptions.OFF, defaults.copy(active = false).effective())
        val disabled = selected.copy(active = false).effective()
        assertEquals(ExperimentalOptions.OFF, disabled)
        assertEquals(6, disabled.names().size)
        assertTrue(disabled.recoverWirelessNetwork && disabled.keepScanning && disabled.waitForCarlink)
        assertFalse(disabled.remotePlaybackProof || disabled.cancelRecoveredRetries || disabled.startupStatus ||
            disabled.startupDiagnostics || disabled.networkObservationBackoff || disabled.screenSessionIsolation ||
            disabled.screenReconnectReset || disabled.screenDeliverySerialization || disabled.sendBluetoothAddress)
        assertEquals(selected.effective(), selected.copy(active = false).copy(active = true).effective())
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
        assertEquals(15, selected.effective().names().size)
    }

    @Test fun addressRequiresSixHexUnicastOctetsAndRejectsHiddenAndZeroAddresses() {
        for (address in listOf("", "00:00:00:00:00:00", "02:00:00:00:00:00", "01:23:45:67:89:ab",
            "ff:ff:ff:ff:ff:ff", "A0:B1:C2:D3:E4", "A0:B1:C2:D3:E4:F5:00", "A:B1:C2:D3:E4:F5",
            "GG:B1:C2:D3:E4:F5", "A0-B1-C2-D3-E4-F5", " A0:B1:C2:D3:E4:F5", "A0:B1:C2:D3:E4:F5\n"))
            assertNull(address, ExperimentalOptions.normalizedBluetoothAddress(address))
        for (address in listOf("00:11:22:33:44:55", "02:11:22:33:44:55", "A0:B1:C2:D3:E4:F5"))
            assertEquals(address.lowercase(), ExperimentalOptions.normalizedBluetoothAddress(address))
    }

    @Test fun retiredChoicesCannotDisableNormalFeatures() {
        val old = ExperimentalOptions(active = false, rememberSubscription = false, keepScanning = false,
            recordAdvertisements = false, isolateCarlinkNetwork = false, waitForCarlink = false, recoverWirelessNetwork = false)
        val enabled = old.effective()
        assertTrue(enabled.rememberSubscription && enabled.keepScanning && enabled.recordAdvertisements &&
            enabled.isolateCarlinkNetwork && enabled.waitForCarlink && enabled.recoverWirelessNetwork)
        assertFalse(old.effective(carlinkTrace = false).recordAdvertisements)
    }

    @Test fun screenExperimentsRemainIndependentAndRespectTheMaster() {
        for (bits in 0..7) {
            val selected = ExperimentalOptions(screenSessionIsolation = bits and 1 != 0,
                screenReconnectReset = bits and 2 != 0, screenDeliverySerialization = bits and 4 != 0)
            val enabled = selected.effective()
            assertEquals(selected.screenSessionIsolation, enabled.screenSessionIsolation)
            assertEquals(selected.screenReconnectReset, enabled.screenReconnectReset)
            assertEquals(selected.screenDeliverySerialization, enabled.screenDeliverySerialization)
            val disabled = selected.copy(active = false).effective()
            assertFalse(disabled.screenSessionIsolation || disabled.screenReconnectReset || disabled.screenDeliverySerialization)
            assertEquals(enabled, selected.copy(active = false).copy(active = true).effective())
        }
    }
}
