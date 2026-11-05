// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-05
package org.openxplay.direlay.settings

import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.relay.OutputChange
import org.openxplay.direlay.relay.RetiringOutputPolicy
import org.openxplay.stream.core.FrameRatePreference

class CarlifeConnectionPolicyTest {
    private val base = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        FrameRatePreference.AUTO, false, true, false, null, outputs = setOf(RelayOutput.CARLIFE))

    @Test fun explicitCarTransportsOpenOnlyTheirOwnLinkAndLegacyOpensBoth() {
        for ((transport, usb, tcp) in listOf(Triple(CarlifeTransport.AUTO, true, true),
            Triple(CarlifeTransport.USB, true, false), Triple(CarlifeTransport.WIFI, false, true))) {
            val chosen = CarlifeConnectionPolicy.choose(base, transport)
            val plan = requireNotNull(CarlifeConnectionPolicy.output(chosen))
            assertEquals(usb, plan.transport.usb)
            assertEquals(tcp, plan.transport.tcp)
            assertEquals(if (usb && !tcp) ConnectionMode.WIRELESS else ConnectionMode.USB, chosen.connectionMode)
            chosen.validate(wireless = false)
        }
    }

    @Test fun changingTransportRetiresTheOldOutputEvenWithoutAnIpChange() {
        val lifecycle = RetiringOutputPolicy<CarlifeOutputPlan>()
        val wifi = CarlifeConnectionPolicy.output(base.copy(carlifeTransport = CarlifeTransport.WIFI))
        val usb = CarlifeConnectionPolicy.output(CarlifeConnectionPolicy.choose(base, CarlifeTransport.USB))
        assertEquals(OutputChange.ADD, lifecycle.update(wifi))
        assertEquals(OutputChange.REMOVE, lifecycle.update(usb))
        assertEquals(OutputChange.KEEP, lifecycle.update(usb))
        lifecycle.ended()
        assertEquals(OutputChange.ADD, lifecycle.update(usb))
        assertEquals(OutputChange.KEEP, lifecycle.update(usb))
        assertEquals(OutputChange.REMOVE, lifecycle.update(null))
    }

    @Test fun usbNeverUsesDirectedDiscoveryAndBothUsbRolesAreRejected() {
        val usb = CarlifeConnectionPolicy.choose(base.copy(carlifeTargetAddress = "192.168.5.220"), CarlifeTransport.USB)
        assertEquals("", CarlifeConnectionPolicy.output(usb)?.targetAddress)
        for (mode in listOf(ConnectionMode.USB, ConnectionMode.LAST_USED)) {
            assertTrue(runCatching { usb.copy(connectionMode = mode).validate(wireless = false) }.isFailure)
        }
        assertEquals("192.168.5.220", CarlifeConnectionPolicy.output(usb.copy(carlifeTransport = CarlifeTransport.WIFI))?.targetAddress)
    }

    @Test fun addingUsbCarlifeAlsoKeepsOtherOutputsRadioConstraints() {
        for (other in listOf(RelayOutput.CARLINK, RelayOutput.MIRACAST)) {
            val wiredInput = base.copy(outputs = setOf(other), carlifeTransport = CarlifeTransport.USB,
                wirelessNetwork = WirelessNetworkSettings(choice = WirelessNetworkChoice.WIFI_DIRECT))
            val chosen = requireNotNull(OutputSelection.toggle(wiredInput, RelayOutput.CARLIFE, true, false))
            assertEquals(ConnectionMode.WIRELESS, chosen.connectionMode)
            assertEquals(WirelessNetworkChoice.PHONE_HOTSPOT, chosen.wirelessNetwork.choice)
            assertFalse(CarlinkConnectionPolicy.conflicts(chosen, true))
        }
    }

    @Test fun otherOutputsDoNotConstrainTheIphoneAndReselectingUsbCarlifeRestoresWirelessInput() {
        val browser = base.copy(outputs = setOf(RelayOutput.BROWSER), carlifeTransport = CarlifeTransport.USB)
        assertNull(CarlifeConnectionPolicy.output(browser))
        assertTrue(CarlifeConnectionPolicy.iphoneUsbAllowed(browser))
        assertEquals(browser, CarlifeConnectionPolicy.normalize(browser))
        assertEquals(ConnectionMode.WIRELESS, OutputSelection.choose(browser, OutputChoice.CARLIFE, WirelessDisplayKind.DLNA, false).connectionMode)
        assertEquals(ConnectionMode.WIRELESS, OutputSelection.toggle(browser, RelayOutput.CARLIFE, true, false)?.connectionMode)
    }
}
