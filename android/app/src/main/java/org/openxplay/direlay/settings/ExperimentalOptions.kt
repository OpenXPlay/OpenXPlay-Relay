// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.settings

/**
 * Laboratory choices; promoted features remain enabled independently of the laboratory master.
 * Legacy constructor choices remain readable in saved drafts, but effective() ignores them.
 */
data class ExperimentalOptions(
    val active: Boolean = true,
    val carlinkWifiRecovery: Boolean = true,
    val rememberSubscription: Boolean = true,
    val keepScanning: Boolean = true,
    val recordAdvertisements: Boolean = true,
    val sendBluetoothAddress: Boolean = false,
    val bluetoothAddress: String = "",
    val isolateCarlinkNetwork: Boolean = true,
    val waitForCarlink: Boolean = true,
    val recoverWirelessNetwork: Boolean = true,
    val remotePlaybackProof: Boolean = true,
    val cancelRecoveredRetries: Boolean = true,
    val startupStatus: Boolean = true,
    val startupDiagnostics: Boolean = true,
    val networkObservationBackoff: Boolean = true,
    val screenSessionIsolation: Boolean = true,
    val screenReconnectReset: Boolean = true,
    val screenDeliverySerialization: Boolean = true,
    val iapChannelWhenOffered: Boolean = true,
    val releaseBluetoothOnRequest: Boolean = true,
    val awaitIphoneReconnect: Boolean = true,
    val radioConflictPrompt: Boolean = true,
    val stallWatchdog: Boolean = true,
) {
    fun effective(carlinkTrace: Boolean = true): ExperimentalOptions {
        val address = normalizedBluetoothAddress(bluetoothAddress)
        return copy(carlinkWifiRecovery = active && carlinkWifiRecovery, rememberSubscription = true, keepScanning = true, recordAdvertisements = carlinkTrace,
            isolateCarlinkNetwork = true, waitForCarlink = true, recoverWirelessNetwork = true,
            sendBluetoothAddress = active && sendBluetoothAddress && address != null,
            bluetoothAddress = if (active) address.orEmpty() else "",
            remotePlaybackProof = true,
            cancelRecoveredRetries = true,
            startupStatus = true,
            startupDiagnostics = true,
            networkObservationBackoff = true,
            screenSessionIsolation = true,
            screenReconnectReset = true,
            screenDeliverySerialization = true,
            iapChannelWhenOffered = true,
            releaseBluetoothOnRequest = true,
            awaitIphoneReconnect = true,
            radioConflictPrompt = true,
            stallWatchdog = true)
    }

    val invalidAddress: Boolean get() = active && sendBluetoothAddress && normalizedBluetoothAddress(bluetoothAddress) == null
    val optionCount: Int get() = listOf(carlinkWifiRecovery, sendBluetoothAddress).count { it }

    fun names(): List<String> = listOf(carlinkWifiRecovery to "carlink_wifi_recovery",
        sendBluetoothAddress to "send_bluetooth_address").filter { it.first }.map { it.second }

    companion object {
        val OFF = ExperimentalOptions(active = false, carlinkWifiRecovery = false)

        fun normalizedBluetoothAddress(value: String): String? {
            val octets = value.split(':')
            if (octets.size != 6 || octets.any { it.length != 2 || it.any { c -> c !in "0123456789abcdefABCDEF" } }) return null
            val bytes = octets.map { it.toInt(16) }
            if (bytes.first() and 1 != 0 || bytes.all { it == 0 } || bytes == listOf(2, 0, 0, 0, 0, 0)) return null
            return value.lowercase(java.util.Locale.ROOT)
        }
    }
}
