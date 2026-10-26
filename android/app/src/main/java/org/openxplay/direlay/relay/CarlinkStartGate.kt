// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
package org.openxplay.direlay.relay

import java.io.IOException

/** One explicit start owns a bounded wait; a saved screen never releases it. */
class CarlinkStartGate {
    @Volatile var untilMillis: Long? = null
        private set

    fun request(nowMillis: Long, enabled: Boolean) {
        untilMillis = if (enabled) nowMillis + MAX_WAIT_MILLIS else null
    }

    fun ready(nowMillis: Long, outputReady: Boolean, outputEnabled: Boolean): Boolean {
        val until = untilMillis ?: return true
        if (!outputEnabled) throw IOException("Carlink output was disabled while waiting to start CarPlay")
        if (outputReady) { cancel(); return true }
        if (nowMillis >= until) throw IOException("Carlink output did not become ready before CarPlay startup")
        return false
    }

    fun cancel() { untilMillis = null }
    companion object { const val MAX_WAIT_MILLIS = 60_000L }
}
