// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.presentation

import org.openxplay.stream.core.SessionPhase

data class HostFlags(val resumed: Boolean = false, val guideOpen: Boolean = false,
    val requestingPermissions: Boolean = false, val bootAutoStart: Boolean = false,
    val importOnly: Boolean = false, val materialChecked: Boolean = false, val identityReady: Boolean = false,
    val networkRefreshPending: Boolean = false, val wifi: String? = null, val deviceName: String? = null)

data class AutoStartSnapshot(val networkReady: Boolean, val networkStartPending: Boolean,
    val canStart: Boolean, val restarting: Boolean, val starting: Boolean, val running: Boolean,
    val failed: Boolean, val paused: Boolean, val observed: Boolean, val trackerRunning: Boolean,
    val phase: SessionPhase?, val automatic: Boolean)

fun shouldAutoStart(value: AutoStartSnapshot, flags: HostFlags): Boolean =
    value.networkReady && !flags.importOnly && !flags.guideOpen && !value.networkStartPending &&
        value.canStart && flags.resumed && !flags.requestingPermissions && !flags.networkRefreshPending &&
        !value.restarting && !value.starting && !value.running && !value.failed && !value.paused &&
        value.observed && !value.trackerRunning &&
        value.phase !in setOf(SessionPhase.CONNECTING, SessionPhase.ACTIVE, SessionPhase.RECONNECTING) &&
        (value.automatic || flags.bootAutoStart)
