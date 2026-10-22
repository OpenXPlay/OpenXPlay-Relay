// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.video

import org.openxplay.stream.core.*

/** Receiver limits stay separate from Android codec availability. */
object VideoCompatibility {
    enum class Mode { PASSTHROUGH, TRANSCODE, UNSUPPORTED }
    data class Target(val codec: VideoCodec, val profile: Int, val level: Int, val codecString: String, val geometry: CarlinkGeometry? = null)
    data class Decision(val mode: Mode, val target: Target? = null)

    internal data class ReportKey(val codec: VideoCodec, val profile: String, val width: Int,
        val height: Int, val decision: Decision, val reason: String)
    internal fun reportKey(format: VideoFormat, decision: Decision, failed: Boolean) =
        ReportKey(format.codec, format.codecString, format.width, format.height, decision,
            if (failed) "pipeline_failed" else when (decision.mode) {
                Mode.PASSTHROUGH -> "receiver_accepts"
                Mode.TRANSCODE -> if (decision.target?.geometry?.identity == false) "carlink_geometry" else "receiver_profile_mismatch"
                Mode.UNSUPPORTED -> "no_supported_target"
            })

    private fun avc(value: String): Target? {
        val match = Regex("avc1\\.([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})").matchEntire(value) ?: return null
        return Target(VideoCodec.H264, match.groupValues[1].toInt(16), match.groupValues[3].toInt(16), value)
    }
    private fun hevc(value: String): List<String>? = value.split('.').takeIf {
        it.size >= 4 && it[0] in setOf("hvc1", "hev1") && it[1].toIntOrNull() != null &&
            it[2].toLongOrNull(16) != null && it[3].matches(Regex("[LH][0-9]+")) &&
            it[3].drop(1).toIntOrNull() != null
    }
    fun hasHevc(decoders: List<DecoderSupport>): Boolean = decoders.any { hevc(it.codecString) != null }

    fun accepts(input: VideoFormat, decoder: DecoderSupport): Boolean {
        if (input.codec == VideoCodec.H264) {
            val source = avc(input.codecString) ?: return false
            val target = avc(decoder.codecString) ?: return false
            // Match ClinkOutput: High receivers also accept Baseline streams.
            return (target.profile == source.profile || target.profile == 100 && source.profile == 66) &&
                target.level >= source.level
        }
        val source = hevc(input.codecString) ?: return false
        val target = hevc(decoder.codecString) ?: return false
        val profile = source[1].toInt()
        val compatible = target[1] == source[1] ||
            (profile in 0..31 && target[2].toLong(16) and (1L shl profile) != 0L)
        return compatible && (target[3][0] == 'H' || source[3][0] == 'L') &&
            target[3].drop(1).toInt() >= source[3].drop(1).toInt()
    }

    fun decide(input: VideoFormat, decoders: List<DecoderSupport>, geometry: CarlinkGeometry? = null): Decision {
        if (geometry?.identity != false && decoders.any { accepts(input, it) }) return Decision(Mode.PASSTHROUGH)
        if (geometry?.identity == false && input.codec == VideoCodec.H265) decoders.firstNotNullOfOrNull { decoder ->
            val h = hevc(decoder.codecString)
            h?.takeIf { it[1] == "1" && it[3][0] == 'L' && it[3].drop(1).toInt() in hevcLevels }?.let {
                Target(VideoCodec.H265, 1, it[3].drop(1).toInt(), decoder.codecString, geometry)
            }
        }?.let { return Decision(Mode.TRANSCODE, it.copy(geometry = geometry)) }
        val avcTargets = decoders.mapNotNull { avc(it.codecString) }
        for (profile in listOf(66, 77, 100)) {
            avcTargets.firstOrNull { it.profile == profile && it.level in avcLevels }?.let {
                return Decision(Mode.TRANSCODE, it.copy(geometry = geometry))
            }
        }
        decoders.forEach { decoder ->
            val h = hevc(decoder.codecString)
            if (h != null && h[1] == "1" && h[3][0] == 'L' && h[3].drop(1).toInt() in hevcLevels) {
                return Decision(Mode.TRANSCODE, Target(VideoCodec.H265, 1, h[3].drop(1).toInt(), decoder.codecString, geometry))
            }
        }
        return Decision(Mode.UNSUPPORTED)
    }
    private val avcLevels = setOf(10, 11, 12, 13, 20, 21, 22, 30, 31, 32, 40, 41, 42, 50, 51, 52, 60, 61, 62)
    private val hevcLevels = setOf(30, 60, 63, 90, 93, 120, 123, 150, 153, 156, 180, 183, 186)
}

/** Unknown capabilities and an empty receiver set must keep the iPhone on H.264. */
fun requestHevc(outputs: List<OutputCapabilities>, requiresAvc: Boolean = false): Boolean =
    VideoCodecPolicy.resolve(outputs, requiresAvc).codec == VideoCodec.H265
