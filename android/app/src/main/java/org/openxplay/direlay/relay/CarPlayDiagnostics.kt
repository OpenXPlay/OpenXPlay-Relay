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
    private val nanoClock: () -> Long = System::nanoTime,
    private val startupEnabled: () -> Boolean = { false },
    private val timeline: org.openxplay.direlay.logs.ConnectionLog? = null) : CarPlayEventObserver {
    @Volatile var error: CarPlayError? = null
        private set

    @Synchronized override fun onStatus(status: CarPlayStatus) {
        if (status == CarPlayStatus.DiscoveringMfi) { milestones.clear(); runtimeFrames = 0 }
        val wall = wallClock()
        val line = when (status) {
            is CarPlayStatus.Failed -> "status=Failed reason=${status.startupFailure?.name ?: "none"} wifiResetRequired=${status.wifiResetRequired}"
            is CarPlayStatus.HotspotReady -> "status=HotspotReady backend=${fact(status.backend)} band=${fact(status.band)} channel=${status.channel}"
            else -> "status=${status.javaClass.simpleName}"
        }
        runCatching { timeline?.phoneStatus(status.javaClass.simpleName, line.substringAfter(" ", "")) }
        trail.record(line, status == CarPlayStatus.DiscoveringMfi, wall, nanoClock())
        if (status is CarPlayStatus.Failed) error = CarPlayError(redactFailureText(status.message, secrets()),
            status.startupFailure?.name?.lowercase(Locale.ROOT), trail.snapshot(), wall)
    }

    private val milestones = linkedSetOf<String>()
    private var runtimeFrames = 0L
    @Synchronized override fun onStartupMilestone(name: String) {
        if (!startupEnabled() || name.length > 64 || name.any { it !in 'a'..'z' && it != '_' } || !milestones.add(name)) return
        record("milestone=$name")
        runCatching { timeline?.phone("startup_milestone", "stage=$name") }
    }
    @Synchronized override fun onIap2Frame(frame: com.shilapi.xcertplay.iap2.wire.Iap2Frame) {
        if (!startupEnabled()) return
        runtimeFrames++
        // Powers of two bound log volume while proving runtime iAP2 survives the handoff.
        if (runtimeFrames and (runtimeFrames - 1) == 0L) {
            record("runtime_iap2 frames=$runtimeFrames")
            runCatching { timeline?.phone("runtime_iap2", "frames=$runtimeFrames") }
        }
    }
    @Synchronized override fun onProgress(message: String) {
        val filtered = redactFailureText(message, secrets())
        if (filtered.length > 160 || filtered.any { it !in ' '..'~' } || hexRun.containsMatchIn(filtered)) return
        record("progress $filtered")
        runCatching { timeline?.phone("progress", filtered) }
    }
    @Synchronized override fun onSessionActive() { record("session_active"); runCatching { timeline?.phone("session_active") }; clear() }
    @Synchronized override fun onSessionEnded() { record("session_ended"); runCatching { timeline?.phone("session_ended") } }
    @Synchronized override fun onCommand(type: String, params: Map<String, Any?>) {
        record("command type=${fact(type)}")
    }
    @Synchronized fun clear() { error = null }
    private fun record(line: String) { trail.record(line, false, wallClock(), nanoClock()) }
    private fun fact(value: String) = value.filter { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it == '_' }
    private companion object { val hexRun = Regex("[A-Fa-f0-9]{16,}") }
}
