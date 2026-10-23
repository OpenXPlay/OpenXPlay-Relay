// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.miracast.MiracastDisplay
import org.openxplay.direlay.miracast.MiracastState
import org.openxplay.direlay.relay.WirelessDisplayFailure
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.stream.cast.CastState

fun outputSummaryLabels(settings: RelaySettings): List<Int> = buildList {
    add(if (settings.bothOutputs) R.string.menu_both_outputs else if (settings.carlinkOutput) R.string.ui_carlink else R.string.ui_browser)
    if (settings.dlnaOutput) add(R.string.wireless_dlna)
    if (settings.miracastOutput) add(R.string.wireless_miracast)
}

fun outputSummary(settings: RelaySettings, label: (Int) -> String): String =
    outputSummaryLabels(settings).joinToString(" · ", transform = label)

data class WirelessDisplayLine(val title: Int, val text: Int, val arguments: List<String> = emptyList())

fun wirelessDisplayLines(settings: RelaySettings, dlnaState: CastState?, rendererName: String?,
    miracastState: MiracastState?, display: MiracastDisplay?): List<WirelessDisplayLine> = buildList {
    if (settings.dlnaOutput) {
        val name = rendererName ?: settings.dlnaRenderer?.name.orEmpty()
        val text = if (settings.dlnaRenderer == null) R.string.dlna_choose_receiver else when (dlnaState) {
            CastState.WAITING_FOR_VIDEO, CastState.ENDED -> R.string.home_dlna_waiting
            CastState.STARTING -> R.string.home_dlna_starting
            CastState.PLAYING -> R.string.home_dlna_playing
            CastState.PAUSED -> R.string.home_dlna_paused
            CastState.STOPPED_BY_RENDERER -> R.string.home_dlna_stopped
            else -> R.string.home_dlna_searching
        }
        add(WirelessDisplayLine(R.string.wireless_dlna, text, listOf(name)))
    }
    if (settings.miracastOutput) add(when {
        miracastState == MiracastState.NEEDS_PERMISSION -> WirelessDisplayLine(R.string.wireless_miracast, R.string.miracast_allow_overlay)
        miracastState == MiracastState.SHOWING && display != null -> WirelessDisplayLine(R.string.wireless_miracast,
            R.string.home_miracast_showing, listOf(display.name, display.width.toString(), display.height.toString()))
        else -> WirelessDisplayLine(R.string.wireless_miracast, R.string.home_miracast_waiting)
    })
}

fun wirelessDisplayProblem(outputId: String, failure: WirelessDisplayFailure): HomeProblem = HomeProblem(
    if (outputId == "cast") R.string.home_problem_dlna else R.string.home_problem_miracast,
    text = failure.code, detail = failure.detail, textResource = when (failure.code) {
        "renderer_not_found" -> R.string.dlna_error_not_found
        "renderer_unreachable" -> R.string.dlna_error_unreachable
        "renderer_no_avtransport" -> R.string.dlna_error_no_avtransport
        "renderer_rejected" -> R.string.dlna_error_rejected
        "renderer_stopped" -> R.string.dlna_error_stopped
        "stream_server_failed" -> R.string.dlna_error_server
        "unsupported_video" -> R.string.dlna_error_video
        "overlay_denied" -> R.string.miracast_allow_overlay
        "window_failed" -> R.string.miracast_error_window
        "decoder_failed" -> R.string.miracast_error_decoder
        "display_lost" -> R.string.miracast_error_display
        else -> null
    }, atMillis = failure.atMillis)
