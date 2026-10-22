// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*

/** Validate before publishing any preferences. Invalid editing state stays in the UI. */
fun settingsErrors(value: RelaySettings, wireless: Boolean, allowEmptySharedWifi: Boolean = false): Map<String, Int> = buildMap {
    fun credentials(ssid: String, password: String, hotspot: Boolean = false, allowEmptySsid: Boolean = false) {
        if (!(allowEmptySsid && ssid.isEmpty()) && (ssid.isBlank() || ssid.toByteArray(Charsets.UTF_8).size !in 1..32 || '\u0000' in ssid))
            put("ssid", if (hotspot) R.string.settings_hotspot_name_error else R.string.settings_ssid_error)
        if ((password.isNotEmpty() && password.length !in 8..63) || '\u0000' in password)
            put("password", if (hotspot) R.string.settings_hotspot_password_error else R.string.settings_password_error)
    }
    if (wireless) when (value.wirelessNetwork.choice) {
        WirelessNetworkChoice.PHONE_HOTSPOT -> credentials(value.wirelessNetwork.manualSsid, value.wirelessNetwork.manualPassphrase,
            hotspot = true, allowEmptySsid = allowEmptySharedWifi && CarlinkConnectionPolicy.enabled(value))
        WirelessNetworkChoice.EXISTING_WIFI -> if (!allowEmptySharedWifi && value.wirelessNetwork.sharedWifiNetworks.isEmpty()) put("ssid", R.string.shared_wifi_empty)
        else -> Unit
    }
    if (value.carlinkDeviceName.isBlank() || value.carlinkDeviceName.toByteArray(Charsets.UTF_8).size > 128 ||
        value.carlinkDeviceName.any { Character.isISOControl(it) }) put("name", R.string.settings_name_error)
    value.teslaAddress()?.let { if (!TeslaAddressPolicy.valid(it)) put("teslaAddress", R.string.tesla_address_invalid) }
    if (isEmpty() && runCatching { value.validate(wireless, allowEmptySharedWifi) }.isFailure) put("advanced", R.string.settings_advanced_error)
}
// Incomplete network setup stays durable after a policy switch; Start still requires valid credentials.
fun settingsSaveErrors(value: RelaySettings, wireless: Boolean) =
    settingsErrors(value, wireless, allowEmptySharedWifi = true)

class SettingsAutoSave(initial: RelaySettings, private val publish: (RelaySettings) -> Unit) {
    var saved = initial
        private set
    fun apply(value: RelaySettings, wireless: Boolean): Boolean {
        if (settingsSaveErrors(value, wireless).isNotEmpty()) return false
        if (value == saved) return false
        publish(value)
        saved = value
        return true
    }
}

/** Validate groups against the last valid draft so one invalid page cannot hide another page's error. */
fun settingsInvalidPages(value: RelaySettings, saved: RelaySettings, wireless: (RelaySettings) -> Boolean): Set<String> = buildSet {
    fun check(page: String, candidate: RelaySettings) {
        if (settingsSaveErrors(candidate, wireless(candidate)).isNotEmpty()) add(page)
    }
    check("settings/output", saved.copy(carlinkDeviceName = value.carlinkDeviceName))
    check("settings/car", saved.copy(teslaBrowser = value.teslaBrowser,
        teslaVirtualAddress = value.teslaVirtualAddress, teslaPublicAddress = value.teslaPublicAddress,
        teslaMirrorAddress = value.teslaMirrorAddress, teslaAndroidAddress = value.teslaAndroidAddress))
    val network = saved.copy(connectionMode = value.connectionMode, wirelessNetwork = value.wirelessNetwork)
    if (settingsErrors(network, wireless(network)).isNotEmpty()) add("settings/connection/network")
    check("developer/video", saved.copy(displayMode = value.displayMode, display = value.display, fps = value.fps))
    check("developer/connection", saved.copy(webPort = value.webPort, carlinkDeviceModel = value.carlinkDeviceModel,
        connectionMode = value.connectionMode, wirelessNetwork = saved.wirelessNetwork.copy(
            channel = value.wirelessNetwork.channel, band = value.wirelessNetwork.band, security = value.wirelessNetwork.security)))
    check("settings/identity", saved.copy(mfiTarget = value.mfiTarget, i2cPath = value.i2cPath,
        remoteServer = value.remoteServer, remoteToken = value.remoteToken))
}
