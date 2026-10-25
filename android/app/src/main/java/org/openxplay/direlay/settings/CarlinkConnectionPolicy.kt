// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.settings

/** Carlink and system casting share the Wi-Fi Direct radio with CarPlay's local P2P group. */
object CarlinkConnectionPolicy {
    const val REASON = "Carlink can replace CarPlay's Wi-Fi Direct group, so use the local hotspot or USB."
    const val REASON_KEY = "constraint.carlinkWifiDirect"
    fun enabled(settings: RelaySettings) = settings.carlinkOutput
    const val MIRACAST_REASON = "Miracast can replace CarPlay's Wi-Fi Direct group, so use the local hotspot or USB."
    const val MIRACAST_REASON_KEY = "constraint.miracastWifiDirect"
    fun usesWifiDirect(settings: RelaySettings, miracast: Boolean = settings.miracastOutput) = enabled(settings) || miracast
    fun wifiDirectAllowed(settings: RelaySettings, miracast: Boolean = settings.miracastOutput) = !usesWifiDirect(settings, miracast)
    fun conflicts(carlink: Boolean, wireless: Boolean, wifiDirect: Boolean, miracast: Boolean = false) =
        (carlink || miracast) && wireless && wifiDirect
    fun conflicts(settings: RelaySettings, wireless: Boolean, miracast: Boolean = settings.miracastOutput) =
        conflicts(enabled(settings), wireless, settings.wirelessNetwork.choice == WirelessNetworkChoice.WIFI_DIRECT, miracast)
    fun enabling(before: RelaySettings, after: RelaySettings, wireless: Boolean = after.connectionMode != ConnectionMode.USB,
        miracast: Boolean = after.miracastOutput): RelaySettings =
        if (((!enabled(before) && enabled(after)) || (!before.miracastOutput && miracast) || before.connectionMode != after.connectionMode) && usesWifiDirect(after, miracast) && wireless && after.wirelessNetwork.choice == WirelessNetworkChoice.WIFI_DIRECT)
            after.copy(wirelessNetwork = after.wirelessNetwork.copy(choice = WirelessNetworkChoice.PHONE_HOTSPOT))
        else after
}
