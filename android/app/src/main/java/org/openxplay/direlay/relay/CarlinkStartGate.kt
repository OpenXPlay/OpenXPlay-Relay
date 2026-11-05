// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.relay

import java.io.IOException

/** The gate ran out of time while the Carlink link was still starting. */
class CarlinkStartTimeout : IOException("Carlink output did not become ready before CarPlay startup")

/** One explicit start owns a bounded wait; a saved screen never releases it. */
class CarlinkStartGate {
    @Volatile var untilMillis: Long? = null
        private set

    fun request(nowMillis: Long, enabled: Boolean) {
        untilMillis = if (enabled) nowMillis + MAX_WAIT_MILLIS else null
    }

    fun ready(nowMillis: Long, outputReady: Boolean, outputEnabled: Boolean, waitingForWifi: Boolean = false): Boolean {
        val until = untilMillis ?: return true
        if (!outputEnabled) throw IOException("Carlink output was disabled while waiting to start CarPlay")
        if (outputReady) { cancel(); return true }
        if (waitingForWifi) { untilMillis = nowMillis + MAX_WAIT_MILLIS; return false }
        if (nowMillis >= until) throw CarlinkStartTimeout()
        return false
    }

    fun cancel() { untilMillis = null }
    companion object { const val MAX_WAIT_MILLIS = 60_000L }
}
