// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.relay

import org.openxplay.direlay.settings.CarlinkConnectionPolicy

/**
 * Carlink waits while CarPlay's next start would use Wi-Fi Direct, or while a running CarPlay session
 * still holds the Wi-Fi Direct group it started with; preferences can change before that session
 * releases its group. The session part changes without a settings change, so the relay re-evaluates
 * the block whenever the session starts or stops.
 */
internal class CarlinkWifiDirectBlock {
    @Volatile private var sessionWifiDirect = false
    private var sessionRunning = false

    /** Records whether the CarPlay session being started reserves Wi-Fi Direct. */
    fun carPlayStarting(wifiDirect: Boolean) { sessionWifiDirect = wifiDirect }

    fun blocked(carlink: Boolean, startConflict: Boolean, running: Boolean): Boolean =
        startConflict || CarlinkConnectionPolicy.conflicts(carlink, running, sessionWifiDirect)

    /** Whether the CarPlay session started or stopped since the previous observation. */
    @Synchronized fun sessionChanged(running: Boolean): Boolean {
        if (running == sessionRunning) return false
        sessionRunning = running
        return true
    }
}
