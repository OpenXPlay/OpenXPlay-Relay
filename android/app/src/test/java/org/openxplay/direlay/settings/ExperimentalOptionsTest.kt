// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.settings

import org.junit.Assert.*
import org.junit.Test

class ExperimentalOptionsTest {
    private val selected = ExperimentalOptions(active = true, rememberSubscription = true, keepScanning = true,
        recordAdvertisements = true, sendBluetoothAddress = true, iapChannelWhenOffered = true, releaseBluetoothOnRequest = true,
            isolateCarlinkNetwork = true, waitForCarlink = true, recoverWirelessNetwork = true, bluetoothAddress = "A0:B1:C2:D3:E4:F5")

    @Test fun onlyWifiRecoveryDefaultsOnAndTheMasterDoesNotDisablePromotedFeatures() {
        val defaults = ExperimentalOptions()
        assertTrue(defaults.active && defaults.carlinkWifiRecovery)
        assertFalse(defaults.sendBluetoothAddress)
        assertEquals(1, defaults.optionCount)
        val disabled = selected.copy(active = false).effective()
        assertFalse(disabled.carlinkWifiRecovery || disabled.sendBluetoothAddress)
        assertTrue(disabled.names().isEmpty())
        assertTrue(disabled.remotePlaybackProof && disabled.cancelRecoveredRetries && disabled.startupStatus &&
            disabled.startupDiagnostics && disabled.networkObservationBackoff && disabled.screenSessionIsolation &&
            disabled.screenReconnectReset && disabled.screenDeliverySerialization && disabled.iapChannelWhenOffered &&
            disabled.releaseBluetoothOnRequest && disabled.awaitIphoneReconnect && disabled.radioConflictPrompt && disabled.stallWatchdog)
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
        assertEquals(2, selected.effective().names().size)
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

    @Test fun historicalDisabledChoicesCannotDisablePromotedFeatures() {
        val old = ExperimentalOptions(active = false, remotePlaybackProof = false, cancelRecoveredRetries = false,
            startupStatus = false, startupDiagnostics = false, networkObservationBackoff = false,
            screenSessionIsolation = false, screenReconnectReset = false, screenDeliverySerialization = false,
            iapChannelWhenOffered = false, releaseBluetoothOnRequest = false, awaitIphoneReconnect = false,
            radioConflictPrompt = false, stallWatchdog = false).effective()
        assertTrue(old.remotePlaybackProof && old.cancelRecoveredRetries && old.startupStatus && old.startupDiagnostics &&
            old.networkObservationBackoff && old.screenSessionIsolation && old.screenReconnectReset &&
            old.screenDeliverySerialization && old.iapChannelWhenOffered && old.releaseBluetoothOnRequest &&
            old.awaitIphoneReconnect && old.radioConflictPrompt && old.stallWatchdog)
        assertFalse(old.carlinkWifiRecovery)
    }
}
