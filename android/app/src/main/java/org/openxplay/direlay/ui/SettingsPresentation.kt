// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.settings.TeslaBrowserMode
import org.openxplay.direlay.settings.WirelessNetworkChoice

data class RadioOption(val title: Int, val supporting: Int)
val teslaBrowserOptions = listOf(
    TeslaBrowserMode.TESLAMIRROR to RadioOption(R.string.tesla_mirror, R.string.tesla_virtual_name_hint),
    TeslaBrowserMode.VIRTUAL_NAME to RadioOption(R.string.tesla_virtual_name, R.string.tesla_virtual_name_hint),
    TeslaBrowserMode.PUBLIC_ADDRESS to RadioOption(R.string.tesla_public_address, R.string.tesla_public_address_hint),
    TeslaBrowserMode.TESLA_ANDROID to RadioOption(R.string.tesla_android, R.string.tesla_public_address_hint),
    TeslaBrowserMode.OFF to RadioOption(R.string.tesla_off, R.string.tesla_intro))
fun teslaBrowserOption(mode: TeslaBrowserMode) = teslaBrowserOptions.first { it.first == mode }.second

val outputOptions = listOf(RadioOption(R.string.ui_browser, R.string.option_browser_hint),
    RadioOption(R.string.ui_carlink, R.string.option_carlink_hint),
    RadioOption(R.string.wireless_display, R.string.option_wireless_display_hint))
val connectionOptions = listOf(RadioOption(R.string.direlay_wireless, R.string.option_wireless_hint),
    RadioOption(R.string.ui_usb, R.string.option_usb_hint))
fun networkOption(choice: WirelessNetworkChoice) = when (choice) {
    WirelessNetworkChoice.WIFI_DIRECT -> RadioOption(R.string.ui_wifi_direct_recommended, R.string.option_direct_hint)
    WirelessNetworkChoice.PHONE_HOTSPOT -> RadioOption(choice.label, R.string.option_hotspot_hint)
    WirelessNetworkChoice.EXISTING_WIFI -> RadioOption(choice.label, R.string.option_wifi_hint)
}
enum class SettingsNotice { SILENT, NEXT_START, RECONNECT }
fun settingsNotice(before: RelaySettings, after: RelaySettings, carlinkConnected: Boolean,
    physicalWidthBefore: Int = 0, physicalWidthAfter: Int = physicalWidthBefore,
    carlinkPictureTrial: org.openxplay.direlay.settings.CarlinkPictureTrial? = null): SettingsNotice = when {
    before == after && physicalWidthBefore == physicalWidthAfter -> SettingsNotice.SILENT
    before.vehicleInformation != after.vehicleInformation && carlinkConnected -> SettingsNotice.RECONNECT
    // These options are captured by the next CarPlay input session.
    before.iphoneAddress != after.iphoneAddress || before.connectionMode != after.connectionMode || !before.wirelessNetwork.sameConnectionAs(after.wirelessNetwork) ||
        before.displayMode != after.displayMode || before.display != after.display || before.mfiTarget != after.mfiTarget ||
        before.i2cPath != after.i2cPath || before.remoteServer != after.remoteServer || before.remoteToken != after.remoteToken ||
        before.carBluetoothAudio != after.carBluetoothAudio || physicalWidthBefore != physicalWidthAfter -> SettingsNotice.NEXT_START
    before.carlinkPictureDefault != after.carlinkPictureDefault &&
        (carlinkPictureTrial == null || carlinkPictureTrial.size != after.carlinkPictureDefault) -> SettingsNotice.NEXT_START
    before.carlinkPictureSize != after.carlinkPictureSize && carlinkConnected -> SettingsNotice.RECONNECT
    else -> SettingsNotice.SILENT
}

data class SettingsFeedback(val message: Int, val action: Int? = null)
fun settingsFeedback(before: RelaySettings, after: RelaySettings, carlinkConnected: Boolean, running: Boolean,
    physicalWidthBefore: Int = 0, physicalWidthAfter: Int = physicalWidthBefore,
    carlinkPictureTrial: org.openxplay.direlay.settings.CarlinkPictureTrial? = null): SettingsFeedback? =
    when (settingsNotice(before, after, carlinkConnected, physicalWidthBefore, physicalWidthAfter, carlinkPictureTrial)) {
        SettingsNotice.SILENT -> null
        SettingsNotice.NEXT_START -> SettingsFeedback(R.string.settings_next_start, if (running) R.string.settings_restart else null)
        SettingsNotice.RECONNECT -> SettingsFeedback(if (before.vehicleInformation != after.vehicleInformation)
            R.string.vehicle_reconnect else R.string.settings_reconnect, R.string.vehicle_reconnect_action)
    }

/** Resolve catalog IDs through localized display names; custom selections show their supplied text. */
fun carSelectionSummary(car: org.openxplay.direlay.settings.CarSelection, models: org.openxplay.direlay.relay.CarModels?,
    locale: java.util.Locale, none: String, other: String): String {
    val brand = if (car.brand == "other") car.brandText.ifBlank { other } else models?.brand(car)?.name(locale)
    val model = if (car.model == "other") car.modelText.ifBlank { other } else models?.model(car)?.name(locale)
    val year = if (car.year == "other") car.yearText.ifBlank { other } else models?.year(car)?.name(locale)
    return listOfNotNull(brand, model, year).filter { it.isNotBlank() }.joinToString(" · ").ifEmpty { none }
}

fun frameRateSummary(setting: org.openxplay.stream.core.FrameRatePreference, fps: Int, automatic: String): String =
    if (setting == org.openxplay.stream.core.FrameRatePreference.AUTO) "$automatic · $fps fps" else "$fps fps"

fun pictureMenuSummary(resolution: String, widthMm: Int, codec: String, fps: Int): String =
    "$resolution · $widthMm mm · $codec · $fps fps"
