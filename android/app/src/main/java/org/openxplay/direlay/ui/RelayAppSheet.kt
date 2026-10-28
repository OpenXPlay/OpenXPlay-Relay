// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.ui

import androidx.compose.runtime.staticCompositionLocalOf
import org.openxplay.direlay.settings.DlnaRendererChoice
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.path.StepAction

internal val LocalOpenRendererPicker = staticCompositionLocalOf<() -> Unit> { {} }

internal enum class RelayAppSheet { RENDERER, CARLINK_PIN }

/** Pairing temporarily covers a requested picker; it can resume when pairing ends. */
internal fun relayAppSheet(rendererPicker: Boolean, pendingPin: Boolean): RelayAppSheet? = when {
    pendingPin -> RelayAppSheet.CARLINK_PIN
    rendererPicker -> RelayAppSheet.RENDERER
    else -> null
}

internal fun rendererSelection(initial: RelaySettings, draft: RelaySettings, editing: Boolean, choice: DlnaRendererChoice) =
    (if (editing) draft else initial).copy(dlnaRenderer = choice)

internal fun stepNavigationDestination(action: StepAction?, settingsErrorPage: String): String? = when (action) {
    StepAction.FIX_SETTINGS -> settingsErrorPage
    StepAction.CHOOSE_IPHONE -> "settings:iphone"
    StepAction.CHOOSE_SHARED_WIFI -> "settings:sharedWifi"
    else -> null
}
