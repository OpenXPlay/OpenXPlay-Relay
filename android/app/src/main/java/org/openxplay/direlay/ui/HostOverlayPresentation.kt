// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.stream.core.SessionPhase

enum class HostOverlayAction { BACK, STOP }
data class HostOverlayButton(val label: Int, val action: HostOverlayAction)
data class HostOverlaySnapshot(val home: HomeSnapshot, val iphoneName: String? = null,
    val receiverUrls: List<String> = emptyList(), val stopped: Boolean = false)
data class HostOverlayPresentation(val headline: Int, val hint: Int?, val tone: StatusTone,
    val iphoneName: String?, val outputs: List<Int>, val receiverUrls: List<String>,
    val buttons: List<HostOverlayButton>)

/** Session state and resource IDs only: no Android services or mutable runtime references. */
fun presentHostOverlay(value: HostOverlaySnapshot): HostOverlayPresentation {
    val home = presentHome(value.home)
    val stopped = value.stopped && !value.home.failure
    val reconnecting = value.home.phase in setOf(SessionPhase.RECONNECTING, SessionPhase.ENDED)
    val tone = when {
        value.home.failure -> StatusTone.ERROR
        stopped -> StatusTone.IDLE
        value.home.phase == SessionPhase.ACTIVE -> StatusTone.RUNNING
        else -> StatusTone.PROGRESS
    }
    val headline = when {
        tone == StatusTone.ERROR -> home.headline
        stopped -> R.string.host_stopped
        tone == StatusTone.RUNNING -> R.string.host_running
        reconnecting -> R.string.ui_connecting
        else -> R.string.host_connecting
    }
    val settings = value.home.settings
    return HostOverlayPresentation(headline, when (tone) {
        StatusTone.ERROR -> home.subline
        StatusTone.RUNNING -> R.string.host_keep_running
        StatusTone.PROGRESS -> R.string.ui_connecting_hint
        else -> null
    }, tone, value.iphoneName?.takeIf { it.isNotBlank() }, buildList {
        if (!settings.carlinkOutput || settings.bothOutputs) add(R.string.ui_browser)
        if (settings.carlinkOutput || settings.bothOutputs) add(R.string.ui_carlink)
    }, if (!settings.carlinkOutput || settings.bothOutputs) value.receiverUrls else emptyList(), buildList {
        add(HostOverlayButton(R.string.host_back, HostOverlayAction.BACK))
        if (!stopped) add(HostOverlayButton(
            if (tone == StatusTone.PROGRESS) R.string.host_cancel else R.string.ui_stop, HostOverlayAction.STOP))
    })
}

/** Keep the view factory lazy: preview must not create a covering view or start its polling. */
internal fun <T> hostOverlayUnlessPreview(localPreview: Boolean, create: () -> T): T? =
    if (localPreview) null else create()

/** The first reachable TLS receiver address also serves the plain HTTP landing page. */
fun landingHost(urls: List<String>): String? = urls.firstOrNull()?.let { address ->
    val uri = runCatching { java.net.URI(address) }.getOrNull() ?: return@let null
    uri.host?.takeIf { uri.scheme == "https" }
}
