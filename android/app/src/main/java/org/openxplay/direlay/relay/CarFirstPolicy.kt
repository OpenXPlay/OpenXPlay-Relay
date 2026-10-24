// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-24
package org.openxplay.direlay.relay

/** A start owns one bounded wait; polling never extends its deadline. */
class CarFirstPolicy {
    @Volatile var untilMillis: Long? = null
        private set
    fun request(nowMillis: Long, automaticFit: Boolean, browser: Boolean, carlink: Boolean,
        screenReported: Boolean, screenSaved: Boolean): Boolean {
        untilMillis = if (automaticFit && (browser || carlink) && !screenReported && !screenSaved)
            nowMillis + MAX_WAIT_MILLIS else null
        return untilMillis != null
    }
    fun ready(nowMillis: Long, screenReported: Boolean): Boolean {
        val until = untilMillis ?: return true
        if (!screenReported && nowMillis < until) return false
        cancel()
        return true
    }
    fun cancel() { untilMillis = null }
    companion object { const val MAX_WAIT_MILLIS = 20_000L }
}
