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
        return copy(rememberSubscription = true, keepScanning = true, recordAdvertisements = carlinkTrace,
            isolateCarlinkNetwork = true, waitForCarlink = true, recoverWirelessNetwork = true,
            sendBluetoothAddress = active && sendBluetoothAddress && address != null,
            bluetoothAddress = if (active) address.orEmpty() else "",
            remotePlaybackProof = active && remotePlaybackProof,
            cancelRecoveredRetries = active && cancelRecoveredRetries,
            startupStatus = active && startupStatus,
            startupDiagnostics = active && startupDiagnostics,
            networkObservationBackoff = active && networkObservationBackoff,
            screenSessionIsolation = active && screenSessionIsolation,
            screenReconnectReset = active && screenReconnectReset,
            screenDeliverySerialization = active && screenDeliverySerialization,
            iapChannelWhenOffered = active && iapChannelWhenOffered,
            releaseBluetoothOnRequest = active && releaseBluetoothOnRequest,
            awaitIphoneReconnect = active && awaitIphoneReconnect,
            radioConflictPrompt = active && radioConflictPrompt,
            stallWatchdog = active && stallWatchdog)
    }

    val invalidAddress: Boolean get() = active && sendBluetoothAddress && normalizedBluetoothAddress(bluetoothAddress) == null
    val optionCount: Int get() = listOf(remotePlaybackProof, cancelRecoveredRetries, startupStatus, startupDiagnostics,
        networkObservationBackoff, screenSessionIsolation, screenReconnectReset, screenDeliverySerialization,
        iapChannelWhenOffered, releaseBluetoothOnRequest, awaitIphoneReconnect, radioConflictPrompt, stallWatchdog,
        sendBluetoothAddress).count { it }

    fun names(): List<String> = listOf(rememberSubscription to "remember_subscription", keepScanning to "keep_scanning",
        recordAdvertisements to "record_advertisements", sendBluetoothAddress to "send_bluetooth_address",
        isolateCarlinkNetwork to "isolate_carlink_network", waitForCarlink to "wait_for_carlink",
        recoverWirelessNetwork to "recover_wireless_network",
        remotePlaybackProof to "remote_playback_proof",
        cancelRecoveredRetries to "cancel_recovered_retries",
        startupStatus to "startup_status",
        startupDiagnostics to "startup_diagnostics",
        networkObservationBackoff to "network_observation_backoff",
        screenSessionIsolation to "screen_session_isolation",
        screenReconnectReset to "screen_reconnect_reset",
        screenDeliverySerialization to "screen_delivery_serialization",
        iapChannelWhenOffered to "iap_channel_when_offered",
        releaseBluetoothOnRequest to "release_bluetooth_on_request",
        awaitIphoneReconnect to "await_iphone_reconnect",
        radioConflictPrompt to "radio_conflict_prompt",
        stallWatchdog to "stall_watchdog")
        .filter { it.first }.map { it.second }

    companion object {
        val OFF = ExperimentalOptions(active = false,
            remotePlaybackProof = false,
            cancelRecoveredRetries = false,
            startupStatus = false,
            startupDiagnostics = false,
            networkObservationBackoff = false,
            screenSessionIsolation = false,
            screenReconnectReset = false,
            screenDeliverySerialization = false,
            iapChannelWhenOffered = false,
            releaseBluetoothOnRequest = false,
            awaitIphoneReconnect = false,
            radioConflictPrompt = false,
            stallWatchdog = false)

        fun normalizedBluetoothAddress(value: String): String? {
            val octets = value.split(':')
            if (octets.size != 6 || octets.any { it.length != 2 || it.any { c -> c !in "0123456789abcdefABCDEF" } }) return null
            val bytes = octets.map { it.toInt(16) }
            if (bytes.first() and 1 != 0 || bytes.all { it == 0 } || bytes == listOf(2, 0, 0, 0, 0, 0)) return null
            return value.lowercase(java.util.Locale.ROOT)
        }
    }
}
