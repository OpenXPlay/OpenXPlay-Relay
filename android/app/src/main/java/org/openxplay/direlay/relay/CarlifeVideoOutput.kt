// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-02
package org.openxplay.direlay.relay

import org.openxplay.direlay.video.MediaCodecPipeline
import org.openxplay.direlay.video.TranscodingOutput
import org.openxplay.direlay.video.VideoPipeline
import org.openxplay.stream.core.*

/** Relay-owned codec and geometry adaptation; the destination receives only OXP media and controls. */
internal fun carlifeVideoOutput(
    destination: ProjectionOutput,
    factory: VideoPipeline.Factory = MediaCodecPipeline.factory,
    diagnostic: (String, String) -> Unit = { _, _ -> },
): ProjectionOutput {
    val formats = mutableMapOf<ScreenId, String>()
    val delivered = object : ProjectionOutput by destination {
        override fun onVideoFormat(screen: ScreenId, format: VideoFormat?) {
            val key = format?.let { "codec=${it.codec} size=${it.width}x${it.height} fps=${it.fpsHint ?: "-"} profile=${it.codecString}" } ?: "none"
            synchronized(formats) {
                if (formats.put(screen, key) != key) diagnostic("delivered_format", "screen=${screen.value} $key")
            }
            destination.onVideoFormat(screen, format)
        }
    }
    var capabilitiesKey: String? = null
    val capabilitiesLock = Any()
    val converted = TranscodingOutput(delivered, factory,
        deliveryCodecs = setOf(VideoCodec.H264), carlinkGeometry = true,
        report = { source, decision, failed ->
            val target = decision.target
            val geometry = target?.geometry
            diagnostic("video_decision", "screen=${source.screen.value} source=${source.codec} " +
                "sourceSize=${source.width}x${source.height} sourceFps=${source.fpsHint ?: "-"} sourceProfile=${source.codecString} " +
                "decision=${decision.mode} failed=$failed target=${target?.codec ?: "-"} targetProfile=${target?.codecString ?: "-"} " +
                "profile=${target?.profile ?: "-"} level=${target?.level ?: "-"} " +
                "canvas=${geometry?.canvas?.let { "${it.width}x${it.height}" } ?: "-"} " +
                "crop=${geometry?.area?.let { "${it.x},${it.y},${it.width},${it.height}" } ?: "-"} " +
                "picture=${geometry?.picture?.let { "${it.width}x${it.height}" } ?: "-"} identity=${geometry?.identity ?: "-"}")
        },
        observeCapabilities = { caps ->
            val main = caps?.displays?.get(ScreenId.MAIN)
            val key = if (caps == null) "none" else "decoders=${caps.decoders.joinToString(",") { it.codecString }} " +
                "screen=${main?.let { "${it.widthPx}x${it.heightPx}" } ?: "-"} refresh=${main?.refreshHz ?: "-"} " +
                "picture=${main?.frameSize?.let { "${it.width}x${it.height}" } ?: "-"} touch=${caps.touch}"
            synchronized(capabilitiesLock) {
                if (capabilitiesKey != key) {
                    capabilitiesKey = key
                    diagnostic("capabilities", key)
                }
            }
        },
        onFailure = { format, failure ->
            diagnostic("video_failed", "screen=${format.screen.value} source=${format.codec} size=${format.width}x${format.height} " +
                "error=${failure.javaClass.simpleName} cause=${failure.cause?.javaClass?.simpleName ?: "-"} " +
                "detail=${failure.message.orEmpty().filterNot(Character::isISOControl).take(240)}")
        })
    var sessionKey: Pair<SessionState, SessionConfig>? = null
    return object : ProjectionOutput by converted {
        override fun onSession(state: SessionState, config: SessionConfig) {
            synchronized(this) {
                val key = state to config
                if (sessionKey != key) {
                    sessionKey = key
                    val main = config.screens[ScreenId.MAIN]
                    diagnostic("oxp_session", "session=${state.id.value} phase=${state.phase} " +
                        "screen=0 codec=${main?.codec ?: "-"} size=${main?.let { "${it.width}x${it.height}" } ?: "-"} fps=${main?.fps ?: "-"}")
                }
            }
            converted.onSession(state, config)
        }
    }
}
