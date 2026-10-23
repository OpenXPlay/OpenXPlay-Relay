// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.relay.CarPlayStage
import org.openxplay.stream.core.SessionPhase

data class ConnectionStatusPresentation(val headline: Int, val subline: Int,
    val sublineArguments: List<String> = emptyList(), val tone: StatusTone = StatusTone.PROGRESS)

/** Active sessions keep each page's running presentation; stream progress wins over the stage. */
fun presentConnectionStatus(phase: SessionPhase?, stage: CarPlayStage, wireless: Boolean,
    deviceName: String): ConnectionStatusPresentation? {
    val (headline, subline) = when (phase) {
        SessionPhase.ACTIVE -> return null
        SessionPhase.CONNECTING -> R.string.ui_connecting to R.string.ui_picture_waiting_hint
        SessionPhase.RECONNECTING -> R.string.ui_reconnecting to R.string.ui_reconnecting_hint
        null, SessionPhase.ENDED -> when (stage) {
            CarPlayStage.INTERRUPTED -> R.string.ui_reconnecting to R.string.ui_reconnecting_hint
            CarPlayStage.SESSION -> R.string.ui_connecting to R.string.ui_picture_waiting_hint
            CarPlayStage.USB_CONNECTED -> R.string.ui_connecting to R.string.ui_usb_connected_hint
            CarPlayStage.BLUETOOTH_CONNECTED -> R.string.ui_connecting to R.string.ui_bluetooth_connected_hint
            CarPlayStage.BLUETOOTH -> R.string.ui_connecting to R.string.ui_bluetooth_connecting_hint
            CarPlayStage.WAITING, CarPlayStage.NONE -> R.string.ui_waiting to
                if (wireless) R.string.ui_waiting_wireless_hint else R.string.ui_usb_hint
        }
    }
    return ConnectionStatusPresentation(headline, subline,
        if (subline == R.string.ui_waiting_wireless_hint) listOf(deviceName) else emptyList())
}
