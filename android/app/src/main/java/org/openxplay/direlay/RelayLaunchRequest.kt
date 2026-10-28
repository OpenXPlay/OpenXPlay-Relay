// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay

import android.content.Intent
import org.openxplay.direlay.path.StepAction
import org.openxplay.direlay.ui.relayRoute

/** One-shot navigation is consumed before composition and never replayed on recreation. */
data class RelayLaunchRequest(val destination: String?, val stepAction: StepAction?) {
    companion object {
        fun take(intent: Intent, restoring: Boolean = false): RelayLaunchRequest {
            val target = intent.getStringExtra(RelayActivity.EXTRA_HOME_DESTINATION) ?: when (intent.getStringExtra("page")) {
                "home" -> "home"
                "settings" -> "settings"
                "wireless-recovery" -> "settings/connection/network"
                else -> null
            }
            val destination = target?.let(::relayRoute)?.takeIf {
                it in setOf("home", "setup", "settings", "developer", "experimental", "vehicle") ||
                    it.startsWith("setup:") || it.startsWith("settings/") || it.startsWith("developer/") ||
                    it.startsWith("snapshot:") && it.substringAfter(':').toLongOrNull() != null
            }
            val action = intent.getStringExtra(RelayActivity.EXTRA_STEP_ACTION)?.let {
                runCatching { StepAction.valueOf(it) }.getOrNull()
            }
            intent.removeExtra(RelayActivity.EXTRA_HOME_DESTINATION)
            intent.removeExtra(RelayActivity.EXTRA_STEP_ACTION)
            intent.removeExtra("page")
            return if (restoring) RelayLaunchRequest(null, null) else RelayLaunchRequest(destination, action)
        }
    }
}
