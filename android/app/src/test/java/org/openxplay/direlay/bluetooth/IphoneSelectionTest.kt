// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.bluetooth

import org.junit.Assert.*
import org.junit.Test

class IphoneSelectionTest {
    private val renamed = BondedDevice("00:11:22:33:A9:30", "Riko", true, true, false)
    private val car = BondedDevice("00:11:22:33:AA:BB", "Car", false, false, true, carClass = true)
    private val android = BondedDevice("00:11:22:33:CC:DD", "Android", true, false, true)

    @Test fun renamedIphoneWithIap2WinsOverCarBond() {
        assertEquals(IphoneSelection.OnlyCandidate(renamed), IphoneSelection.select(null, listOf(car, renamed)))
    }
    @Test fun nameEvidenceWinsOverConnectedAndroidPhoneClass() {
        val iphone = renamed.copy(name = "my IPHONE", iap2 = false)
        assertEquals(IphoneSelection.OnlyCandidate(iphone), IphoneSelection.select(null, listOf(android, iphone)))
    }
    @Test fun explicitChoiceWinsOverStrongerOrConnectedEvidence() {
        assertEquals(IphoneSelection.Selected(car), IphoneSelection.select(car.address.lowercase(), listOf(renamed, car)))
        assertEquals(car.address, IphoneSelection.select(car.address, listOf(renamed, car)).address)
    }
    @Test fun unpairedChoiceNeverFallsBackEvenToSingleStrongBond() {
        val result = IphoneSelection.select(car.address, listOf(renamed))
        assertEquals(IphoneSelection.SelectedUnpaired(car.address), result)
        assertNull(result.address)
        assertEquals(IphoneSelection.SelectedUnpaired(car.address), IphoneSelection.select(car.address, emptyList()))
    }
    @Test fun strongAndWeakCandidatesEachApplyConnectedThenBondedRules() {
        for (strong in listOf(true, false)) {
            val first = renamed.copy(iap2 = strong)
            val second = android.copy(iap2 = strong, connected = false)
            assertEquals(IphoneSelection.Connected(first.copy(connected = true)),
                IphoneSelection.select(null, listOf(first.copy(connected = true), second, car)))
            assertEquals(IphoneSelection.Connected(second.copy(connected = true)),
                IphoneSelection.select(null, listOf(first, second.copy(connected = true), car)))
            val connected = listOf(first.copy(connected = true), second.copy(connected = true))
            val multiple = IphoneSelection.select(null, connected + car)
            assertEquals(IphoneSelection.MultipleConnected(connected), multiple)
            assertNull(multiple.address)
            val disconnected = IphoneSelection.select(null, listOf(first, second, car))
            assertEquals(IphoneSelection.MultipleCandidates(listOf(first, second)), disconnected)
            assertNull(disconnected.address)
            assertEquals(IphoneSelection.OnlyCandidate(first), IphoneSelection.select(null, listOf(first, car)))
        }
    }
    @Test fun strongEvidenceDoesNotNeedPhoneClass() {
        val device = renamed.copy(phoneClass = false)
        assertEquals(IphoneSelection.OnlyCandidate(device), IphoneSelection.select(null, listOf(device, android)))
    }
    @Test fun singleUnknownBondFallsBackAndOtherUnknownListsDoNot() {
        assertEquals(IphoneSelection.OnlyBonded(car), IphoneSelection.select(null, listOf(car)))
        assertEquals(car.address, IphoneSelection.select(null, listOf(car)).address)
        assertEquals(IphoneSelection.NoIphone, IphoneSelection.select(null, listOf(car, car.copy(address = android.address))))
        assertEquals(IphoneSelection.NoIphone, IphoneSelection.select(null, emptyList()))
        assertNull(IphoneSelection.NoIphone.address)
    }
    @Test fun chosenAddressesAreAvailableOnlyForResolvedResults() {
        assertEquals(renamed.address, IphoneSelection.Connected(renamed).address)
        assertEquals(renamed.address, IphoneSelection.OnlyCandidate(renamed).address)
    }
    @Test fun diagnosticsNeverExposeFullAddressesOrNames() {
        assertEquals("Connected · …:A9:30", IphoneSelection.Connected(renamed).diagnostic())
        val results = listOf(IphoneSelection.Selected(renamed), IphoneSelection.SelectedUnpaired(renamed.address),
            IphoneSelection.MultipleConnected(listOf(renamed, car)), IphoneSelection.MultipleCandidates(listOf(renamed, car)),
            IphoneSelection.OnlyCandidate(renamed), IphoneSelection.OnlyBonded(renamed), IphoneSelection.NoIphone)
        results.forEach { assertFalse(it.diagnostic().contains(renamed.address)); assertFalse(it.diagnostic().contains("Riko")) }
        assertEquals("—", maskedAddress("malformed"))
    }
}
