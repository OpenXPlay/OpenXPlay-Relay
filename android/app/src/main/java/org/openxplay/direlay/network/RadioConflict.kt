// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.network

import org.openxplay.direlay.settings.WirelessNetworkChoice

/** A connection that needs only one of Wi-Fi and the hotspot: the iPhone's network, the car browser, or USB. */
enum class RadioChoice { PHONE_HOTSPOT, SHARED_WIFI, BROWSER, USB }

enum class RadioConflictKind {
    /** Carlink or Miracast reach the car over this phone's Wi-Fi while the iPhone joins its hotspot. */
    CAR_WIFI_WITH_HOTSPOT,
    /** The iPhone network runs over Wi-Fi, and the hotspot (most likely for the car) switched Wi-Fi off. */
    HOTSPOT_STOPS_WIFI,
}

data class RadioConflict(val kind: RadioConflictKind, val choices: List<RadioChoice>) {
    fun line() = "kind=${kind.name.lowercase()} choices=${choices.joinToString(",") { it.name.lowercase() }}"
}

/**
 * A setup this phone cannot carry: it needs Wi-Fi and the hotspot at once on a phone that keeps only one.
 * [carWifi] is true when the car output itself uses this phone's Wi-Fi radio (Carlink, Miracast).
 */
fun radioConflict(concurrency: RadioConcurrency, wireless: Boolean, network: WirelessNetworkChoice,
    carWifi: Boolean, readings: NetworkReadings): RadioConflict? {
    if (concurrency != RadioConcurrency.EXCLUSIVE) return null
    if (wireless && network == WirelessNetworkChoice.PHONE_HOTSPOT && carWifi)
        // The car browser joins the hotspot as the iPhone does; USB keeps the car output.
        return RadioConflict(RadioConflictKind.CAR_WIFI_WITH_HOTSPOT, listOf(RadioChoice.BROWSER, RadioChoice.USB))
    if (wireless && network != WirelessNetworkChoice.PHONE_HOTSPOT && readings.hotspot == true && readings.wifi == false)
        return RadioConflict(RadioConflictKind.HOTSPOT_STOPS_WIFI, buildList {
            // The car output's own Wi-Fi rules out moving the iPhone to the hotspot.
            if (!carWifi) add(RadioChoice.PHONE_HOTSPOT)
            if (!carWifi && network != WirelessNetworkChoice.EXISTING_WIFI) add(RadioChoice.SHARED_WIFI)
            add(RadioChoice.USB)
        })
    return null
}
