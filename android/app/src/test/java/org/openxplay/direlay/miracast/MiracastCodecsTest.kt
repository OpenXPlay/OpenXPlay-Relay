// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.miracast

import android.app.Application
import android.media.MediaCodecInfo.CodecProfileLevel.*
import android.media.MediaFormat
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.stream.core.*
import org.openxplay.stream.core.codec.*
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.nio.ByteBuffer

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class MiracastCodecsTest {
    @Test fun avcProfilesAndLevelsBecomeCodecStringsAtTheirBestAdvertisedLevel() {
        assertEquals(listOf(DecoderSupport("avc1.64002a", true), DecoderSupport("avc1.42e01f", true)),
            MiracastCodecs.supports("video/avc", listOf(AVCProfileHigh to AVCLevel31,
                AVCProfileHigh to AVCLevel42, AVCProfileConstrainedBaseline to AVCLevel31)))
        assertEquals("avc1.4d0034", MiracastCodecs.codecString("video/avc", AVCProfileMain, AVCLevel52))
        assertEquals("avc1.42100b", MiracastCodecs.codecString("video/avc", AVCProfileBaseline, AVCLevel1b))
        assertEquals("avc1.64003e", MiracastCodecs.codecString("video/avc", AVCProfileHigh, AVCLevel62))
        assertNull(MiracastCodecs.codecString("video/avc", -1, AVCLevel31))
        assertNull(MiracastCodecs.codecString("video/avc", AVCProfileHigh, 0))
        assertNull(MiracastCodecs.codecString("video/avc", AVCProfileHigh, 3))
    }
    @Test fun hevcProfilesPreserveTierAndLevelAndUnknownCodecsAreExcluded() {
        assertEquals("hvc1.1.6.L153", MiracastCodecs.codecString("video/hevc", HEVCProfileMain, HEVCMainTierLevel51))
        assertEquals("hvc1.2.4.H186", MiracastCodecs.codecString("video/hevc", HEVCProfileMain10, HEVCHighTierLevel62))
        assertEquals("hvc1.2.4.L153", MiracastCodecs.codecString("video/hevc", HEVCProfileMain10HDR10, HEVCMainTierLevel51))
        assertEquals("hvc1.3.8.L30", MiracastCodecs.codecString("video/hevc", HEVCProfileMainStill, HEVCMainTierLevel1))
        assertNull(MiracastCodecs.codecString("video/av1", HEVCProfileMain, HEVCMainTierLevel1))
        assertNull(MiracastCodecs.codecString("video/hevc", -1, HEVCMainTierLevel1))
    }
    private fun bytes(buffer: ByteBuffer): ByteArray = buffer.duplicate().let { copy -> ByteArray(copy.remaining()).also { copy.get(it) } }
    private fun annexB(nals: List<ByteArray>) = nals.fold(ByteArray(0)) { data, nal -> data + byteArrayOf(0, 0, 0, 1) + nal }
    @Test fun csdContainsAnnexBSpsPpsForAvcAndCombinedVpsSpsPpsForHevc() {
        val avc = AvcConfig.parse(avcBytes)
        val format = MiracastCodecs.configuration(avcFormat())
        assertArrayEquals(annexB(avc.sps), bytes(format.getByteBuffer("csd-0")!!))
        assertArrayEquals(annexB(avc.pps), bytes(format.getByteBuffer("csd-1")!!))
        val hevc = HevcConfig.parse(hevcBytes)
        val h265 = MiracastCodecs.configuration(VideoFormat.create(ScreenId.MAIN, VideoCodec.H265, hevcBytes))
        assertArrayEquals(annexB(hevc.vps + hevc.sps + hevc.pps), bytes(h265.getByteBuffer("csd-0")!!))
        assertFalse(h265.containsKey("csd-1"))
    }
    @Test fun lowLatencyIsEnabledOnlyWhenSupported() {
        for (supported in listOf(false, true)) {
            var attempts = 0
            MiracastCodecs.configure(avcFormat(), supported) { format ->
                attempts++
                assertEquals(supported, format.containsKey(MediaFormat.KEY_LOW_LATENCY))
                if (supported) assertEquals(1, format.getInteger(MediaFormat.KEY_LOW_LATENCY))
            }
            assertEquals(1, attempts)
        }
    }
    @Test fun rejectedLowLatencyHintsRetryWithFreshConfigurationWithoutTheHint() {
        val hints = mutableListOf<Boolean>()
        val result = MiracastCodecs.configure(avcFormat(), true) { format ->
            val hinted = format.containsKey(MediaFormat.KEY_LOW_LATENCY)
            hints += hinted
            assertTrue(format.containsKey("csd-0"))
            assertTrue(format.containsKey("csd-1"))
            if (hinted) throw IllegalArgumentException("optional hint rejected")
            "configured"
        }
        assertEquals("configured", result)
        assertEquals(listOf(true, false), hints)
    }
    @Config(sdk = [29])
    @Test fun lowLatencyHintsAreOmittedBeforeAndroid30() {
        MiracastCodecs.configure(avcFormat(), true) { assertFalse(it.containsKey("low-latency")) }
    }
    @Test fun failureWithoutTheHintIsPropagated() {
        val hints = mutableListOf<Boolean>()
        val failure = assertThrows(IllegalStateException::class.java) {
            MiracastCodecs.configure(avcFormat(), true) { format ->
                val hinted = format.containsKey(MediaFormat.KEY_LOW_LATENCY)
                hints += hinted
                throw IllegalStateException(if (hinted) "hint rejected" else "decoder unavailable")
            }
        }
        assertEquals(listOf(true, false), hints)
        assertEquals("decoder unavailable", failure.message)
    }
}
