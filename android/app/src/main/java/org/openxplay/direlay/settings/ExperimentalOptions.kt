// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.settings

data class ExperimentalOptions(
    val active: Boolean = false,
    val rememberSubscription: Boolean = false,
    val keepScanning: Boolean = false,
    val recordAdvertisements: Boolean = false,
    val sendBluetoothAddress: Boolean = false,
    val bluetoothAddress: String = "",
    val isolateCarlinkNetwork: Boolean = false,
    val waitForCarlink: Boolean = false,
    val recoverWirelessNetwork: Boolean = false,
) {
    fun effective(carlinkTrace: Boolean = true): ExperimentalOptions {
        if (!active) return ExperimentalOptions()
        val address = normalizedBluetoothAddress(bluetoothAddress)
        return copy(recordAdvertisements = recordAdvertisements && carlinkTrace,
            sendBluetoothAddress = sendBluetoothAddress && address != null,
            bluetoothAddress = address.orEmpty())
    }

    val invalidAddress: Boolean get() = active && sendBluetoothAddress && normalizedBluetoothAddress(bluetoothAddress) == null
    val optionCount: Int get() = listOf(rememberSubscription, keepScanning, recordAdvertisements, sendBluetoothAddress,
        isolateCarlinkNetwork, waitForCarlink, recoverWirelessNetwork).count { it }

    fun names(): List<String> = listOf(rememberSubscription to "remember_subscription", keepScanning to "keep_scanning",
        recordAdvertisements to "record_advertisements", sendBluetoothAddress to "send_bluetooth_address",
        isolateCarlinkNetwork to "isolate_carlink_network", waitForCarlink to "wait_for_carlink",
        recoverWirelessNetwork to "recover_wireless_network")
        .filter { it.first }.map { it.second }

    companion object {
        fun normalizedBluetoothAddress(value: String): String? {
            val octets = value.split(':')
            if (octets.size != 6 || octets.any { it.length != 2 || it.any { c -> c !in "0123456789abcdefABCDEF" } }) return null
            val bytes = octets.map { it.toInt(16) }
            if (bytes.first() and 1 != 0 || bytes.all { it == 0 } || bytes == listOf(2, 0, 0, 0, 0, 0)) return null
            return value.lowercase(java.util.Locale.ROOT)
        }
    }
}
