// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-05
package org.openxplay.direlay.ui

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.FrameRatePreference

class CarlifePresentationTest {
    private val base = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        FrameRatePreference.AUTO, false, true, false, null, outputs = setOf(RelayOutput.CARLIFE))

    @Test fun guideRequiresAnExplicitCarLinkEvenWhenOpenedAtALaterStep() {
        assertTrue(setupCarlifeChoiceRequired(base))
        for (step in SetupStep.entries) assertEquals(if (step.ordinal > SetupStep.OUTPUT.ordinal) SetupStep.OUTPUT else step, setupEntryStep(base, step))
        for (transport in listOf(CarlifeTransport.USB, CarlifeTransport.WIFI)) {
            val chosen = CarlifeConnectionPolicy.choose(base, transport)
            assertFalse(setupCarlifeChoiceRequired(chosen))
            for (step in SetupStep.entries) assertEquals(step, setupEntryStep(chosen, step))
        }
        assertFalse(setupCarlifeChoiceRequired(base.copy(outputs = setOf(RelayOutput.BROWSER))))
    }

    @Test fun everyUsbNetworkOptionDescribesOnlyTheIphoneWifiAndKeepsTheCarOnUsb() {
        val usb = CarlifeConnectionPolicy.choose(base, CarlifeTransport.USB)
        val copy = listOf(R.string.carlife_usb_direct_hint, R.string.carlife_usb_hotspot_hint, R.string.carlife_usb_shared_hint)
        WirelessNetworkChoice.entries.forEachIndexed { index, choice ->
            val draft = usb.copy(wirelessNetwork = usb.wirelessNetwork.copy(choice = choice))
            assertEquals(copy[index], carlifeNetworkOption(draft, choice).supporting)
            assertEquals(R.string.carlife_usb_link, carlifeRoute(draft, true).car)
            assertEquals(R.string.carlife_usb_summary, modeRows(draft, true).first().value)
            assertEquals(listOf(R.string.carlife_usb_summary), outputSummaryLabels(draft))
        }
        assertEquals(R.string.carlife_shared_wifi, setupNetworkLabel(usb, WirelessNetworkChoice.EXISTING_WIFI))
        assertEquals(ConnectionMode.WIRELESS, chooseConnection(usb, false).connectionMode)
    }

    @Test fun wirelessCarlifeSupportsEitherIphoneInputAndOtherOutputDescriptionsAreUnchanged() {
        val wifi = CarlifeConnectionPolicy.choose(base, CarlifeTransport.WIFI)
        for (choice in WirelessNetworkChoice.entries) {
            val draft = wifi.copy(wirelessNetwork = wifi.wirelessNetwork.copy(choice = choice))
            assertEquals(networkOption(choice), carlifeNetworkOption(draft, choice))
            for (wireless in listOf(false, true)) {
                val chosen = chooseConnection(draft, wireless)
                assertEquals(wireless, chosen.connectionMode == ConnectionMode.WIRELESS)
                assertEquals(R.string.network_wifi, carlifeRoute(chosen, wireless).car)
                if (!wireless) assertEquals(R.string.carlife_usb_link, carlifeRoute(chosen, false).iphone)
            }
        }
        val browser = base.copy(outputs = setOf(RelayOutput.BROWSER), carlifeTransport = CarlifeTransport.USB)
        for (choice in WirelessNetworkChoice.entries) assertEquals(networkOption(choice), carlifeNetworkOption(browser, choice))
        assertEquals(ConnectionMode.USB, chooseConnection(browser, false).connectionMode)
    }
}
