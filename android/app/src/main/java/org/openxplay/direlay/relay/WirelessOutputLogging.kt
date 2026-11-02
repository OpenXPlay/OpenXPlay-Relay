// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.relay

import org.openxplay.direlay.settings.*
import org.openxplay.stream.core.DisplayInfo

internal fun wirelessCanvasLogLine(screen: CarPlayScreen, declared: DisplayInfo?, settings: RelaySettings): String =
    "canvas ${screen.profile.width}x${screen.profile.height} source=${if (screen.source == "carlink picture") "fixed" else screen.source} " +
        "declared=${declared?.let { "${it.widthPx}x${it.heightPx}" } ?: "-"} " +
        "resolution=${settings.carplayResolution.wire} picture=${if (settings.wirelessPicture == WirelessPictureMode.CUSTOM) "custom" else "auto"}"

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
