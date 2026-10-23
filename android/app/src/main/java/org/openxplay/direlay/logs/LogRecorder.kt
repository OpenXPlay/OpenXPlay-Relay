// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import android.util.Log
import java.io.Closeable
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class LogRecorder(val store: LogSegmentStore, private val redactor: LogRedactor) : Closeable {
    @Volatile private var running = false
    @Volatile private var process: Process? = null
    private var thread: Thread? = null
    private val timer = Executors.newSingleThreadScheduledExecutor { Thread(it, "relay-log-flush").apply { isDaemon = true } }
    private var retryWriteAt = 0L
    private var warned = false
    @Synchronized fun start(version: String, code: Long) {
        if (running) return
        running = true
        timer.scheduleWithFixedDelay({ synchronized(this) {
            if (running) writeSafely { store.maintenance() }
        } }, 1, 1, TimeUnit.SECONDS)
        thread = Thread({ record() }, "relay-logcat").apply { isDaemon = true; start() }
        Log.i("OXP-Logs", "Recorder started: OpenXPlay Relay $version ($code), pid ${android.os.Process.myPid()}")
    }
    private fun record() {
        val failures = ArrayDeque<Long>()
        val stream = LogRedactorStream(redactor)
        while (running) {
            try {
                val command = mutableListOf("logcat", "-b", "main", "-b", "system", "-b", "crash", "-v", "threadtime", "-v", "year", "-v", "UTC")
                store.lastTimestamp?.let { command += listOf("-T", String.format(Locale.ROOT, "%.3f", (it + 1) / 1000.0)) }
                val child = ProcessBuilder(command).redirectErrorStream(true).start()
                process = child
                if (!running) child.destroy()
                child.inputStream.bufferedReader(Charsets.UTF_8).use { reader ->
                    while (running) {
                        val line = reader.readLine() ?: break
                        if (line.startsWith("---------")) continue
                        val cleaned = runCatching { stream.redact(line) }.getOrNull() ?: continue
                        synchronized(this) { writeSafely(writing = true) { store.append(cleaned) } }
                    }
                }
            } catch (_: Exception) {
                if (!running) break
            } finally { process?.destroy(); process = null; runCatching { flush() } }
            if (!running) break
            val now = System.currentTimeMillis()
            while (failures.isNotEmpty() && failures.first() < now - 60_000) failures.removeFirst()
            failures.addLast(now)
            if (failures.size >= 5) { Log.w("OXP-Logs", "Log recorder stopped after repeated logcat failures"); break }
            try { Thread.sleep(2_000) } catch (_: InterruptedException) { break }
        }
    }
    private fun writeSafely(writing: Boolean = false, action: () -> Unit) {
        if (System.currentTimeMillis() < retryWriteAt) return
        try { action(); if (writing) warned = false } catch (_: Exception) {
            runCatching { store.close() }
            retryWriteAt = System.currentTimeMillis() + 30_000
            if (!warned) { warned = true; Log.w("OXP-Logs", "Log storage unavailable; retrying in 30 seconds") }
        }
    }
    @Synchronized fun flush() { store.flush() }
    override fun close() {
        running = false
        process?.destroy()
        thread?.interrupt()
        timer.shutdownNow()
        thread?.join(2_000)
        synchronized(this) { runCatching { store.close() } }
    }
}
