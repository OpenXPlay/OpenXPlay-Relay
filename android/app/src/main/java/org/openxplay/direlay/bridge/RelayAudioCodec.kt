// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.bridge

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.util.Log
import com.shilapi.xcertplay.airplay.AudioCodecKind
import com.shilapi.xcertplay.airplay.AudioFormat as CarPlayAudioFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.ArrayDeque

/** Used only by a media worker; codecs and their pending input never escape it. */
internal class RelayAudioCodec(format: MediaFormat, encode: Boolean) : AutoCloseable {
    private val codec = if (encode) MediaCodec.createEncoderByType(format.getString(MediaFormat.KEY_MIME)!!)
        else MediaCodec.createDecoderByType(format.getString(MediaFormat.KEY_MIME)!!)
    private data class Frame(val data: ByteArray, val pts: Long)
    private val pending = ArrayDeque<Frame>()
    private val info = MediaCodec.BufferInfo()
    init {
        try {
            codec.configure(format, null, null, if (encode) MediaCodec.CONFIGURE_FLAG_ENCODE else 0)
            codec.start()
        } catch (failure: Exception) { codec.release(); throw failure }
    }
    fun offer(data: ByteArray, pts: Long) {
        if (data.isEmpty()) return
        if (pending.size == 32) { pending.removeFirst(); Log.w("DiRelayAudio", "Codec queue full; dropped oldest input") }
        pending.addLast(Frame(data, pts))
    }
    fun pump(output: (ByteArray, Long) -> Unit, changed: (MediaFormat) -> Unit = {}) {
        drain(output, changed)
        while (pending.isNotEmpty()) {
            val index = codec.dequeueInputBuffer(0)
            if (index < 0) break
            val frame = pending.removeFirst()
            val buffer = requireNotNull(codec.getInputBuffer(index)).apply { clear() }
            require(frame.data.size <= buffer.remaining()) { "Audio access unit exceeds codec input buffer" }
            buffer.put(frame.data)
            codec.queueInputBuffer(index, 0, frame.data.size, frame.pts, 0)
        }
        drain(output, changed)
    }
    private fun drain(output: (ByteArray, Long) -> Unit, changed: (MediaFormat) -> Unit) {
        while (true) {
            val index = codec.dequeueOutputBuffer(info, 0)
            if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) { changed(codec.outputFormat); continue }
            if (index < 0) return
            try {
                if (info.size > 0 && info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG == 0) {
                    val buffer = requireNotNull(codec.getOutputBuffer(index))
                    buffer.position(info.offset); buffer.limit(info.offset + info.size)
                    val data = ByteArray(info.size); buffer.get(data)
                    output(data, info.presentationTimeUs)
                }
            } finally { codec.releaseOutputBuffer(index, false) }
        }
    }
    override fun close() { try { codec.stop() } finally { codec.release() } }

    companion object {
        fun encoder(mime: String, rate: Int, channels: Int, bitrate: Int): MediaFormat =
            MediaFormat.createAudioFormat(mime, rate, channels).apply {
                setInteger(MediaFormat.KEY_BIT_RATE, bitrate)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 64 * 1024)
                if (mime == MediaFormat.MIMETYPE_AUDIO_AAC)
                    setInteger(MediaFormat.KEY_AAC_PROFILE, MediaCodecInfo.CodecProfileLevel.AACObjectLC)
            }
        private fun frequency(rate: Int): Int = listOf(96000, 88200, 64000, 48000, 44100, 32000,
            24000, 22050, 16000, 12000, 11025, 8000, 7350).indexOf(rate).also { require(it >= 0) }
        fun decoder(source: CarPlayAudioFormat): MediaFormat {
            val mime = if (source.codec == AudioCodecKind.AAC_LC) MediaFormat.MIMETYPE_AUDIO_AAC else MediaFormat.MIMETYPE_AUDIO_OPUS
            return MediaFormat.createAudioFormat(mime, source.sampleRate, source.channels).apply {
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 64 * 1024)
                setInteger(MediaFormat.KEY_PCM_ENCODING, android.media.AudioFormat.ENCODING_PCM_16BIT)
                if (source.codec == AudioCodecKind.AAC_LC) {
                    val asc = (2 shl 11) or (frequency(source.sampleRate) shl 7) or (source.channels shl 3)
                    setInteger(MediaFormat.KEY_IS_ADTS, 1)
                    setByteBuffer("csd-0", ByteBuffer.wrap(byteArrayOf((asc ushr 8).toByte(), asc.toByte())))
                } else {
                    val head = ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN)
                    head.put("OpusHead".toByteArray(Charsets.US_ASCII)).put(1.toByte()).put(source.channels.toByte())
                        .putShort(312.toShort()).putInt(source.sampleRate).putShort(0.toShort()).put(0.toByte())
                    setByteBuffer("csd-0", ByteBuffer.wrap(head.array()))
                    setByteBuffer("csd-1", ByteBuffer.wrap(ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(6_500_000).array()))
                    setByteBuffer("csd-2", ByteBuffer.wrap(ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(80_000_000).array()))
                }
            }
        }
        fun adts(data: ByteArray, source: CarPlayAudioFormat): ByteArray {
            val length = data.size + 7
            require(length < 8192)
            val f = frequency(source.sampleRate)
            val c = source.channels
            return byteArrayOf(0xff.toByte(), 0xf1.toByte(), ((1 shl 6) or (f shl 2) or (c ushr 2)).toByte(),
                (((c and 3) shl 6) or (length ushr 11)).toByte(), (length ushr 3).toByte(),
                (((length and 7) shl 5) or 31).toByte(), 0xfc.toByte()) + data
        }
    }
}
