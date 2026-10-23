// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.relay.CarPlayStage
import org.openxplay.stream.core.SessionPhase

internal data class ConnectionStatusCase(val phase: SessionPhase?, val stage: CarPlayStage, val wireless: Boolean,
    val headline: Int, val subline: Int, val arguments: List<String> = emptyList())

/** Expected copy from the task's table, exercised on both pages. */
internal fun connectionStatusCases(deviceName: String): List<ConnectionStatusCase> = buildList {
    val stages = listOf(
        Triple(CarPlayStage.INTERRUPTED, R.string.ui_reconnecting, R.string.ui_reconnecting_hint),
        Triple(CarPlayStage.SESSION, R.string.ui_connecting, R.string.ui_picture_waiting_hint),
        Triple(CarPlayStage.USB_CONNECTED, R.string.ui_connecting, R.string.ui_usb_connected_hint),
        Triple(CarPlayStage.BLUETOOTH_CONNECTED, R.string.ui_connecting, R.string.ui_bluetooth_connected_hint),
        Triple(CarPlayStage.BLUETOOTH, R.string.ui_connecting, R.string.ui_bluetooth_connecting_hint),
    )
    for (phase in listOf(null, SessionPhase.ENDED)) for (wireless in listOf(false, true)) {
        stages.forEach { (stage, headline, subline) ->
            add(ConnectionStatusCase(phase, stage, wireless, headline, subline))
        }
        for (stage in listOf(CarPlayStage.NONE, CarPlayStage.WAITING)) {
            add(ConnectionStatusCase(phase, stage, wireless, R.string.ui_waiting,
                if (wireless) R.string.ui_waiting_wireless_hint else R.string.ui_usb_hint,
                if (wireless) listOf(deviceName) else emptyList()))
        }
    }
    for (stage in CarPlayStage.entries) for (wireless in listOf(false, true)) {
        add(ConnectionStatusCase(SessionPhase.CONNECTING, stage, wireless,
            R.string.ui_connecting, R.string.ui_picture_waiting_hint))
        add(ConnectionStatusCase(SessionPhase.RECONNECTING, stage, wireless,
            R.string.ui_reconnecting, R.string.ui_reconnecting_hint))
    }
}
