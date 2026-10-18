// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-18

package org.openxplay.direlay.relay

import org.openxplay.direlay.settings.*
import kotlin.math.abs

data class DisplayTarget(val profile: DisplayProfile, val resolution: CarPlayResolution, val screenWidth: CarPlayScreenWidth, val screen: ScreenDataKey? = null)
/** Compare to the frozen, running canvas; a request is never evidence that CarPlay applied it. */
class DisplayRefit {
    private var pending: DisplayTarget? = null
    private var since = 0L
    private var lastRestart: Long? = null
    @Synchronized fun restarted(now: Long) { lastRestart = now }
    @Synchronized fun due(now: Long, running: Boolean, mode: DisplayMode, applied: DisplayTarget?,
        desired: DisplayTarget?, viewport: Boolean, viewAreas: Boolean = false): Boolean {
        val changedWidth = applied != null && desired != null && applied.screenWidth != desired.screenWidth
        val changedSetting = changedWidth || (applied != null && desired != null && applied.resolution != desired.resolution)
        val changedScreen = applied != null && desired?.screen != null && applied.screen != desired.screen
        val geometry = applied != null && desired != null && !viewAreas && viewport && desired.resolution != CarPlayResolution.SCREEN && desired.resolution != CarPlayResolution.NATIVE &&
            significant(applied.profile, desired.profile)
        if (!running || (mode != DisplayMode.AUTOMATIC && !(mode == DisplayMode.FIXED && changedWidth)) || desired == null || (!changedSetting && !changedScreen && !geometry)) {
            pending = null; return false
        }
        if (pending != desired) { pending = desired; since = now }
        return now - since >= 1000 && (lastRestart == null || now - lastRestart!! >= 10_000)
    }
    companion object {
        fun significant(running: DisplayProfile, desired: DisplayProfile): Boolean =
            abs(desired.width.toDouble() * running.height - running.width.toDouble() * desired.height) / (desired.height.toDouble() * running.width) > 0.03 ||
            abs(desired.width.toDouble() - running.width) / running.width > 0.10 ||
            abs(desired.height.toDouble() - running.height) / running.height > 0.10
    }
}
