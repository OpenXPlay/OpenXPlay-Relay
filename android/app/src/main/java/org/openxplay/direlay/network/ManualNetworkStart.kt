// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.network

/** Session actions serialize mutations; snapshots are also read by the local UI. */
class ManualNetworkStart {
    private var sessionSeen = false
    @Volatile var pending: NetworkReadiness? = null
        private set
    @Volatile var reminders: Set<NetworkRequirement> = emptySet()
        private set

    fun request(manual: Boolean, readiness: NetworkReadiness): Boolean {
        if (pending != null) return false
        sessionSeen = false
        reminders = emptySet()
        if (readiness.problems.isNotEmpty()) {
            pending = readiness
            return false
        }
        reminders = if (manual) readiness.unknown else emptySet()
        return true
    }

    fun recheck(readiness: NetworkReadiness): Boolean {
        if (pending == null) return false
        pending = readiness
        return if (readiness.problems.isEmpty()) accept() else false
    }

    fun accept(): Boolean {
        val request = pending ?: return false
        reminders = request.unknown
        pending = null
        return true
    }

    /** A stopped or working session must not leave reminders for a later Bluetooth/boot launch. */
    fun observeSession(running: Boolean, active: Boolean) {
        if (active || sessionSeen && !running) reminders = emptySet()
        if (running) pending = null
        sessionSeen = running
    }

    fun cancel() { pending = null; reminders = emptySet(); sessionSeen = false }
}
