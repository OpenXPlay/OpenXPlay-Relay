// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.bridge

import com.shilapi.xcertplay.airplay.*
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test
import java.net.InetAddress

class MicrophoneFramesTest {
    private fun config(frameMs: Int = 20, channels: Int = 1) = MicrophoneConfig("speechrecognition", 16000,
        channels, 102, frameMs, InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)), 7000, ByteArray(32))
    private fun body(packet: ByteArray, config: MicrophoneConfig): ByteArray {
        val nonce = ByteArray(12)
        packet.copyInto(nonce, 4, packet.size - 8, packet.size)
        return AirPlayCrypto.chachaOpen(config.key, nonce, packet.copyOfRange(12, packet.size - 8), packet.copyOfRange(4, 12))
    }
    private fun timestamp(packet: ByteArray): Int = java.nio.ByteBuffer.wrap(packet, 4, 4).int
    @Test fun everyTickSendsReceivedPcmOrSilenceWithAuthenticatedRtpCounters() {
        val config = config()
        val frames = MicrophoneFrames(config)
        val packets = mutableListOf<ByteArray>()
        val pcm = ByteArray(640) { (it % 127).toByte() }
        assertFalse(frames.offer(ByteArray(639)))
        assertTrue(frames.offer(pcm))
        pcm.fill(0)
        repeat(3) { frames.tick({ fail("LPCM must not use an encoder") }, packets::add) }
        assertEquals(3, packets.size)
        assertArrayEquals(AudioPolicy.swap(ByteArray(640) { (it % 127).toByte() }), body(packets[0], config))
        for (i in 1..2) assertArrayEquals(ByteArray(640), body(packets[i], config))
        for (i in packets.indices) {
            assertEquals(676, packets[i].size)
            assertEquals(0x80.toByte(), packets[i][0]); assertEquals(102.toByte(), packets[i][1])
            assertEquals(i.toByte(), packets[i][3]); assertEquals(i * 320, timestamp(packets[i]))
            assertEquals(i.toByte(), packets[i][packets[i].size - 8])
        }
    }
    @Test fun ticksRepacketizeNegotiatedDurationsAndExpandMonoToStereo() {
        for (duration in listOf(5, 10, 40, 60)) {
            val config = config(duration, 2)
            val frames = MicrophoneFrames(config)
            val packets = mutableListOf<ByteArray>()
            repeat(6) { frames.tick({ emptyList() }, packets::add) }
            assertEquals(120 / duration, packets.size)
            packets.forEach { assertArrayEquals(ByteArray(config.frameBytes), body(it, config)) }
            for (i in packets.indices) assertEquals(i * config.samplesPerPacket, timestamp(packets[i]))
        }
        val config = config(20, 2)
        val frames = MicrophoneFrames(config)
        frames.offer(ByteArray(640) { if (it % 2 == 0) 1 else 2 })
        val packets = mutableListOf<ByteArray>(); frames.tick({ emptyList() }, packets::add)
        assertArrayEquals(ByteArray(1280) { if (it % 2 == 0) 2 else 1 }, body(packets.single(), config))
    }
    @Test fun opusUsesEncodedBodiesAndNegotiatedRtpClockEvenForSilence() {
        val config = config().copy(codec = AudioCodecKind.OPUS, sampleRate = 48000, opusClockRate = 24000)
        val frames = MicrophoneFrames(config)
        val packets = mutableListOf<ByteArray>()
        repeat(2) { frames.tick({ pcm -> assertArrayEquals(ByteArray(1920), pcm); listOf(hex("f8 ff fe")) }, packets::add) }
        assertEquals(480, timestamp(packets[1]))
        assertArrayEquals(hex("f8 ff fe"), body(packets[0], config))
    }
}
