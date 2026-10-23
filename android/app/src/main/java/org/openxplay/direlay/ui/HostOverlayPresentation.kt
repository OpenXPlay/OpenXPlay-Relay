// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import org.openxplay.direlay.R
import org.openxplay.direlay.relay.VideoMeasurement
import org.openxplay.direlay.relay.VideoStatsSnapshot
import org.openxplay.direlay.settings.VideoStatsWindow
import org.openxplay.stream.core.SessionPhase
import org.openxplay.stream.core.VideoCodec
import java.util.Locale

enum class HostOverlayAction { BACK, STOP }
data class HostOverlayButton(val label: Int, val action: HostOverlayAction)
data class HostOverlaySize(val width: Int, val height: Int) {
    val text: String get() = "$width × $height"
}
data class HostOverlaySnapshot(val home: HomeSnapshot, val iphoneName: String? = null,
    val receiverUrls: List<String> = emptyList(), val stopped: Boolean = false,
    val publicNameUrl: String? = null, val carIpv4: List<String> = emptyList(),
    val carlinkState: Int = R.string.ui_searching, val carlinkWindow: HostOverlaySize? = null,
    val canvas: String? = null, val screenWidthMm: Int? = null, val videoCodec: VideoCodec? = null,
    val carPlayScreen: Pair<String, String>? = null, val viewport: HostOverlaySize? = null,
    val videoStats: VideoStatsSnapshot? = null, val teslaUrl: String? = null)
data class HostOverlayDetail(val label: Int, val value: String = "", val monospace: Boolean = false,
    val valueResource: Int? = null, val arguments: List<String> = emptyList())
data class HostOverlaySection(val title: Int, val rows: List<HostOverlayDetail>)
data class HostOverlayPresentation(val headline: Int, val hint: Int?, val tone: StatusTone,
    val iphoneName: String?, val outputs: List<Int>, val receiverUrls: List<String>,
    val buttons: List<HostOverlayButton>, val hintArguments: List<String> = emptyList(),
    val browserHint: Int? = null, val problems: List<HomeProblem> = emptyList(),
    val sections: List<HostOverlaySection> = emptyList(), val videoStats: VideoStatsPresentation? = null,
    val teslaUrl: String? = null)

/** Session state and resource IDs only: no Android services or mutable runtime references. */
fun presentHostOverlay(value: HostOverlaySnapshot): HostOverlayPresentation {
    val home = presentHome(value.home)
    val stopped = value.stopped && home.tone != StatusTone.ERROR
    val tone = when {
        home.tone == StatusTone.ERROR -> StatusTone.ERROR
        stopped -> StatusTone.IDLE
        value.home.phase == SessionPhase.ACTIVE -> StatusTone.RUNNING
        else -> StatusTone.PROGRESS
    }
    val connection = presentConnectionStatus(value.home.phase, value.home.stage, value.home.wireless, value.home.deviceName)
    val headline = when {
        tone == StatusTone.ERROR -> home.headline
        stopped -> R.string.host_stopped
        tone == StatusTone.RUNNING -> R.string.host_running
        else -> checkNotNull(connection).headline
    }
    val settings = value.home.settings
    val browser = !settings.carlinkOutput || settings.bothOutputs
    val carlink = settings.carlinkOutput || settings.bothOutputs
    val urls = if (browser) (listOfNotNull(value.teslaUrl, value.publicNameUrl?.takeIf { it.isNotBlank() }) + value.receiverUrls).distinct()
        else emptyList()
    val stats = value.videoStats?.takeIf { settings.videoStats != VideoStatsWindow.OFF && it.window == settings.videoStats }
    val measuredFps = (stats?.iphone as? VideoMeasurement.Values)?.fps
    val sections = buildList {
        if (browser) add(HostOverlaySection(R.string.host_head_unit_address, buildList {
            if (urls.isEmpty() && value.carIpv4.isNotEmpty())
                add(HostOverlayDetail(R.string.network_addresses, value.carIpv4.joinToString("\n"), monospace = true))
        }))
        if (carlink) add(HostOverlaySection(R.string.ui_carlink, buildList {
            add(HostOverlayDetail(R.string.host_state, valueResource = value.carlinkState))
            value.carlinkWindow?.let { add(HostOverlayDetail(R.string.picture_window, it.text, monospace = true)) }
        }))
        add(HostOverlaySection(R.string.picture_title, listOf(
            HostOverlayDetail(R.string.picture_resolution, value.canvas ?: "—", monospace = true),
            HostOverlayDetail(R.string.picture_screen_width, "${value.screenWidthMm ?: settings.carplayScreenWidth.mm} mm", monospace = true),
            HostOverlayDetail(R.string.direlay_fps, valueResource = if (measuredFps == null) R.string.host_frame_rate
                else R.string.host_frame_rate_measured, arguments = listOfNotNull(settings.fps.toString(),
                    measuredFps?.let { String.format(Locale.ROOT, "%.1f", it) })),
            HostOverlayDetail(R.string.picture_video_codec, when (value.videoCodec) {
                VideoCodec.H264 -> "H.264"
                VideoCodec.H265 -> "H.265"
                null -> "—"
            })
        )))
        add(HostOverlaySection(R.string.host_detected_screen, buildList {
            val screen = value.carPlayScreen
            add(if (screen == null) HostOverlayDetail(R.string.dev_carplay_screen, "—", monospace = true)
                else HostOverlayDetail(R.string.dev_carplay_screen, monospace = true, valueResource = screenSourceLabel(screen.second),
                    arguments = listOf(screen.first)))
            val viewport = value.viewport
            add(HostOverlayDetail(R.string.picture_window, if (viewport != null && viewport.width > 0 && viewport.height > 0)
                "${viewport.text} · ${String.format(Locale.ROOT, "%.2f", viewport.width.toDouble() / viewport.height)}:1" else "—",
                monospace = true))
        }))
    }
    return HostOverlayPresentation(headline, when (tone) {
        StatusTone.ERROR -> home.subline
        StatusTone.RUNNING -> R.string.host_keep_running
        StatusTone.PROGRESS -> checkNotNull(connection).subline
        else -> null
    }, tone, value.iphoneName?.takeIf { tone != StatusTone.ERROR && it.isNotBlank() }, buildList {
        if (browser) add(R.string.ui_browser)
        if (carlink) add(R.string.ui_carlink)
    }, urls, buildList {
        add(HostOverlayButton(R.string.host_back, HostOverlayAction.BACK))
        if (!stopped) add(HostOverlayButton(
            if (tone == StatusTone.PROGRESS) R.string.host_cancel else R.string.ui_stop, HostOverlayAction.STOP))
    }, when (tone) {
        StatusTone.ERROR -> home.sublineArguments
        StatusTone.PROGRESS -> checkNotNull(connection).sublineArguments
        else -> emptyList()
    }, when {
        !browser || value.home.webReason != null -> null
        urls.isNotEmpty() -> R.string.ui_browser_https_hint
        value.home.webListening && !value.home.carNetworkAvailable -> R.string.ui_head_unit_network
        else -> R.string.ui_no_network
    }, home.problems + home.hints.filter { it.title == R.string.tesla_title }, sections,
        if (settings.videoStats == VideoStatsWindow.OFF) null else stats?.let(::presentVideoStats)
        ?: VideoStatsPresentation(VideoStatsText(R.string.video_stats_measuring, listOf(settings.videoStats.seconds.toString())), emptyList()), value.teslaUrl)
}

internal fun screenSourceLabel(source: String): Int = when (source) {
    "car screen" -> R.string.dev_screen_car
    "remembered" -> R.string.dev_screen_remembered
    "built-in model" -> R.string.dev_screen_model
    "default" -> R.string.dev_screen_default
    "fixed" -> R.string.dev_screen_fixed
    else -> R.string.dev_screen_relay
}

/** Keep the view factory lazy: preview must not create a covering view or start its polling. */
internal fun <T> hostOverlayUnlessPreview(localPreview: Boolean, create: () -> T): T? =
    if (localPreview) null else create()
