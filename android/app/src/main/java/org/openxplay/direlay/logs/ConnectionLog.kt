// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.logs

import android.os.SystemClock
import android.util.Log

/** Android delivery boundary. No callback reads settings, secrets or another component's locks. */
class ConnectionLog(private val clock: () -> Long = SystemClock::elapsedRealtime,
    private val write: (String, String) -> Unit = { tag, line -> Log.i(tag, line); Unit },
    private val context: () -> String = { ConnectionContext().line() }) : AutoCloseable {
    private val redactor = LogRedactor()
    @Volatile private var redact: (String) -> String = redactor::redact
    private val timeline = ConnectionTimeline(clock, context = { context() })
    private val worker = BoundedLogWorker("relay-timeline", warning = { detail ->
        timeline.event("relay", "logging", detail).forEach { write("OXP-Timeline", it) }
    })
    private val timer = java.util.concurrent.Executors.newSingleThreadScheduledExecutor {
        Thread(it, "relay-timeline-flaps").apply { isDaemon = true }
    }.apply { scheduleWithFixedDelay({ submit { at -> timeline.flush(at) } }, 1, 1, java.util.concurrent.TimeUnit.SECONDS) }
    fun filterWith(filter: (String) -> String) { redact = filter }
    fun linkEnded(domain: String, end: LinkEnd, connection: Long = 0) = submit { at -> timeline.linkEnded(domain, end.filtered(redact), at, connection) }
    fun phoneDecision(end: LinkEnd, next: String) = submit { at -> timeline.phoneDecision(end.filtered(redact), next, at) }
    fun event(domain: String, event: String, detail: String = "") = submit { at -> timeline.event(domain, event, detail, at) }
    fun detailEvent(domain: String, event: String, facts: String, detail: String) = submit { at ->
        timeline.event(domain, event, facts + quotedDetail(redact(detail)), at)
    }
    fun transition(domain: String, key: String, event: String, detail: String = "") = submit { at -> timeline.transition(domain, key, event, detail, at) }
    fun phoneStatus(status: String, detail: String = "") = submit { at -> timeline.phoneStatus(status, detail, at) }
    fun phone(event: String, detail: String = "") = submit { at -> timeline.phone(event, detail, at) }
    fun carlink(message: String, connection: Long = 0) = submit { at -> timeline.carlink(message, at, connection) }
    fun carlinkDiagnostic(message: String, connection: Long) = submit { at -> timeline.carlinkDiagnostic(message.take(4096), connection, at) }
    fun cast(message: String, connection: Long = 0) = submit { at -> timeline.cast(message.take(4096), connection, at) }
    fun castEvent(event: String, detail: String = "", connection: Long) = submit { at -> timeline.castEvent(event, detail, connection, at) }
    fun diagnostic(message: String) { runCatching { worker.offer { write("DiRelayCarlink", redact(message)) } } }
    private fun submit(action: (Long) -> List<String>) {
        runCatching { val at = clock(); worker.offer { action(at).forEach { write("OXP-Timeline", redact(it)) } } }
    }
    internal fun awaitIdle() = worker.awaitIdle()
    override fun close() { timer.shutdownNow(); submit { at -> timeline.flush(at, force = true) }; worker.close() }
}
