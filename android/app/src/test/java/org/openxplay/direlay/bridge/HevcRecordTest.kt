// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18
package org.openxplay.direlay.bridge

import com.shilapi.xcertplay.airplay.*
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import org.openxplay.stream.core.codec.HevcConfig
import org.openxplay.stream.core.VideoCodec as StreamCodec

class HevcRecordTest {
    /** What DiPlay passed for an iPhone's H.265 stream (2026-10-18): the hvcC record, then a colr box and padding. */
    private val iphoneConfig = hex("""
    01 01 60 00 00 00 b0 00 00 00 00 00 7b f0 00 fc fd f8 f8 00 00 0b 03 a0 00 01 00 18 40 01 0c 01
    ff ff 01 60 00 00 03 00 b0 00 00 03 00 00 03 00 7b 08 c0 90 a1 00 01 00 30 42 01 01 01 60 00 00
    03 00 b0 00 00 03 00 00 03 00 7b a0 03 c0 80 36 16 20 23 92 45 21 0b 9f c4 fc 2f e8 6f d4 3f ea
    82 3f 55 29 b8 08 68 08 04 a2 00 01 00 07 44 01 c0 72 4c 16 c9 00 00 00 12 63 6f 6c 72 6e 63 6c
    63 00 01 00 0d 00 01 00 00 00 00
    """)

    @Test fun boxesAfterTheHvccRecordAreDropped() {
        val record = hevcRecord(iphoneConfig)
        assertEquals(117, record.size)
        assertArrayEquals(iphoneConfig.copyOf(117), record)
        val parsed = HevcConfig.parse(record)
        assertEquals(1920, parsed.width)
        assertEquals(864, parsed.height)
    }

    @Test fun aBareRecordOrSomethingElseIsLeftAsItIs() {
        assertArrayEquals(hevcConfig, hevcRecord(hevcConfig))
        val notARecord = hex("01 02 03")
        assertArrayEquals(notARecord, hevcRecord(notARecord))
        // A record whose arrays run past the end is not trimmed; the parser reports it.
        val truncated = iphoneConfig.copyOf(60)
        assertArrayEquals(truncated, hevcRecord(truncated))
    }

    @Test fun theIphoneConfigurationBecomesAnH265Format() {
        DiPlayInput().use { input ->
            val capture = CapturingInputSink()
            input.start(configuration, capture)
            val bridge = StreamBridgeSink(input, object : MediaSink {}, SessionControl { true })
            bridge.onVideoCodec(110, VideoCodec.H265)
            bridge.onVideoConfig(110, iphoneConfig)
            val format = capture.formats.mapNotNull { it.second }.single()
            assertEquals(StreamCodec.H265, format.codec)
            assertEquals(1920, format.width)
        }
    }
}
