// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.path.*

enum class HostOverlayAction { BACK, STOP, RETRY }
data class HostOverlayButton(val label: Int, val action: HostOverlayAction)
data class HostOverlaySnapshot(val input: PathInput, val receiverUrls: List<String> = emptyList(),
    val localPreview: Boolean = false, val problems: List<HomeProblem> = emptyList())
data class HostOverlayPresentation(val path: ConnectionPath, val receiverUrls: List<String>,
    val buttons: List<HostOverlayButton>, val problems: List<HomeProblem> = emptyList())

/** The host and Home use exactly the same readiness, failures and live state. */
fun presentHostOverlay(value: HostOverlaySnapshot): HostOverlayPresentation {
    val path = presentPath(value.input)
    return HostOverlayPresentation(path, value.receiverUrls.distinct(), buildList {
        add(HostOverlayButton(R.string.home2_back, HostOverlayAction.BACK))
        add(HostOverlayButton(R.string.design_stop, HostOverlayAction.STOP))
        if (path.steps.any { it.state == StepState.FAILED })
            add(HostOverlayButton(R.string.path_action_retry, HostOverlayAction.RETRY))
    }, value.problems)
}

/** Keep the view factory lazy: preview must not create a covering view or start its polling. */
internal fun <T> hostOverlayUnlessPreview(localPreview: Boolean, create: () -> T): T? =
    if (localPreview) null else create()
