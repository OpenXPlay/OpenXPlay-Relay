// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18

package org.openxplay.direlay.carlinkdebug

import java.io.File
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import org.openxplay.stream.clink.trace.ClinkMediaTap

class CarlinkMediaCapture(private val directory: File, private val durationNs: Long = 60_000_000_000,
    private val maxBytes: Long = 64L * 1024 * 1024) : ClinkMediaTap, AutoCloseable {
    private val queue = ArrayBlockingQueue<ByteArray>(256)
    private val lock = Any()
    @Volatile private var captureDeadline = 0L
    @Volatile var capturing = false
        private set
    @Volatile var bytes = 0L
        private set
    @Volatile var failure: String? = null
        private set
    @Volatile private var clearing = false
    private var worker: Thread? = null
    @Synchronized fun start() {
        if (clearing || worker?.isAlive == true) return
        queue.clear(); bytes = 0; failure = null
        val deadline = System.nanoTime() + durationNs
        captureDeadline = deadline; capturing = true
        worker = Thread({
            try {
                check(directory.isDirectory || directory.mkdirs())
                File(directory, "media-${System.currentTimeMillis()}.ts").outputStream().buffered().use { output ->
                    while (capturing && System.nanoTime() < deadline) {
                        val remaining = deadline - System.nanoTime()
                        if (remaining <= 0) break
                        val payload = queue.poll(minOf(remaining, 100_000_000), TimeUnit.NANOSECONDS) ?: continue
                        if (!capturing || System.nanoTime() >= deadline) break
                        if (bytes + payload.size > maxBytes) break
                        output.write(payload); bytes += payload.size
                        if (bytes >= maxBytes) break
                    }
                }
            } catch (e: Exception) { failure = e.javaClass.simpleName }
            finally { capturing = false; synchronized(lock) { queue.clear() } }
        }, "carlink-ts-writer").apply { isDaemon = true; start() }
    }
    override fun rtp(packet: ByteArray, offset: Int, length: Int) {
        if (!capturing || length <= 12 || offset < 0 || offset > packet.size - length) return
        if (System.nanoTime() >= captureDeadline) { capturing = false; return }
        synchronized(lock) {
            if (!capturing) return
            if (!queue.offer(packet.copyOfRange(offset + 12, offset + length))) capturing = false
        }
    }
    fun stop() { capturing = false }
    @Synchronized fun clear() {
        stop(); clearing = true
        val previous = worker
        Thread({
            try { previous?.join(); directory.listFiles { f -> f.extension == "ts" }.orEmpty().forEach { it.delete() } }
            finally { clearing = false }
        }, "carlink-ts-clear").apply { isDaemon = true; start() }
    }
    override fun close() { stop(); worker?.join(5000) }
}
