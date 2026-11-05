// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.ui

internal enum class ExperimentalOption { CARLINK_WIFI_RECOVERY, SEND_BLUETOOTH_ADDRESS }

internal data class ExperimentalSection(val version: String, val options: List<ExperimentalOption>)

internal val experimentalSections = listOf(
    ExperimentalSection("0.2.7", listOf(ExperimentalOption.CARLINK_WIFI_RECOVERY)),
    ExperimentalSection("0.2.0", listOf(ExperimentalOption.SEND_BLUETOOTH_ADDRESS)),
)
