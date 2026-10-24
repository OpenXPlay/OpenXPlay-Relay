// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.video.CarlinkGeometry
import org.openxplay.direlay.video.VideoCompatibility

data class DeveloperRow(val label: Int, val value: String, val argument: String = "", val valueKey: Int = 0,
    val valueArguments: List<String> = emptyList())
data class DeveloperView(val rows: List<DeveloperRow> = emptyList(), val urls: List<String> = emptyList(),
    val fingerprint: String = "", val identity: Int = R.string.direlay_missing,
    val carlinkGeometry: CarlinkGeometry? = null, val carlinkGeometryMode: VideoCompatibility.Mode? = null)
fun developerView(runtime: RelayRuntime): DeveloperView {
    return DeveloperView(buildList {
    runtime.carPlayScreenSummary.let { (size, source) ->
        add(DeveloperRow(R.string.dev_carplay_screen, "", valueKey = when (source) {
            "car screen" -> R.string.dev_screen_car
            "remembered" -> R.string.dev_screen_remembered
            "built-in model" -> R.string.dev_screen_model
            "default" -> R.string.dev_screen_default
            "fixed" -> R.string.dev_screen_fixed
            else -> R.string.dev_screen_relay
        }, valueArguments = listOf(size)))
    }
    runtime.displayPolicy.primaryDisplay?.viewport?.let {
        add(DeveloperRow(R.string.picture_window, "${it.width} × ${it.height} · ${"%.2f".format(java.util.Locale.ROOT, it.width.toDouble() / it.height)}:1"))
    }
    add(DeveloperRow(R.string.picture_video_codec, "${runtime.status.videoCodec ?: runtime.appliedVideoDecision.codec} · ${runtime.videoDecision.reason}"))
    val webReason = runtime.serverReason
    add(DeveloperRow(R.string.dev_receiver, "", valueKey = when (webReason?.failure) {
        org.openxplay.direlay.relay.WebStartFailure.PORT_BUSY -> R.string.ui_web_port_busy
        org.openxplay.direlay.relay.WebStartFailure.OTHER -> R.string.ui_web_start_failed
        null -> developerStateKey(runtime.serverState)
    }, valueArguments = if (webReason?.failure == org.openxplay.direlay.relay.WebStartFailure.PORT_BUSY)
        listOf(webReason.port.toString()) else emptyList()))
    add(DeveloperRow(R.string.ui_carlink, "", valueKey = developerStateKey(runtime.carlinkState)))
    add(DeveloperRow(R.string.dev_iphone_selection, runtime.iphoneSelection().diagnostic()))
    add(DeveloperRow(R.string.dev_session, "", valueKey = developerStateKey(runtime.status.session?.phase?.name ?: "idle")))
    runtime.status.frameCounts.toSortedMap().forEach { (screen, frames) -> add(DeveloperRow(R.string.dev_frames, frames.toString(), screen.toString())) }
    add(DeveloperRow(R.string.dev_clients, runtime.status.recentWebReporters.toString()))
    runtime.status.outputMeasurements.forEach { (output, stats) ->
        val source = "$output / ${stats.screen.value}"
        add(DeveloperRow(R.string.dev_decode, stats.decodeTimeUs?.toString() ?: "—", source))
        add(DeveloperRow(R.string.dev_queue, stats.decodeQueue.toString(), source))
        add(DeveloperRow(R.string.dev_dropped, stats.dropped.toString(), source))
        add(DeveloperRow(R.string.dev_rendered, stats.rendered.toString(), source))
        add(DeveloperRow(R.string.dev_latency, stats.endToEndLatencyUs?.toString() ?: "—", source))
        add(DeveloperRow(R.string.dev_audio_buffer, stats.audioBufferedUs?.toString() ?: "—", source))
        add(DeveloperRow(R.string.dev_underruns, stats.audioUnderruns.toString(), source))
    }
    runtime.input.keyframeLog.forEachIndexed { index, item ->
        val parts = Regex("(\\d+): Screen (\\d+): (.+)").matchEntire(item)?.groupValues
        val key = when (parts?.get(3)) {
            "requested" -> R.string.dev_keyframe_requested
            "no recovery handler" -> R.string.dev_keyframe_no_handler
            "request failed" -> R.string.dev_keyframe_failed
            "stale request" -> R.string.dev_keyframe_stale
            "queue full" -> R.string.dev_keyframe_queue_full
            else -> R.string.dev_unavailable
        }
        add(DeveloperRow(R.string.dev_keyframe, "", index.toString(), key,
            if (parts == null) emptyList() else listOf(snapshotTime(parts[1].toLong()), parts[2])))
    }
    runtime.status.failure?.let { add(DeveloperRow(R.string.dev_failure, "", valueKey = R.string.ui_error)) }
}, runtime.receiverUrls, runCatching { runtime.tlsStore.fingerprint() }.getOrNull() ?: "—",
    identityStatusKey(runtime.identityStore.status()), runtime.carlinkGeometry, runtime.carlinkGeometryMode)
}

@Composable fun DeveloperScreen(state: SettingsEditorState, view: DeveloperView, route: String, back: () -> Unit,
    open: (String) -> Unit, copy: (String) -> Unit, reconnect: () -> Unit,
    sample: Boolean, setSample: (Boolean) -> Unit, openVehicle: () -> Unit, wireless: (RelaySettings) -> Boolean,
    debug: RelayRuntime? = null) {
    val invalidPages = state.errorPages(wireless)
    val draft = state.draft.value
    DetailPage(stringResource(developerPageTitle(route)), back) {
        when (route) {
            "developer" -> {
                Toggle(stringResource(R.string.dev_use_options), state.developerDraft.value.active, { enabled ->
                    state.changeDeveloper { it.copy(active = enabled) }
                })
                Hint(stringResource(R.string.dev_use_options_hint))
                SettingsMenuRow(R.string.design_gallery, stringResource(R.string.design_gallery_hint), R.drawable.symbol_settings_2) { open("developer/design") }
                SettingsMenuRow(R.string.menu_diagnostics, stringResource(R.string.menu_diagnostics_summary, view.rows.size, view.urls.size), R.drawable.symbol_info) { open("developer/diagnostics") }
                SettingsMenuRow(R.string.ui_carlink, "${hevcSummary(draft)} · ${menuToggleSummary(draft.carlinkTrace)}", R.drawable.symbol_car) { open("developer/carlink") }
                SettingsMenuRow(R.string.menu_video, stringResource(R.string.menu_fps_summary, draft.fps), R.drawable.symbol_settings,
                    "developer/video" in invalidPages) { open("developer/video") }
                SettingsMenuRow(R.string.menu_connection_outputs, "${outputSummary(draft)} · ${connectionModeSummary(draft)}", R.drawable.symbol_wifi,
                    "developer/connection" in invalidPages) { open("developer/connection") }
            }
            "developer/diagnostics" -> DeveloperDiagnostics(view, sample, setSample, openVehicle, reconnect, copy,
                enabled = state.developerDraft.value.active)
            "developer/carlink" -> DeveloperCarlink(state, view, debug)
            "developer/video" -> DeveloperVideo(state)
            "developer/connection" -> DeveloperConnection(state, wireless)
        }
        if (state.savingError) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        if (route in invalidPages || (route == "developer" && invalidPages.any { it.startsWith("developer/") }))
            Text(stringResource(R.string.settings_advanced_error), color = MaterialTheme.colorScheme.error)
    }
}

fun developerPageTitle(route: String) = when (route) {
    "developer/diagnostics" -> R.string.menu_diagnostics
    "developer/carlink" -> R.string.ui_carlink
    "developer/video" -> R.string.menu_video
    "developer/connection" -> R.string.menu_connection_outputs
    else -> R.string.ui_developer
}

/** Presentation keys keep protocol enum names and platform status strings out of UI copy. */
fun developerStateKey(value: String): Int = when {
    value == "Disabled" || value == "Web receiver disabled" -> R.string.dev_disabled
    value == "Requires Android 10+" -> R.string.dev_android_required
    value == "Waiting for permissions" -> R.string.dev_permissions
    value == "CONNECTING" || value.startsWith("Starting web") -> R.string.ui_connecting
    value == "AUTHENTICATING" -> R.string.dev_authenticating
    value == "NEGOTIATING" -> R.string.dev_negotiating
    value == "ACTIVE" || value == "PLAYING" -> R.string.dev_active
    value == "RECONNECTING" -> R.string.dev_reconnecting
    value == "ENDED" -> R.string.dev_ended
    value.contains("Receiver assets missing") -> R.string.dev_assets_missing
    value.startsWith("TLS enabled") -> R.string.dev_listening
    value.startsWith("Web receiver unavailable") -> R.string.dev_receiver_failed
    else -> R.string.dev_idle
}
