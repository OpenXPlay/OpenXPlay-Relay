// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-05
package org.openxplay.direlay.settings

/** The Android-to-car transport, independent of the iPhone's CarPlay input. */
enum class CarlifeTransport(val wire: String, val usb: Boolean, val tcp: Boolean) {
    AUTO("auto", true, true), USB("usb", true, false), WIFI("wifi", false, true);
    companion object { fun fromStored(value: String?) = entries.firstOrNull { it.wire == value } ?: AUTO }
}

data class CarlifeOutputPlan(val transport: CarlifeTransport, val targetAddress: String)

object CarlifeConnectionPolicy {
    fun usbOutput(settings: RelaySettings) = settings.carlifeOutput && settings.carlifeTransport == CarlifeTransport.USB
    fun iphoneUsbAllowed(settings: RelaySettings) = !usbOutput(settings)
    // A phone cannot be a USB accessory to the car and a USB host for the iPhone at the same time.
    fun normalize(settings: RelaySettings) = if (usbOutput(settings))
        CarlinkConnectionPolicy.enabling(settings, settings.copy(connectionMode = ConnectionMode.WIRELESS), wireless = true)
    else settings
    fun choose(settings: RelaySettings, transport: CarlifeTransport) = normalize(settings.copy(carlifeTransport = transport))
    fun output(settings: RelaySettings): CarlifeOutputPlan? = if (!settings.carlifeOutput) null else
        CarlifeOutputPlan(settings.carlifeTransport, settings.carlifeTargetAddress.takeIf { settings.carlifeTransport.tcp }.orEmpty())
}
