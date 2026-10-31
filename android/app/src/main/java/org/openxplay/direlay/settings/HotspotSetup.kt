// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-31
package org.openxplay.direlay.settings

import java.security.SecureRandom

/** Two ways to enter the same hotspot credentials, without replacing them when switching. */
enum class HotspotSetupMethod {
    COPY_TO_SYSTEM, EXISTING;

    companion object {
        fun forCredentials(ssid: String, passphrase: String) =
            if (ssid.isEmpty() && passphrase.isEmpty()) COPY_TO_SYSTEM else EXISTING
    }
}

data class SuggestedHotspot(val ssid: String, val passphrase: String) {
    override fun toString() = "SuggestedHotspot(ssid=$ssid, passphrase=[redacted])"
}

/** Generated once for an unconfigured installation; unrelated to any device identifier. */
fun suggestedHotspot(random: SecureRandom = SecureRandom()): SuggestedHotspot {
    val alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"
    fun token(length: Int) = buildString { repeat(length) { append(alphabet[random.nextInt(alphabet.length)]) } }
    return SuggestedHotspot("OpenXPlay-A-${token(5)}", token(16))
}
