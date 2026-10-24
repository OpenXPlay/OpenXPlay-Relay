// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui.design

import org.openxplay.direlay.R
import org.openxplay.direlay.path.StepId

fun stepIcon(id: StepId): Int = when (id) {
    StepId.MODE -> R.drawable.symbol_settings_2
    StepId.IDENTITY -> R.drawable.symbol_file_key_2
    StepId.PERMISSIONS -> R.drawable.symbol_shield_check
    StepId.KEEP_ALIVE -> R.drawable.symbol_battery_charging
    StepId.BLUETOOTH, StepId.IPHONE_BLUETOOTH -> R.drawable.symbol_bluetooth
    StepId.WIFI, StepId.SHARED_WIFI, StepId.IPHONE_WIFI -> R.drawable.symbol_wifi
    StepId.HOTSPOT -> R.drawable.symbol_radio
    StepId.CAR_NETWORK -> R.drawable.symbol_route
    StepId.WEB_SERVER, StepId.TESLA -> R.drawable.symbol_globe
    StepId.CAR_PAGE -> R.drawable.symbol_monitor
    StepId.CARLINK_FIND -> R.drawable.symbol_scan_search
    StepId.CARLINK_LINK -> R.drawable.symbol_cable
    StepId.CARLINK_PIN -> R.drawable.symbol_key_round
    StepId.CARLINK_READY -> R.drawable.symbol_car_front
    StepId.DLNA, StepId.MIRACAST -> R.drawable.symbol_cast
    StepId.IPHONE_PAIRED, StepId.USB_IPHONE -> R.drawable.symbol_smartphone
    StepId.USB_HOST -> R.drawable.symbol_usb
    StepId.CARPLAY_START -> R.drawable.symbol_power
    StepId.IPHONE_AUTH, StepId.USB_TRUST -> R.drawable.symbol_lock
    StepId.USB_CONTROL -> R.drawable.symbol_cable
    StepId.SESSION -> R.drawable.symbol_activity
    StepId.PICTURE -> R.drawable.symbol_tv
}

fun stepHue(id: StepId): IconHue = when (id) {
    StepId.MODE -> IconHue.DEVELOPER
    StepId.IDENTITY, StepId.IPHONE_AUTH, StepId.USB_TRUST -> IconHue.IDENTITY
    StepId.PERMISSIONS, StepId.KEEP_ALIVE -> IconHue.PERMISSIONS
    StepId.BLUETOOTH, StepId.IPHONE_BLUETOOTH -> IconHue.BLUETOOTH
    StepId.WIFI, StepId.HOTSPOT, StepId.SHARED_WIFI, StepId.CAR_NETWORK, StepId.IPHONE_WIFI -> IconHue.NETWORK
    StepId.WEB_SERVER, StepId.CAR_PAGE, StepId.CARLINK_FIND, StepId.CARLINK_LINK, StepId.CARLINK_PIN,
    StepId.CARLINK_READY, StepId.DLNA, StepId.MIRACAST -> IconHue.CAR
    StepId.IPHONE_PAIRED, StepId.USB_HOST, StepId.CARPLAY_START, StepId.USB_IPHONE, StepId.USB_CONTROL -> IconHue.IPHONE
    StepId.SESSION, StepId.TESLA, StepId.PICTURE -> IconHue.PICTURE
}
