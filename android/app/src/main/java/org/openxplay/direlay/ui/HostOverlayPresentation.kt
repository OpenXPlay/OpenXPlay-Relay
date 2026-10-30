// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import org.openxplay.direlay.path.*

enum class HostOverlayAction { BACK, STOP }
data class HostOverlaySnapshot(val input: PathInput, val receiverUrls: List<String> = emptyList(),
    val localPreview: Boolean = false, val problems: List<HomeProblem> = emptyList(),
    val path: ConnectionPath = presentPath(input),
    val displayChanged: Boolean = false, val carIncomplete: Boolean = false, val setupCompleted: Boolean = true,
    val highPerformance: Boolean = true)
data class HostOverlayPresentation(val path: ConnectionPath, val receiverUrls: List<String>,
    val problems: List<HomeProblem> = emptyList(), val displayChanged: Boolean = false,
    val carIncomplete: Boolean = false, val setupCompleted: Boolean = true, val highPerformance: Boolean = true)

/** The host and Home use exactly the same readiness, failures and live state. */
fun presentHostOverlay(value: HostOverlaySnapshot): HostOverlayPresentation {
    return HostOverlayPresentation(value.path, value.receiverUrls.distinct(), value.problems,
        value.displayChanged, value.carIncomplete, value.setupCompleted, value.highPerformance)
}

/** Keep the view factory lazy: preview must not create a covering view or start its polling. */
internal fun <T> hostOverlayUnlessPreview(localPreview: Boolean, create: () -> T): T? =
    if (localPreview) null else create()
