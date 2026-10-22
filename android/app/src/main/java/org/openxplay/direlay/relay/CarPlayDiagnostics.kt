// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.CarPlayEventObserver
import com.shilapi.xcertplay.orchestration.CarPlayStatus
import java.util.Locale

data class CarPlayError(val text: String, val code: String?, val trail: List<String>, val atMillis: Long)

/** Store only filtered facts; credentials are fetched at redaction time, never retained here. */
class CarPlayDiagnostics(private val secrets: () -> Collection<String> = { emptyList() },
    private val trail: StepTrail = StepTrail(), private val wallClock: () -> Long = System::currentTimeMillis,
    private val nanoClock: () -> Long = System::nanoTime) : CarPlayEventObserver {
    @Volatile var error: CarPlayError? = null
        private set

    @Synchronized override fun onStatus(status: CarPlayStatus) {
        val wall = wallClock()
        val line = when (status) {
            is CarPlayStatus.Failed -> "status=Failed reason=${status.startupFailure?.name ?: "none"} wifiResetRequired=${status.wifiResetRequired}"
            is CarPlayStatus.HotspotReady -> "status=HotspotReady backend=${fact(status.backend)} band=${fact(status.band)} channel=${status.channel}"
            else -> "status=${status.javaClass.simpleName}"
        }
        trail.record(line, status == CarPlayStatus.DiscoveringMfi, wall, nanoClock())
        if (status is CarPlayStatus.Failed) error = CarPlayError(redactFailureText(status.message, secrets()),
            status.startupFailure?.name?.lowercase(Locale.ROOT), trail.snapshot(), wall)
    }

    @Synchronized override fun onProgress(message: String) {
        val filtered = redactFailureText(message, secrets())
        if (filtered.length > 160 || filtered.any { it !in ' '..'~' } || hexRun.containsMatchIn(filtered)) return
        record("progress $filtered")
    }
    @Synchronized override fun onSessionActive() { record("session_active"); clear() }
    @Synchronized override fun onSessionEnded() { record("session_ended") }
    @Synchronized override fun onCommand(type: String, params: Map<String, Any?>) {
        record("command type=${fact(type)}")
    }
    @Synchronized fun clear() { error = null }
    private fun record(line: String) { trail.record(line, false, wallClock(), nanoClock()) }
    private fun fact(value: String) = value.filter { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it == '_' }
    private companion object { val hexRun = Regex("[A-Fa-f0-9]{16,}") }
}
