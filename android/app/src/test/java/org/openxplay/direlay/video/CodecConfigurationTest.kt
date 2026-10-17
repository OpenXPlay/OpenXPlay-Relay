// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.video

import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.direlay.bridge.avcConfig
import org.openxplay.direlay.bridge.hevcConfig
import org.openxplay.stream.core.*
import org.openxplay.stream.core.codec.*

class CodecConfigurationTest {
    @Test fun encoderParameterSetsProduceParseableConfigurationsForBothCodecs() {
        val avc = AvcConfig.parse(avcConfig)
        val hevc = HevcConfig.parse(hevcConfig)
        for ((codec, nals) in listOf(VideoCodec.H264 to (avc.sps + avc.pps), VideoCodec.H265 to (hevc.vps + hevc.sps + hevc.pps))) {
            val config = CodecConfiguration.build(codec, nals)
            val format = VideoFormat.create(ScreenId.MAIN, codec, config)
            val source = VideoFormat.create(ScreenId.MAIN, codec, if (codec == VideoCodec.H264) avcConfig else hevcConfig)
            assertEquals(source.codecString, format.codecString)
            assertEquals(source.width, format.width)
            assertEquals(source.height, format.height)
            val annexB = CodecConfiguration.annexB(nals)
            val avcc = CodecConfiguration.accessUnit(annexB)
            assertArrayEquals(annexB, Nal.avccToAnnexB(avcc))
            assertArrayEquals(avcc, CodecConfiguration.accessUnit(avcc))
        }
    }
}
