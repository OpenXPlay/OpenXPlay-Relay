// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.settings

enum class TeslaBrowserMode(val usesTrustedName: Boolean = false) {
    OFF, VIRTUAL_NAME(true), PUBLIC_ADDRESS, TESLAMIRROR(true), TESLA_ANDROID
}

enum class TeslaBrowserActivation { OFF, NEEDS_BROWSER, NEEDS_WIRELESS, NEEDS_PHONE_NETWORK, NEEDS_NAME, READY }

/** A shared rule for the editor, VPN consent and session startup. */
fun teslaBrowserActivation(mode: TeslaBrowserMode, wireless: Boolean, network: WirelessNetworkChoice,
    trustedNameConfigured: Boolean, browserOutput: Boolean): TeslaBrowserActivation = when {
    mode == TeslaBrowserMode.OFF -> TeslaBrowserActivation.OFF
    !browserOutput -> TeslaBrowserActivation.NEEDS_BROWSER
    !wireless -> TeslaBrowserActivation.NEEDS_WIRELESS
    network == WirelessNetworkChoice.EXISTING_WIFI -> TeslaBrowserActivation.NEEDS_PHONE_NETWORK
    mode.usesTrustedName && !trustedNameConfigured -> TeslaBrowserActivation.NEEDS_NAME
    else -> TeslaBrowserActivation.READY
}

object TeslaAddressPolicy {
    const val DEFAULT_VIRTUAL = "169.254.88.1"
    const val DEFAULT_PUBLIC = "169.254.88.1"
    const val DEFAULT_MIRROR = "100.88.0.1"
    const val DEFAULT_ANDROID = "3.3.3.1"

    /** Numeric dotted quads only: never resolve a hostname or accept ambiguous shortened IPv4 forms. */
    fun valid(address: String, phoneAddresses: Set<String> = emptySet()): Boolean {
        val parts = address.split('.')
        if (parts.size != 4 || parts.any { it.isEmpty() || it.length > 3 ||
                it.any { c -> c !in '0'..'9' } || it.length > 1 && it[0] == '0' }) return false
        val octets = parts.map { it.toInt() }
        if (octets.any { it > 255 }) return false
        val (a, b, c) = octets
        return a !in setOf(0, 10, 127) && !(a == 169 && b == 254 && c !in 1..254) &&
            !(a == 172 && b in 16..31) && !(a == 192 && b == 168) && a !in 224..239 &&
            octets.any { it != 255 } && address !in phoneAddresses
    }

    fun needsAndroid13Warning(sdk: Int, address: String): Boolean =
        sdk >= 33 && valid(address) && !address.startsWith("169.254.")
}

fun RelaySettings.teslaAddress(): String? = when (teslaBrowser) {
    TeslaBrowserMode.OFF -> null
    TeslaBrowserMode.VIRTUAL_NAME -> teslaVirtualAddress
    TeslaBrowserMode.PUBLIC_ADDRESS -> teslaPublicAddress
    TeslaBrowserMode.TESLAMIRROR -> teslaMirrorAddress
    TeslaBrowserMode.TESLA_ANDROID -> teslaAndroidAddress
}

/** Keep generated TLS ready before the VPN appears; TlsStore retains previously covered addresses. */
fun teslaTlsAddresses(addresses: Set<String>, mode: TeslaBrowserMode, address: String): Set<String> =
    if (mode != TeslaBrowserMode.OFF && !mode.usesTrustedName && TeslaAddressPolicy.valid(address)) addresses + address else addresses

/** Public DNS changes only after the local address was actually established. */
fun teslaPublishedAddress(mode: TeslaBrowserMode, activeAddress: String?): String? =
    activeAddress.takeIf { mode.usesTrustedName }
