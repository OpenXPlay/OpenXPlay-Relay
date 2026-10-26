// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import android.os.SystemClock
import android.util.Log

/** Android delivery boundary. No callback reads settings, secrets or another component's locks. */
class ConnectionLog(private val clock: () -> Long = SystemClock::elapsedRealtime,
    private val write: (String, String) -> Unit = { tag, line -> Log.i(tag, line); Unit }) : AutoCloseable {
    private val timeline = ConnectionTimeline(clock)
    private val worker = BoundedLogWorker("relay-timeline", warning = { detail ->
        timeline.event("relay", "logging", detail).forEach { write("OXP-Timeline", it) }
    })
    fun event(domain: String, event: String, detail: String = "") = submit { at -> timeline.event(domain, event, detail, at) }
    fun transition(domain: String, key: String, event: String, detail: String = "") = submit { at -> timeline.transition(domain, key, event, detail, at) }
    fun phoneStatus(status: String, detail: String = "") = submit { at -> timeline.phoneStatus(status, detail, at) }
    fun phone(event: String, detail: String = "") = submit { at -> timeline.phone(event, detail, at) }
    fun carlink(message: String, connection: Long = 0) = submit { at -> timeline.carlink(message, at, connection) }
    fun diagnostic(message: String) { runCatching { worker.offer { write("DiRelayCarlink", message) } } }
    private fun submit(action: (Long) -> List<String>) {
        runCatching { val at = clock(); worker.offer { action(at).forEach { write("OXP-Timeline", it) } } }
    }
    internal fun awaitIdle() = worker.awaitIdle()
    override fun close() = worker.close()
}
