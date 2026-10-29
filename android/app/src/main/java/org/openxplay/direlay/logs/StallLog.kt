// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.logs

/** Reports a lasting state after 30 s, then with doubling gaps of at most 10 min. */
internal fun nextStallReport(waited: Long): Long = minOf(waited * 2, waited + 600_000L).coerceAtLeast(30_000L)

/**
 * Named waits: one line on entry, lines with growing gaps while the wait lasts or when its detail changes,
 * and one line at its end. The owner calls [tick] at least every few seconds.
 */
class WaitLog(private val clock: () -> Long, private val write: (event: String, detail: String) -> Unit) {
    private class Wait(val since: Long, var detail: String) { var next = 30_000L }
    private val waits = linkedMapOf<String, Wait>()

    @Synchronized fun begin(name: String, detail: String = "") {
        if (name in waits) return tick(name, detail)
        waits[name] = Wait(clock(), detail)
        write("carplay_wait", join("for=$name", detail))
    }

    @Synchronized fun tick(name: String, detail: String = "") {
        val wait = waits[name] ?: return
        val waited = clock() - wait.since
        if (detail == wait.detail && waited < wait.next) return
        wait.detail = detail
        if (waited >= wait.next) wait.next = nextStallReport(waited)
        write("carplay_waiting", join("for=$name waited=$waited", detail))
    }

    @Synchronized fun end(name: String, outcome: String) {
        val wait = waits.remove(name) ?: return
        write("carplay_wait_end", "for=$name waited=${clock() - wait.since} outcome=$outcome")
    }

    @Synchronized fun active(name: String) = name in waits

    /** For the heartbeat: every wait still open, with its age in seconds. */
    @Synchronized fun summary(): String = waits.entries.joinToString(",") { "${it.key}:${(clock() - it.value.since) / 1000}s" }

    private fun join(head: String, detail: String) = if (detail.isEmpty()) head else "$head $detail"
}

/** A value that stops changing before it is done, such as the connection path line. */
class SteadyStateLog(private val write: (waited: Long, value: String) -> Unit) {
    private var value: String? = null
    private var since = 0L
    private var next = 30_000L

    /** [done] values (or null) never report; any change restarts the clock. */
    @Synchronized fun observe(current: String?, done: Boolean, now: Long) {
        if (current != value) { value = current; since = now; next = 30_000L; return }
        if (current == null || done) return
        val waited = now - since
        if (waited < next) return
        next = nextStallReport(waited)
        write(waited, current)
    }
}
