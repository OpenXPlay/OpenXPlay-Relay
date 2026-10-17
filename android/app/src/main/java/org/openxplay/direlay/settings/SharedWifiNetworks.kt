// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.settings

// Credentials must never be included in diagnostics or logs.
data class SharedWifiNetwork(val ssid: String, val passphrase: String) {
    override fun toString() = "SharedWifiNetwork(ssid=$ssid)"
}
object SharedWifiNetworks {
    const val MAX_SIZE = 10
    fun validateCredentials(ssid: String, passphrase: String) {
        require(ssid.isNotBlank() && ssid.toByteArray(Charsets.UTF_8).size in 1..32 && '\u0000' !in ssid) {
            "Enter an SSID containing 1–32 UTF-8 bytes without null characters."
        }
        require((passphrase.isEmpty() || passphrase.length in 8..63) && '\u0000' !in passphrase) {
            "Passphrase must be empty for an open network, or 8–63 characters without null characters."
        }
    }
    fun validate(networks: List<SharedWifiNetwork>) {
        require(networks.size <= MAX_SIZE)
        require(networks.map { it.ssid }.distinct().size == networks.size)
        networks.forEach { validateCredentials(it.ssid, it.passphrase) }
    }
    // Length prefixes preserve arbitrary Unicode and delimiters without Android dependencies.
    fun encode(networks: List<SharedWifiNetwork>): String {
        validate(networks)
        return networks.flatMap { listOf(it.ssid, it.passphrase) }.joinToString("") { "${it.length}:$it" }
    }
    fun decode(value: String): List<SharedWifiNetwork> {
        var offset = 0
        fun read(): String {
            val end = value.indexOf(':', offset)
            require(end >= offset)
            val size = requireNotNull(value.substring(offset, end).toIntOrNull())
            require(size >= 0 && size <= value.length - end - 1)
            offset = end + 1 + size
            return value.substring(end + 1, offset)
        }
        return buildList<SharedWifiNetwork> {
            while (offset < value.length) { require(size < MAX_SIZE); add(SharedWifiNetwork(read(), read())) }
        }.also(::validate)
    }
    fun migrate(stored: String?, ssid: String, password: String): List<SharedWifiNetwork> =
        if (stored != null) decode(stored) else if (ssid.isBlank()) emptyList()
        else listOf(SharedWifiNetwork(ssid, password)).also(::validate)
}
fun resolveSharedWifi(currentSsid: String?, networks: List<SharedWifiNetwork>): SharedWifiNetwork? =
    currentSsid?.let { ssid -> networks.firstOrNull { it.ssid == ssid } }

/** An unreadable SSID can be selected explicitly, until the observed Wi-Fi changes. */
class SharedWifiSelection {
    private var observed: String? = null
    private var manual: String? = null
    @Synchronized fun resolve(current: String?, networks: List<SharedWifiNetwork>): SharedWifiNetwork? {
        if (current != observed) { manual = null; observed = current }
        return resolveSharedWifi(current, networks) ?: if (current == null) networks.firstOrNull { it.ssid == manual } else null
    }
    @Synchronized fun use(ssid: String) { manual = ssid }
    @Synchronized fun wifiChanged() { manual = null }
}
