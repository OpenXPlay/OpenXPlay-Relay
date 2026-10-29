// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.logs

import android.content.Context
import android.os.Process
import android.os.SystemClock
import java.io.File
import java.time.Instant
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Turns one-second watchdog samples into findings. A sample gap means the whole process stopped running
 * (device sleep when uptime stood still too, otherwise a frozen or starved process). A stale runtime tick,
 * or a main-thread probe still queued, while the watchdog itself runs means that thread is blocked.
 */
class ProcessStallDetector(private val threshold: Long = 5_000) {
    /** [thread] names the thread whose stack belongs with the finding. */
    data class Finding(val event: String, val detail: String, val thread: String? = null)
    private var lastRealtime: Long? = null
    private var lastUptime = 0L
    private var graceUntil = Long.MIN_VALUE

    /** One thread that should answer within [threshold]: reported blocked with growing gaps, then unblocked. */
    private inner class Blocked(val name: String) {
        var since: Long? = null
        var next = 0L
        fun observe(realtime: Long, age: Long?, findings: MutableList<Finding>) {
            if (age == null) return
            val start = since
            if (age <= threshold) {
                if (start != null) { findings += Finding("${name}_unblocked", "after=${realtime - start}"); since = null }
                return
            }
            if (realtime < graceUntil) return
            if (start == null) since = realtime - age
            else if (age < next) return
            next = nextStallReport(age)
            findings += Finding("${name}_blocked", "for=$age", name)
        }
    }
    private val runtime = Blocked("runtime")
    private val main = Blocked("main")

    fun observe(realtime: Long, uptime: Long, tickAge: Long?, mainAge: Long? = null): List<Finding> = buildList {
        lastRealtime?.let { previous ->
            val gap = realtime - previous
            if (gap > threshold) {
                val uptimeGap = uptime - lastUptime
                add(Finding("process_paused", "gap=$gap uptime_gap=$uptimeGap cause=${if (uptimeGap * 2 < gap) "device_sleep" else "process_stopped"}"))
                // The runtime tick missed the same pause; give it one threshold to catch up.
                graceUntil = realtime + threshold
            }
        }
        lastRealtime = realtime
        lastUptime = uptime
        runtime.observe(realtime, tickAge, this)
        main.observe(realtime, mainAge, this)
    }
}

/**
 * Its own thread, never the runtime executor: logs process pauses and runtime or main-thread blocks, reports a connection
 * path that stops changing, keeps a heartbeat file and tells, at start, how the previous process ended.
 * The lambdas read volatile state only, except [pathLine], which runs on a separate sampler thread.
 * [enabled] is the stall-watch experiment (null until settings load); while it is off only the start report runs.
 */
class RelayWatchdog(private val context: Context, private val log: ConnectionLog, private val tickAge: () -> Long?,
    private val facts: () -> Map<String, String>, private val pathLine: () -> String?,
    private val enabled: () -> Boolean? = { true }) : AutoCloseable {
    private val heartbeat = HeartbeatFile(File(context.noBackupFilesDir, "diagnostics/heartbeat.properties"))
    private val preferences = context.getSharedPreferences("diagnostics", Context.MODE_PRIVATE)
    private var detector = ProcessStallDetector()
    private val path = SteadyStateLog { waited, line -> log.event("relay", "path_waiting", "waited=$waited line=$line") }
    private val sampler = Executors.newSingleThreadExecutor { Thread(it, PATH_THREAD).apply { isDaemon = true } }
    private val sampling = AtomicBoolean()
    @Volatile private var sampleStarted = 0L
    @Volatile private var sampleReported = false
    @Volatile private var lastPath: String? = null
    private val main = android.os.Handler(android.os.Looper.getMainLooper())
    private val mainQueued = AtomicBoolean()
    @Volatile private var mainPostedAt = 0L
    private val mainProbe = Runnable { mainQueued.set(false) }
    private val running = AtomicBoolean()
    private var thread: Thread? = null
    @Volatile var previousProcess: String? = null
        private set

    fun start() {
        if (!running.compareAndSet(false, true)) return
        thread = Thread({ run() }, "relay-watchdog").apply { isDaemon = true; start() }
    }

    private fun run() {
        runCatching { reportPrevious() }
        var round = 0L
        var watching: Boolean? = null
        while (running.get()) {
            val on = runCatching { enabled() }.getOrNull()
            if (on != null && on != watching) { watching = on; runCatching { switched(on) } }
            if (on == true) runCatching { sample(round++) }
            try { Thread.sleep(if (on == true) 1_000 else 5_000) } catch (_: InterruptedException) { break }
        }
    }

    /** A fresh detector, so time spent off never reads as a pause; off drops the heartbeat so no start reports it stale. */
    private fun switched(on: Boolean) {
        detector = ProcessStallDetector()
        if (!on) heartbeat.delete()
        log.event("relay", "stall_watchdog", if (on) "on" else "off")
    }

    private fun sample(round: Long) {
        val realtime = SystemClock.elapsedRealtime()
        for (finding in detector.observe(realtime, SystemClock.uptimeMillis(), tickAge(), mainAge(realtime)))
            log.event("relay", finding.event, when (finding.thread) {
                null -> finding.detail
                "main" -> "${finding.detail} ${threadStack(main.looper.thread)}"
                else -> "${finding.detail} ${threadStack(RUNTIME_THREAD)}"
            })
        if (round % 5 == 0L) samplePath(realtime)
        if (round % 10 == 0L) heartbeat.write(state())
        if (round % 15 == 0L) log.transition("relay", "power", "power", readPowerState(context).line())
        if (round % 30 == 0L) log.event("relay", "heartbeat", "importance=${processImportance()} " +
            facts().entries.joinToString(" ") { "${it.key}=${it.value}" })
    }

    /** How long the probe has waited in the main queue; a frozen UI shows here before Android's own frame warnings. */
    private fun mainAge(realtime: Long): Long {
        if (mainQueued.get()) return realtime - mainPostedAt
        mainPostedAt = realtime
        mainQueued.set(true)
        if (!main.post(mainProbe)) mainQueued.set(false)
        return 0
    }

    private fun samplePath(realtime: Long) {
        if (!sampling.compareAndSet(false, true)) {
            // A path read that never returns waits on a runtime lock; say where, once.
            if (!sampleReported && realtime - sampleStarted > 30_000) {
                sampleReported = true
                log.event("relay", "path_sample_blocked", "for=${realtime - sampleStarted} ${threadStack(PATH_THREAD)}")
            }
            return
        }
        sampleStarted = realtime
        sampleReported = false
        runCatching { sampler.execute {
            try {
                val line = pathLine()
                lastPath = line
                path.observe(line, line?.contains("LIVE✓") == true, SystemClock.elapsedRealtime())
            } catch (_: Exception) { } finally { sampling.set(false) }
        } }.onFailure { sampling.set(false) }
    }

    private fun state(): Map<String, String> = facts() + mapOf("pid" to Process.myPid().toString(),
        "wall" to System.currentTimeMillis().toString(), "realtime" to SystemClock.elapsedRealtime().toString(),
        "importance" to processImportance()) + listOfNotNull(lastPath?.let { "path" to it })

    /** Exits newer than the last ones logged, then the previous process's last heartbeat. */
    private fun reportPrevious() {
        val previous = heartbeat.read()
        val exits = readProcessExits(context)
        val logged = preferences.getLong("exitLogged", 0L)
        exits.filter { it.atMillis > logged }.asReversed().forEach { log.event("relay", "process_exit", it.line()) }
        exits.maxOfOrNull { it.atMillis }?.let { if (it > logged) preferences.edit().putLong("exitLogged", it).apply() }
        if (previous == null) return
        val pid = previous["pid"]?.toIntOrNull()
        val wall = previous["wall"]?.toLongOrNull()
        val exit = exits.firstOrNull { it.pid == pid }
        val line = buildString {
            append("pid=$pid")
            if (wall != null) append(" last_seen=${Instant.ofEpochMilli(wall)} ago=${System.currentTimeMillis() - wall}")
            exit?.let { append(" exit=${it.reason} exit_at=${Instant.ofEpochMilli(it.atMillis)}") }
            for ((key, value) in previous.toSortedMap()) if (key !in setOf("pid", "wall", "realtime")) append(" $key=$value")
        }
        previousProcess = line
        log.event("relay", "previous_process", line)
    }

    /** For the export summary: fresh platform facts plus what the start found. */
    fun details(): String = buildString {
        append("Power: ${readPowerState(context).line()} importance=${processImportance()}\n")
        previousProcess?.let { append("Previous process: $it\n") }
        readProcessExits(context, 3).forEach { append("Process exit: ${it.line()}\n") }
    }

    override fun close() {
        running.set(false)
        thread?.interrupt()
        sampler.shutdownNow()
    }

    companion object {
        const val RUNTIME_THREAD = "direlay-runtime"
        private const val PATH_THREAD = "relay-path-watch"

        /** The named thread's state and top frames, e.g. "thread=BLOCKED stack=RelayRuntime.tick:771<...". */
        fun threadStack(name: String, frames: Int = 12): String =
            runCatching { Thread.getAllStackTraces().keys.firstOrNull { it.name == name } }.getOrNull()?.let { threadStack(it, frames) }
                ?: "thread=missing"

        fun threadStack(thread: Thread, frames: Int = 12): String = "thread=${thread.state} stack=" +
            runCatching { thread.stackTrace }.getOrDefault(emptyArray()).take(frames).joinToString("<") {
                "${it.className.substringAfterLast('.')}.${it.methodName}:${it.lineNumber}"
            }
    }
}
