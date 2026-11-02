// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.bridge

import android.media.MediaFormat
import java.nio.ByteBuffer

/** Worker-owned codecs; tests substitute PCM for the encoded access units. */
internal interface CastAudioCodec : AutoCloseable {
    fun offer(data: ByteArray, ptsUs: Long)
    fun pump(output: (ByteArray, Long) -> Unit)
    interface Factory {
        fun decoder(): CastAudioCodec
        fun encoder(): CastAudioCodec
    }
}

internal object CastAudioCodecs : CastAudioCodec.Factory {
    override fun decoder(): CastAudioCodec = adapt(RelayAudioCodec(
        MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_AAC, 48_000, 2).apply {
            // Stream supplies raw AAC, unlike the ADTS input of the CarPlay-side decoder helper.
            setInteger(MediaFormat.KEY_IS_ADTS, 0)
            setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 64 * 1024)
            setInteger(MediaFormat.KEY_PCM_ENCODING, android.media.AudioFormat.ENCODING_PCM_16BIT)
            setByteBuffer("csd-0", ByteBuffer.wrap(byteArrayOf(0x11, 0x90.toByte())))
        }, false), decoded = true)
    override fun encoder(): CastAudioCodec = adapt(RelayAudioCodec(
        RelayAudioCodec.encoder(MediaFormat.MIMETYPE_AUDIO_AAC, 48_000, 2, 192_000), true), decoded = false)

    private fun adapt(codec: RelayAudioCodec, decoded: Boolean): CastAudioCodec = object : CastAudioCodec {
        override fun offer(data: ByteArray, ptsUs: Long) = codec.offer(data, ptsUs)
        override fun pump(output: (ByteArray, Long) -> Unit) = codec.pump(output) { actual ->
            if (decoded) {
                require(actual.getInteger(MediaFormat.KEY_SAMPLE_RATE) == 48_000 &&
                    actual.getInteger(MediaFormat.KEY_CHANNEL_COUNT) == 2) { "Unexpected cast decoded audio format" }
                if (actual.containsKey(MediaFormat.KEY_PCM_ENCODING)) require(
                    actual.getInteger(MediaFormat.KEY_PCM_ENCODING) == android.media.AudioFormat.ENCODING_PCM_16BIT)
            }
        }
        override fun close() = codec.close()
    }
}
