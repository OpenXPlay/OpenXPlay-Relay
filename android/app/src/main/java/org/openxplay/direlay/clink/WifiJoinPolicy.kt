// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.clink

/** Pure platform choice and endpoint validation; never guesses a peer address. */
object WifiJoinPolicy {
    internal fun <T> withWifi(wifiEnabled: Boolean?, connect: () -> T): T {
        if (wifiEnabled == false) throw ClinkFailure(ClinkFailureReason.WIFI_OFF)
        return connect()
    }
    enum class Method { P2P_CREDENTIALS, NETWORK_SPECIFIER, UNSUPPORTED }
    fun first(sdk: Int, p2pAvailable: Boolean, replyType: Int = 1001): Method = when {
        sdk < 29 -> Method.UNSUPPORTED
        replyType == 1002 -> Method.NETWORK_SPECIFIER
        p2pAvailable -> Method.P2P_CREDENTIALS
        else -> Method.NETWORK_SPECIFIER
    }
    fun mayRemoveGroup(groupFormed: Boolean, isGroupOwner: Boolean,
        networkName: String?, expectedSsid: String): Boolean =
        groupFormed && !isGroupOwner && networkName != null && networkName == expectedSsid

    fun fallback(sdk: Int, method: Method): Method =
        if (sdk >= 29 && method == Method.P2P_CREDENTIALS) Method.NETWORK_SPECIFIER else Method.UNSUPPORTED
    fun endpoints(peer: String?, local: String?): Pair<String, String>? {
        fun valid(value: String?): Boolean {
            val parts = value?.split('.') ?: return false
            if (parts.size != 4) return false
            val numbers = parts.map { it.toIntOrNull() ?: return false }
            return numbers.all { it in 0..255 } && numbers[0] in 1..223 && numbers[0] != 127 &&
                value != "169.254.${numbers[2]}.${numbers[3]}"
        }
        return if (valid(peer) && valid(local) && peer != local) peer!! to local!! else null
    }
}
