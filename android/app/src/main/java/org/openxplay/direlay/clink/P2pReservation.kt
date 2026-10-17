// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.clink

/** The composition root decides whether another input owns the device's P2P group. */
fun interface P2pReservation {
    fun reservedByCarPlay(): Boolean
}

/** Read effective DiPlay preferences at join time, including changes outside the relay settings UI. */
class AirPlayP2pReservation(private val context: android.content.Context) : P2pReservation {
    override fun reservedByCarPlay() =
        com.shilapi.xcertplay.AirPlayPersistence.loadWirelessEnabled(context) &&
            com.shilapi.xcertplay.AirPlayPersistence.loadWirelessHotspotMode(context) ==
            com.shilapi.xcertplay.orchestration.WirelessHotspotMode.WIFI_P2P
}
