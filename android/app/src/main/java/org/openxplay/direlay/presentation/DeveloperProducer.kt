// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import org.openxplay.direlay.R
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.ui.*

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

