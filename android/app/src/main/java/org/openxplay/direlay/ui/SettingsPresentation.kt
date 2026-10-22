// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.settings.WirelessNetworkChoice

data class RadioOption(val title: Int, val supporting: Int)
val outputOptions = listOf(RadioOption(R.string.ui_browser, R.string.option_browser_hint),
    RadioOption(R.string.ui_carlink, R.string.option_carlink_hint))
val connectionOptions = listOf(RadioOption(R.string.direlay_wireless, R.string.option_wireless_hint),
    RadioOption(R.string.ui_usb, R.string.option_usb_hint))
fun networkOption(choice: WirelessNetworkChoice) = when (choice) {
    WirelessNetworkChoice.WIFI_DIRECT -> RadioOption(R.string.ui_wifi_direct_recommended, R.string.option_direct_hint)
    WirelessNetworkChoice.PHONE_HOTSPOT -> RadioOption(choice.label, R.string.option_hotspot_hint)
    WirelessNetworkChoice.EXISTING_WIFI -> RadioOption(choice.label, R.string.option_wifi_hint)
}
enum class SettingsNotice { SILENT, NEXT_START, RECONNECT }
fun settingsNotice(before: RelaySettings, after: RelaySettings, carlinkConnected: Boolean,
    physicalWidthBefore: Int = 0, physicalWidthAfter: Int = physicalWidthBefore): SettingsNotice = when {
    before == after && physicalWidthBefore == physicalWidthAfter -> SettingsNotice.SILENT
    before.vehicleInformation != after.vehicleInformation && carlinkConnected -> SettingsNotice.RECONNECT
    // These options are captured by the next CarPlay input session.
    before.iphoneAddress != after.iphoneAddress || before.connectionMode != after.connectionMode || before.wirelessNetwork != after.wirelessNetwork ||
        before.displayMode != after.displayMode || before.display != after.display || before.fps != after.fps || before.mfiTarget != after.mfiTarget ||
        before.i2cPath != after.i2cPath || before.remoteServer != after.remoteServer || before.remoteToken != after.remoteToken ||
        before.carBluetoothAudio != after.carBluetoothAudio || physicalWidthBefore != physicalWidthAfter -> SettingsNotice.NEXT_START
    before.carlinkPictureSize != after.carlinkPictureSize && carlinkConnected -> SettingsNotice.RECONNECT
    else -> SettingsNotice.SILENT
}

data class SettingsFeedback(val message: Int, val action: Int? = null)
fun settingsFeedback(before: RelaySettings, after: RelaySettings, carlinkConnected: Boolean, running: Boolean,
    physicalWidthBefore: Int = 0, physicalWidthAfter: Int = physicalWidthBefore): SettingsFeedback? =
    when (settingsNotice(before, after, carlinkConnected, physicalWidthBefore, physicalWidthAfter)) {
        SettingsNotice.SILENT -> null
        SettingsNotice.NEXT_START -> SettingsFeedback(R.string.settings_next_start, if (running) R.string.settings_restart else null)
        SettingsNotice.RECONNECT -> SettingsFeedback(if (before.vehicleInformation != after.vehicleInformation)
            R.string.vehicle_reconnect else R.string.settings_reconnect, R.string.vehicle_reconnect_action)
    }
