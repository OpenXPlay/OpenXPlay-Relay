// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.relay

import org.openxplay.direlay.network.NetworkReadings
import org.openxplay.direlay.path.LinkFailure

/** Observe real off-to-on edges, once per user start, including while the host covers Home. */
class RecoverOncePolicy {
    private var previous = NetworkReadings()
    private var recovered = false
    fun reset() { recovered = false }
    fun observe(failure: LinkFailure?, readings: NetworkReadings, userStopped: Boolean): String? {
        val before = previous
        previous = readings
        if (recovered || userStopped || failure == null || failure.retrying) return null
        val restored = when (failure.code) {
            "hotspot_not_ready", "hotspot_configuration" -> before.hotspot == false && readings.hotspot == true
            "bluetooth_off", "bluetooth_unusable" -> before.bluetooth == false && readings.bluetooth == true
            "wifi_off" -> before.wifi == false && readings.wifi == true
            else -> false
        }
        if (!restored) return null
        recovered = true
        return failure.code
    }
}
