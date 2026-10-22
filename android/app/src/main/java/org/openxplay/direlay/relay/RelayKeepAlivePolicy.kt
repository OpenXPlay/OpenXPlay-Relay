// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.relay

/** Start accepts a request; foregroundStarted confirms that the service reached the foreground. */
class RelayKeepAlivePolicy(private val start: () -> Boolean, private val stop: () -> Unit) : RelayKeepAlive {
    private var needed = false
    private var starting = false
    private var started = false
    val pending: Boolean get() = needed && !starting && !started

    override fun update(needed: Boolean) {
        if (this.needed == needed) return
        this.needed = needed
        if (needed) attemptStart() else if (started) {
            started = false
            stop()
        }
    }

    override fun uiVisible() { if (pending) attemptStart() }

    private fun attemptStart() {
        if (starting || started) return
        starting = true
        if (!start()) foregroundRefused()
    }

    fun foregroundStarted() {
        if (!starting) return
        starting = false
        started = needed
        if (!needed) stop()
    }

    fun foregroundRefused() {
        starting = false
        started = false
    }

    companion object {
        fun needed(open: Boolean, webEnabled: Boolean, carlinkEnabled: Boolean) =
            open && (webEnabled || carlinkEnabled)
    }
}
