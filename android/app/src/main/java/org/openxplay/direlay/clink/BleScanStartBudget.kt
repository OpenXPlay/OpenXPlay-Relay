// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.clink

/** Shared scan-start history; filtered scans record their starts without asking for permission. */
internal class BleScanStartBudget {
    private val starts = java.util.ArrayDeque<Long>()

    @Synchronized fun recordStart(nowMs: Long) {
        expire(nowMs)
        starts.addLast(nowMs)
    }

    @Synchronized fun tryStart(nowMs: Long, reserve: Int): Boolean {
        expire(nowMs)
        if (starts.size >= 5 - reserve) return false
        starts.addLast(nowMs)
        return true
    }

    @Synchronized fun count(nowMs: Long): Int {
        expire(nowMs)
        return starts.size
    }

    private fun expire(nowMs: Long) {
        while (starts.isNotEmpty() && nowMs - starts.first >= 30_000) starts.removeFirst()
    }
}
