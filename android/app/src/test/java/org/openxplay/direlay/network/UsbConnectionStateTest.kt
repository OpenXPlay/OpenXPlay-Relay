// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.network

import org.junit.Assert.*
import org.junit.Test

class UsbConnectionStateTest {
    @Test fun broadcastExtrasDistinguishDeviceHostAndNoConnection() {
        assertEquals(UsbConnectionState.DEVICE, usbConnectionState(true, false))
        assertEquals(UsbConnectionState.HOST, usbConnectionState(false, true))
        assertEquals(UsbConnectionState.HOST, usbConnectionState(true, true))
        assertEquals(UsbConnectionState.HOST, usbConnectionState(null, true))
        assertEquals(UsbConnectionState.DISCONNECTED, usbConnectionState(false, false))
        for ((connected, host) in listOf(null to null, true to null, null to false, false to null))
            assertEquals(UsbConnectionState.UNKNOWN, usbConnectionState(connected, host))
    }

    @Test fun roleHintRequiresALiveWiredWaitAndAReadableEmptyList() {
        for (wireless in listOf(false, true)) for (waiting in listOf(false, true))
            for (usb in UsbConnectionState.entries) for (empty in listOf(true, false, null)) {
                assertEquals(!wireless && waiting && usb == UsbConnectionState.DEVICE && empty == true,
                    wiredUsbRoleProblem(wireless, waiting, NetworkReadings(usb = usb, usbDevicesEmpty = empty)))
            }
    }
}
