// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.ui

internal enum class ExperimentalOption {
    IAP_CHANNEL_WHEN_OFFERED,
    RELEASE_BLUETOOTH_ON_REQUEST,
    AWAIT_IPHONE_RECONNECT,
    SCREEN_SESSION_ISOLATION,
    SCREEN_RECONNECT_RESET,
    SCREEN_DELIVERY_SERIALIZATION,
    REMOTE_PLAYBACK_PROOF,
    CANCEL_RECOVERED_RETRIES,
    STARTUP_STATUS,
    STARTUP_DIAGNOSTICS,
    NETWORK_OBSERVATION_BACKOFF,
    SEND_BLUETOOTH_ADDRESS,
}

internal data class ExperimentalSection(val version: String, val options: List<ExperimentalOption>)

internal val experimentalSections = listOf(
    ExperimentalSection("0.2.3", listOf(
        ExperimentalOption.IAP_CHANNEL_WHEN_OFFERED,
        ExperimentalOption.RELEASE_BLUETOOTH_ON_REQUEST,
        ExperimentalOption.AWAIT_IPHONE_RECONNECT)),
    ExperimentalSection("0.2.2", listOf(
        ExperimentalOption.SCREEN_SESSION_ISOLATION,
        ExperimentalOption.SCREEN_RECONNECT_RESET,
        ExperimentalOption.SCREEN_DELIVERY_SERIALIZATION)),
    ExperimentalSection("0.2.1", listOf(
        ExperimentalOption.REMOTE_PLAYBACK_PROOF,
        ExperimentalOption.CANCEL_RECOVERED_RETRIES,
        ExperimentalOption.STARTUP_STATUS,
        ExperimentalOption.STARTUP_DIAGNOSTICS,
        ExperimentalOption.NETWORK_OBSERVATION_BACKOFF)),
    ExperimentalSection("0.2.0", listOf(ExperimentalOption.SEND_BLUETOOTH_ADDRESS)),
)
