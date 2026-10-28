// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.view.Surface
import org.openxplay.stream.core.*
import org.openxplay.stream.core.codec.AvcConfig
import org.openxplay.stream.core.codec.HevcConfig
import java.nio.ByteBuffer
import java.util.Locale

internal interface MiracastCodec : AutoCloseable {
    val name: String
    fun offer(annexB: ByteArray, ptsUs: Long): Boolean
    fun drain(): Int
    fun interface Factory { fun create(format: VideoFormat, surface: Surface): MiracastCodec }
}

internal object MiracastCodecs {
    private val avcLevels = listOf(10, 9, 11, 12, 13, 20, 21, 22, 30, 31, 32, 40, 41, 42, 50, 51, 52, 60, 61, 62)
    private val hevcLevels = listOf(30, 60, 63, 90, 93, 120, 123, 150, 153, 156, 180, 183, 186)
    fun mime(codec: VideoCodec): String = if (codec == VideoCodec.H264) "video/avc" else "video/hevc"

    fun hardware(info: MediaCodecInfo): Boolean = !info.isEncoder &&
        if (Build.VERSION.SDK_INT >= 29) info.isHardwareAccelerated
        else !info.name.lowercase(Locale.ROOT).let { it.startsWith("omx.google.") || it.startsWith("c2.android.") || it.startsWith("omx.ffmpeg.") }

    fun codecString(mime: String, profile: Int, level: Int): String? {
        if (level <= 0 || Integer.bitCount(level) != 1) return null
        val bit = Integer.numberOfTrailingZeros(level)
        if (mime.equals("video/avc", true)) {
            val pair = when (profile) {
                MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline -> 66 to 0
                MediaCodecInfo.CodecProfileLevel.AVCProfileConstrainedBaseline -> 66 to 0xe0
                MediaCodecInfo.CodecProfileLevel.AVCProfileMain -> 77 to 0
                MediaCodecInfo.CodecProfileLevel.AVCProfileExtended -> 88 to 0
                MediaCodecInfo.CodecProfileLevel.AVCProfileHigh -> 100 to 0
                MediaCodecInfo.CodecProfileLevel.AVCProfileConstrainedHigh -> 100 to 0x0c
                MediaCodecInfo.CodecProfileLevel.AVCProfileHigh10 -> 110 to 0
                MediaCodecInfo.CodecProfileLevel.AVCProfileHigh422 -> 122 to 0
                MediaCodecInfo.CodecProfileLevel.AVCProfileHigh444 -> 244 to 0
                else -> return null
            }
            val value = avcLevels.getOrNull(bit) ?: return null
            // Level 1b is encoded as level_idc 11 with constraint_set3_flag.
            return String.format(Locale.ROOT, "avc1.%02x%02x%02x", pair.first,
                pair.second or if (value == 9) 0x10 else 0, if (value == 9) 11 else value)
        }
        if (!mime.equals("video/hevc", true)) return null
        val value = hevcLevels.getOrNull(bit / 2) ?: return null
        val profileId = when (profile) {
            MediaCodecInfo.CodecProfileLevel.HEVCProfileMain -> 1
            MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10,
            MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10,
            MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10Plus -> 2
            MediaCodecInfo.CodecProfileLevel.HEVCProfileMainStill -> 3
            else -> return null
        }
        // Codec strings reverse general_profile_compatibility_flags; Main sets flags 1 and 2.
        val compatibility = if (profileId == 1) 6 else 1 shl profileId
        return "hvc1.$profileId.${compatibility.toString(16)}.${if (bit % 2 == 0) 'L' else 'H'}$value"
    }

    fun supports(mime: String, profiles: List<Pair<Int, Int>>): List<DecoderSupport> = profiles
        .groupBy { it.first }.values.mapNotNull { levels ->
            val best = levels.maxByOrNull { it.second } ?: return@mapNotNull null
            codecString(mime, best.first, best.second)?.let { DecoderSupport(it, true) }
        }.distinct()

    fun discover(): List<DecoderSupport> = runCatching {
        MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter(::hardware).flatMap { info ->
            info.supportedTypes.filter { it.equals("video/avc", true) || it.equals("video/hevc", true) }.flatMap { type ->
                runCatching { supports(type, info.getCapabilitiesForType(type).profileLevels.map { it.profile to it.level }) }
                    .getOrDefault(emptyList())
            }
        }.distinct()
    }.getOrDefault(emptyList())

    fun configuration(format: VideoFormat): MediaFormat = MediaFormat.createVideoFormat(mime(format.codec), format.width, format.height).apply {
        val sets = when (format.codec) {
            VideoCodec.H264 -> AvcConfig.parse(format.config).let { config ->
                val profile = when (config.profile) {
                    66 -> MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline
                    77 -> MediaCodecInfo.CodecProfileLevel.AVCProfileMain
                    88 -> MediaCodecInfo.CodecProfileLevel.AVCProfileExtended
                    100 -> MediaCodecInfo.CodecProfileLevel.AVCProfileHigh
                    110 -> MediaCodecInfo.CodecProfileLevel.AVCProfileHigh10
                    122 -> MediaCodecInfo.CodecProfileLevel.AVCProfileHigh422
                    244 -> MediaCodecInfo.CodecProfileLevel.AVCProfileHigh444
                    else -> null
                }
                profile?.let { setInteger(MediaFormat.KEY_PROFILE, it) }
                val level = if (config.level == 11 && config.compatibility and 0x10 != 0) 9 else config.level
                avcLevels.indexOf(level).takeIf { it >= 0 }?.let { setInteger(MediaFormat.KEY_LEVEL, 1 shl it) }
                listOf(config.sps, config.pps)
            }
            VideoCodec.H265 -> HevcConfig.parse(format.config).let { config ->
                val profile = when (config.profile) {
                    1 -> MediaCodecInfo.CodecProfileLevel.HEVCProfileMain
                    2 -> MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10
                    3 -> MediaCodecInfo.CodecProfileLevel.HEVCProfileMainStill
                    else -> null
                }
                profile?.let { setInteger(MediaFormat.KEY_PROFILE, it) }
                hevcLevels.indexOf(config.level).takeIf { it >= 0 }?.let {
                    setInteger(MediaFormat.KEY_LEVEL, 1 shl (it * 2 + if (config.highTier) 1 else 0))
                }
                listOf(config.vps + config.sps + config.pps)
            }
        }
        sets.forEachIndexed { index, nals ->
            val size = nals.sumOf { 4 + it.size }
            val bytes = ByteBuffer.allocate(size)
            nals.forEach { bytes.putInt(1); bytes.put(it) }
            bytes.flip()
            setByteBuffer("csd-$index", bytes)
        }
    }

    /** Vendor feature declarations can be wrong; optional latency hints must not prevent decoding. */
    internal fun <T> configure(format: VideoFormat, lowLatency: Boolean, create: (MediaFormat) -> T): T {
        if (Build.VERSION.SDK_INT >= 30 && lowLatency) {
            try { return create(configuration(format).apply { setInteger(MediaFormat.KEY_LOW_LATENCY, 1) }) }
            catch (_: Exception) { }
        }
        return create(configuration(format))
    }

    val factory = MiracastCodec.Factory { format, surface ->
        val mediaFormat = configuration(format)
        val info = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.firstOrNull { info ->
            hardware(info) && info.supportedTypes.any { it.equals(mime(format.codec), true) } &&
                runCatching { info.getCapabilitiesForType(mime(format.codec)).isFormatSupported(mediaFormat) }.getOrDefault(false)
        } ?: error("hardware_decoder_unavailable")
        val lowLatency = Build.VERSION.SDK_INT >= 30 && runCatching {
            info.getCapabilitiesForType(mime(format.codec)).isFeatureSupported(MediaCodecInfo.CodecCapabilities.FEATURE_LowLatency)
        }.getOrDefault(false)
        configure(format, lowLatency) { configuration ->
            val codec = MediaCodec.createByCodecName(info.name)
            try {
                codec.configure(configuration, surface, null, 0)
                codec.start()
                AndroidMiracastCodec(codec)
            } catch (failure: Exception) { runCatching { codec.release() }; throw failure }
        }
    }
}

private class AndroidMiracastCodec(private val codec: MediaCodec) : MiracastCodec {
    override val name: String get() = codec.name
    private val info = MediaCodec.BufferInfo()
    override fun offer(annexB: ByteArray, ptsUs: Long): Boolean {
        val index = codec.dequeueInputBuffer(0)
        if (index < 0) return false
        val buffer = checkNotNull(codec.getInputBuffer(index))
        require(buffer.capacity() >= annexB.size) { "decoder_input_too_large" }
        buffer.clear(); buffer.put(annexB)
        codec.queueInputBuffer(index, 0, annexB.size, ptsUs, 0)
        return true
    }
    override fun drain(): Int {
        var count = 0
        repeat(16) {
            val index = codec.dequeueOutputBuffer(info, 0)
            if (index >= 0) {
                val render = info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0
                codec.releaseOutputBuffer(index, render)
                if (render) count++
            } else if (index != MediaCodec.INFO_OUTPUT_FORMAT_CHANGED && index != MediaCodec.INFO_OUTPUT_BUFFERS_CHANGED) return count
        }
        return count
    }
    override fun close() {
        runCatching { codec.stop() }
        runCatching { codec.release() }
    }
}
