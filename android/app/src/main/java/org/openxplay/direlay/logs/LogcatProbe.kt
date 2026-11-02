// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-01
package org.openxplay.direlay.logs

import java.util.UUID

/** Measures only the Android logging API -> this recorder's raw logcat input boundary. */
class LogcatProbe(private val clock: () -> Long, private val nonce: () -> String = { UUID.randomUUID().toString() },
    private val windowMillis: Long = 5_000) {
    enum class State { NOT_STARTED, PENDING, COMPLETE }
    data class Result(val state: State, val seen: Set<Char>, val writes: Map<Char, Int?>, val windowMillis: Long = 5_000) {
        val incomplete: Boolean get() = state == State.COMPLETE && seen.size != priorities.size
        fun summary(): String = buildString {
            append("Probe (current recorder start, ${windowMillis} ms): ")
            when (state) {
                State.NOT_STARTED -> append("not started; capture completeness unknown")
                State.PENDING -> append("pending; capture completeness unknown")
                State.COMPLETE -> {
                    append(when {
                        seen.isEmpty() -> "logcat returned no probe markers"
                        seen.size == priorities.size -> "logcat returned all priorities (V, D, I, W, E)"
                        seen.size == 1 -> "logcat returned ${seen.single()} only"
                        else -> "logcat returned " + priorities.filter { it in seen }.joinToString(", ")
                    })
                    val missing = priorities.filter { it !in seen }
                    if (missing.isNotEmpty()) append("; missing " + missing.joinToString(", ") { names.getValue(it) })
                }
            }
            append("\nMarker write results (Log.println, null means failure): " +
                priorities.joinToString(", ") { "$it=${writes[it]}" })
            append("\nProbe scope: logging API to app logcat capture with *:V; older records and car connectivity are not assessed. " +
                "Markers are checked before redaction and storage; missing priorities did not reach that boundary within the window. " +
                "Emission, firmware, runtime and reader filtering or delay remain possible causes.\n")
        }
    }
    private var started: Long? = null
    private var markerNonce = ""
    private val seen = linkedSetOf<Char>()
    private val writes = linkedMapOf<Char, Int?>()
    @Synchronized fun start(write: (Char, String) -> Int) {
        if (started != null) return
        markerNonce = nonce()
        started = clock()
        for (priority in priorities) writes[priority] = runCatching { write(priority, marker(priority)) }.getOrNull()
    }
    @Synchronized fun observe(line: String) {
        val start = started ?: return
        if (clock() - start >= windowMillis) return
        val header = LogLine.header(line)
        if (!header.endsWith(" $TAG: ") && !header.endsWith(" $TAG:")) return
        val priority = header.trim().substringBeforeLast(" $TAG:").lastOrNull() ?: return
        if (priority in priorities && LogLine.message(line) == marker(priority)) seen += priority
    }
    @Synchronized fun result(): Result = Result(when {
        started == null -> State.NOT_STARTED
        clock() - checkNotNull(started) < windowMillis -> State.PENDING
        else -> State.COMPLETE
    }, seen.toSet(), writes.toMap(), windowMillis)
    private fun marker(priority: Char) = "nonce=$markerNonce priority=$priority"
    companion object {
        const val TAG = "OXP-CaptureProbe"
        private val priorities = listOf('V', 'D', 'I', 'W', 'E')
        private val names = mapOf('V' to "VERBOSE", 'D' to "DEBUG", 'I' to "INFO", 'W' to "WARN", 'E' to "ERROR")
    }
}
