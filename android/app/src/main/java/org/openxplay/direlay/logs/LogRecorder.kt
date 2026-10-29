// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.logs

import android.os.SystemClock
import android.util.Log
import java.io.Closeable
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class LogRecorder(val store: LogSegmentStore, private val redactor: LogRedactor,
    private val clock: () -> Long = SystemClock::elapsedRealtime, private val silenceLimit: Long = 90_000) : Closeable {
    @Volatile private var running = false
    @Volatile private var process: Process? = null
    private var thread: Thread? = null
    private val timer = Executors.newSingleThreadScheduledExecutor { Thread(it, "relay-log-flush").apply { isDaemon = true } }
    private var retryWriteAt = 0L
    private var warned = false
    @Volatile private var lastLineAt = 0L
    private var lastTimerAt = 0L
    @Volatile private var startedAt: Long? = null
    @Volatile private var restarts = 0
    @Volatile private var gaps = 0
    @Volatile private var silences = 0
    /** The stall-watch experiment: false never restarts a silent logcat. */
    @Volatile var watchSilence: () -> Boolean = { true }
    @Synchronized fun start(version: String, code: Long) {
        if (running) return
        running = true
        startedAt = System.currentTimeMillis()
        lastLineAt = clock(); lastTimerAt = lastLineAt
        timer.scheduleWithFixedDelay({ synchronized(this) {
            if (running) { writeSafely { store.maintenance() }; checkSilence() }
        } }, 1, 1, TimeUnit.SECONDS)
        thread = Thread({ record() }, "relay-logcat").apply { isDaemon = true; start() }
        Log.i("OXP-Logs", "Recorder started: OpenXPlay Relay $version ($code), pid ${android.os.Process.myPid()}")
    }
    private fun record() {
        val failures = ArrayDeque<Long>()
        val stream = LogRedactorStream(redactor)
        var previousLine: Long? = null
        while (running) {
            var lines = 0L
            val opened = clock()
            var exit = "eof"
            try {
                val command = mutableListOf("logcat", "-b", "main", "-b", "system", "-b", "crash", "-v", "threadtime", "-v", "year", "-v", "UTC")
                store.lastTimestamp?.let { command += listOf("-T", String.format(Locale.ROOT, "%.3f", (it + 1) / 1000.0)) }
                val child = ProcessBuilder(command).redirectErrorStream(true).start()
                process = child
                if (!running) child.destroy()
                child.inputStream.bufferedReader(Charsets.UTF_8).use { reader ->
                    while (running) {
                        val line = reader.readLine() ?: break
                        lastLineAt = clock()
                        if (line.startsWith("---------")) continue
                        val cleaned = runCatching { stream.redact(line) }.getOrNull() ?: continue
                        lines++
                        val at = LogLine.timestamp(cleaned)
                        val gap = if (at != null && previousLine != null) at - previousLine!! else 0L
                        if (at != null) previousLine = at
                        synchronized(this) {
                            // Logcat still had lines on both sides: nothing of this app was logged in between.
                            if (gap > GAP) { gaps++; writeSafely(writing = true) { store.append(note(at!!, "no log lines for ${gap / 1000} s, since ${stamp(at - gap)}")) } }
                            writeSafely(writing = true) { store.append(cleaned) }
                        }
                    }
                }
                exit = runCatching { if (child.waitFor(1, TimeUnit.SECONDS)) "exit=${child.exitValue()}" else "running" }.getOrDefault("unknown")
            } catch (failure: Exception) {
                exit = "failure=${failure.javaClass.simpleName}"
                if (!running) break
            } finally { process?.destroy(); process = null; runCatching { flush() } }
            if (!running) break
            restarts++
            writeNote("logcat ended $exit lines=$lines after=${clock() - opened} ms; restarting")
            val now = System.currentTimeMillis()
            while (failures.isNotEmpty() && failures.first() < now - 60_000) failures.removeFirst()
            failures.addLast(now)
            if (failures.size >= 5) {
                writeNote("recorder stopped after repeated logcat failures")
                Log.w("OXP-Logs", "Log recorder stopped after repeated logcat failures"); break
            }
            try { Thread.sleep(2_000) } catch (_: InterruptedException) { break }
        }
    }
    /** Runs on the timer under this lock. A paused timer means a paused process, not a silent logcat. */
    private fun checkSilence() {
        val now = clock()
        if (now - lastTimerAt > 5_000 || !runCatching { watchSilence() }.getOrDefault(false)) lastLineAt = now
        lastTimerAt = now
        val child = process ?: return
        if (now - lastLineAt < silenceLimit) return
        silences++
        writeNote("logcat silent for ${(now - lastLineAt) / 1000} s; restarting it")
        lastLineAt = now
        child.destroy()
    }
    /**
     * Recorder events go straight to the store: logcat may be the part that failed. Notes carry the last
     * recorded timestamp, so a logcat resume (-T) never skips lines that were still in flight.
     */
    private fun writeNote(message: String) = synchronized(this) {
        writeSafely(writing = true) { store.append(note(store.lastTimestamp ?: System.currentTimeMillis(), "$message (at ${stamp(System.currentTimeMillis())})")) }
    }
    private fun writeSafely(writing: Boolean = false, action: () -> Unit) {
        if (System.currentTimeMillis() < retryWriteAt) return
        try { action(); if (writing) warned = false } catch (_: Exception) {
            runCatching { store.close() }
            retryWriteAt = System.currentTimeMillis() + 30_000
            if (!warned) { warned = true; Log.w("OXP-Logs", "Log storage unavailable; retrying in 30 seconds") }
        }
    }
    /** For the export summary. */
    fun summary(): String = "Recorder: started ${startedAt?.let(::stamp) ?: "never"}, logcat restarts $restarts, " +
        "silences $silences, gaps over ${GAP / 1000} s $gaps, last line ${store.lastTimestamp?.let(::stamp) ?: "none"}"
    @Synchronized fun flush() { store.flush() }
    override fun close() {
        running = false
        process?.destroy()
        thread?.interrupt()
        timer.shutdownNow()
        thread?.join(2_000)
        synchronized(this) { runCatching { store.close() } }
    }
    companion object {
        /** Longer than two heartbeats: a quiet but running relay never leaves such a gap. */
        const val GAP = 75_000L
        private val format = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS", Locale.ROOT).withZone(ZoneOffset.UTC)
        internal fun note(at: Long, message: String) =
            "${format.format(Instant.ofEpochMilli(at))} +0000 ${android.os.Process.myPid()} ${android.os.Process.myTid()} I OXP-Logs: $message"
        private fun stamp(at: Long) = Instant.ofEpochMilli(at).toString()
    }
}
