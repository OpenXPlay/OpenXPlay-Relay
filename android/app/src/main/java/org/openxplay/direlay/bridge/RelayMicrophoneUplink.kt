// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.bridge

import android.media.MediaFormat
import android.os.Build
import android.util.Log
import com.shilapi.xcertplay.airplay.*
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.locks.LockSupport
import org.concentus.OpusApplication
import org.concentus.OpusSignal

/** Queue and packet framing are independent of the scheduler, socket and encoder. */
internal class MicrophoneFrames(private val config: MicrophoneConfig) {
    private val queue = ArrayBlockingQueue<ByteArray>(5)
    private val inputBytes = config.sampleRate / 50 * 2
    private var pending = ByteArray(0)
    private val counters = MicrophoneCounters()
    fun offer(pcm: ByteArray): Boolean {
        if (pcm.size != inputBytes) return false
        val copy = pcm.copyOf()
        if (!queue.offer(copy)) {
            queue.poll(); queue.offer(copy)
            Log.w("DiRelayMic", "Microphone queue full; dropped oldest frame")
        }
        return true
    }
    fun tick(encode: (ByteArray) -> List<ByteArray>, send: (ByteArray) -> Unit) {
        val mono = queue.poll() ?: ByteArray(inputBytes)
        val interleaved = ByteArray(mono.size * config.channels)
        for (sample in 0 until mono.size / 2) for (channel in 0 until config.channels) {
            val offset = (sample * config.channels + channel) * 2
            interleaved[offset] = mono[sample * 2]; interleaved[offset + 1] = mono[sample * 2 + 1]
        }
        pending += interleaved
        while (pending.size >= config.frameBytes) {
            val frame = pending.copyOfRange(0, config.frameBytes)
            pending = pending.copyOfRange(config.frameBytes, pending.size)
            val bodies = if (config.codec == AudioCodecKind.OPUS) encode(frame)
                else listOf(MicrophonePacketizer.toWirePcm(frame))
            for (body in bodies) seal(body, send)
        }
    }
    fun seal(body: ByteArray, send: (ByteArray) -> Unit) {
        send(MicrophonePacketizer.sealPacket(config.key, config.payloadType, counters, body, config.rtpSamplesPerPacket))
    }
}

internal interface MicrophoneUplink : AutoCloseable {
    fun start()
    fun offer(pcm: ByteArray): Boolean
}

/** Runs continuously in 20 ms periods, even before the head unit supplies its first frame. */
internal class RelayMicrophoneUplink(private val config: MicrophoneConfig, private val failed: () -> Unit) : MicrophoneUplink {
    private val running = AtomicBoolean(true)
    private val frames = MicrophoneFrames(config)
    private val worker = Thread({ run() }, "direlay-microphone").apply { isDaemon = true }
    override fun start() = worker.start()
    override fun offer(pcm: ByteArray): Boolean = running.get() && frames.offer(pcm)
    private fun run() {
        var platform: RelayAudioCodec? = null
        var socket: DatagramSocket? = null
        try {
            require(config.codec in setOf(AudioCodecKind.LPCM, AudioCodecKind.OPUS))
            require(config.channels in 1..2 && config.frameBytes > 0)
            var software: org.concentus.OpusEncoder? = null
            if (config.codec == AudioCodecKind.OPUS) {
                require(config.sampleRate == 48_000 && config.channels == 1)
                if (Build.VERSION.SDK_INT >= 29) platform = runCatching {
                    RelayAudioCodec(RelayAudioCodec.encoder(MediaFormat.MIMETYPE_AUDIO_OPUS,
                        48_000, 1, config.bitrate ?: 48_000), true)
                }.getOrNull()
                if (platform == null) software = org.concentus.OpusEncoder(48_000, 1,
                    OpusApplication.OPUS_APPLICATION_VOIP).apply {
                    setBitrate(config.bitrate ?: 48_000); setComplexity(5); setSignalType(OpusSignal.OPUS_SIGNAL_VOICE)
                }
            }
            val udp = DatagramSocket().also { socket = it }
            fun send(packet: ByteArray) { if (running.get()) udp.send(DatagramPacket(packet, packet.size, config.host, config.port)) }
            var pts = 0L
            var next = System.nanoTime()
            while (running.get()) {
                frames.tick({ pcm ->
                    val codec = platform
                    if (codec != null) {
                        codec.offer(pcm, pts); pts += 20_000
                        val output = mutableListOf<ByteArray>(); codec.pump({ data, _ -> output.add(data) }); output
                    } else {
                        val output = ByteArray(1275)
                        val length = software!!.encode(pcm, 0, 960, output, 0, output.size)
                        if (length > 0) listOf(output.copyOf(length)) else emptyList()
                    }
                }, ::send)
                next += 20_000_000
                if (next - System.nanoTime() < -20_000_000) {
                    Log.w("DiRelayMic", "Microphone tick late; reset schedule")
                    next = System.nanoTime() + 20_000_000
                }
                while (running.get()) {
                    val wait = next - System.nanoTime()
                    if (wait <= 0) break
                    LockSupport.parkNanos(wait)
                }
            }
        } catch (failure: Exception) {
            if (running.get()) { Log.e("DiRelayMic", "Microphone uplink failed", failure); failed() }
        } finally {
            running.set(false); socket?.close(); runCatching { platform?.close() }
        }
    }
    override fun close() { running.set(false); LockSupport.unpark(worker) }
}
