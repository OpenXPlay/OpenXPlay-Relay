// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.relay

/** Confined to setup; suppressed callbacks do not postpone the next statistics line. */
internal class WirelessStatisticsLogLimiter {
    private var last: Long? = null
    fun allow(now: Long): Boolean {
        val previous = last
        if (previous != null && now - previous < 10_000_000_000L) return false
        last = now
        return true
    }
}

private val wirelessLogUrl = Regex("""(?i)\b[a-z][a-z0-9+.-]*://[^\s]+""")
internal fun wirelessLogLine(message: String): String = wirelessLogUrl.replace(message, "[redacted-url]")
    .lineSequence().joinToString(" ")
