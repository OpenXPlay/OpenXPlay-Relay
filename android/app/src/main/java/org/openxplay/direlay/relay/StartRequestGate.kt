// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.relay

import java.util.concurrent.atomic.AtomicBoolean

interface StartRequestGate {
    val pending: Boolean
    fun begin(): Boolean
    fun finish()
}
/** Preparation outlives an activity recreation, but never a process. */
class ProcessStartRequestGate : StartRequestGate {
    private val active = AtomicBoolean(false)
    override val pending get() = active.get()
    override fun begin() = active.compareAndSet(false, true)
    override fun finish() { active.set(false) }
}
