// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.settings

/** Carlink joins the head unit's P2P network, which replaces CarPlay's local P2P group. */
object CarlinkConnectionPolicy {
    const val REASON = "Carlink can replace CarPlay's Wi-Fi Direct group, so use the local hotspot or USB."
    const val REASON_KEY = "constraint.carlinkWifiDirect"
    fun enabled(settings: RelaySettings) = settings.carlinkOutput || settings.bothOutputs
    fun wifiDirectAllowed(settings: RelaySettings) = !enabled(settings)
    fun conflicts(carlink: Boolean, wireless: Boolean, wifiDirect: Boolean) = carlink && wireless && wifiDirect
    fun conflicts(settings: RelaySettings, wireless: Boolean) =
        conflicts(enabled(settings), wireless, settings.wirelessNetwork.choice == WirelessNetworkChoice.WIFI_DIRECT)
    fun enabling(before: RelaySettings, after: RelaySettings, wireless: Boolean = after.connectionMode != ConnectionMode.USB): RelaySettings =
        if ((!enabled(before) || before.connectionMode != after.connectionMode) && enabled(after) && wireless && after.wirelessNetwork.choice == WirelessNetworkChoice.WIFI_DIRECT)
            after.copy(wirelessNetwork = after.wirelessNetwork.copy(choice = WirelessNetworkChoice.PHONE_HOTSPOT))
        else after
}
