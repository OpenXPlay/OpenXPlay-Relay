// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-14
package org.openxplay.direlay.ui

import org.openxplay.direlay.bluetooth.IphoneSelection
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.stream.core.SessionPhase

/** No Android services or mutable runtime references enter the presentation mapper. */
enum class StatusTone { IDLE, PROGRESS, ATTENTION, RUNNING, ERROR }
enum class SetupAction { PERMISSIONS, IDENTITY, IPHONE, SETTINGS, SHARED_WIFI, CHOOSE_IPHONE }
data class SetupItem(val text: Int, val actionLabel: Int, val action: SetupAction)
data class HomePresentation(val headline: Int, val subline: Int, val tone: StatusTone,
    val chips: List<Int>, val checklist: List<SetupItem>, val canStart: Boolean, val running: Boolean)
data class HomeSnapshot(
    val settings: RelaySettings, val wireless: Boolean, val permissionsReady: Boolean,
    val identityReady: Boolean, val checking: Boolean = false, val validSettings: Boolean = true,
    val phase: SessionPhase? = null, val started: Boolean = false, val failure: Boolean = false,
    val iphoneSelection: IphoneSelection = IphoneSelection.NoIphone, val autoStartPaused: Boolean = false,
    val sharedWifiReady: Boolean = true, val currentWifi: String? = null,
)

fun presentHome(value: HomeSnapshot): HomePresentation {
    val iphoneReady = !value.wireless || value.iphoneSelection.address != null
    val checklist = buildList {
        if (!value.permissionsReady) add(SetupItem(R.string.ui_permissions, R.string.ui_allow, SetupAction.PERMISSIONS))
        if (!value.identityReady && !value.checking) add(SetupItem(R.string.direlay_identity_required, R.string.direlay_import, SetupAction.IDENTITY))
        if (!value.validSettings) add(SetupItem(R.string.ui_check_settings, R.string.direlay_settings, SetupAction.SETTINGS))
        if (!value.sharedWifiReady) add(SetupItem(
            if (value.currentWifi == null) R.string.shared_wifi_unreadable else R.string.shared_wifi_not_saved,
            if (value.currentWifi == null) R.string.shared_wifi_use else R.string.shared_wifi_save_current, SetupAction.SHARED_WIFI))
        if (!iphoneReady) {
            val text = when (value.iphoneSelection) {
                is IphoneSelection.SelectedUnpaired -> R.string.iphone_unpaired
                is IphoneSelection.MultipleCandidates,
                is IphoneSelection.MultipleConnected -> R.string.iphone_multiple
                else -> R.string.iphone_pair
            }
            add(SetupItem(text, if (text == R.string.iphone_pair) R.string.ui_pair else R.string.iphone_choose,
                if (text == R.string.iphone_pair) SetupAction.IPHONE else SetupAction.CHOOSE_IPHONE))
        }
    }
    val ready = value.permissionsReady && value.identityReady && value.validSettings && value.sharedWifiReady && !value.checking
    val active = value.phase == SessionPhase.ACTIVE
    val running = active || value.phase in setOf(SessionPhase.CONNECTING, SessionPhase.RECONNECTING) || value.started
    val (headline, tone) = when {
        value.failure -> R.string.ui_error to StatusTone.ERROR
        !ready -> R.string.ui_setup_needed to StatusTone.ATTENTION
        active -> R.string.ui_running to StatusTone.RUNNING
        value.phase in setOf(SessionPhase.CONNECTING, SessionPhase.RECONNECTING) -> R.string.ui_connecting to StatusTone.PROGRESS
        value.started -> R.string.ui_waiting to StatusTone.PROGRESS
        !iphoneReady -> R.string.ui_setup_needed to StatusTone.ATTENTION
        else -> R.string.ui_ready to StatusTone.IDLE
    }
    val subline = when {
        value.failure -> R.string.ui_retry_hint
        value.checking -> R.string.direlay_checking_identity
        !ready -> R.string.ui_setup_hint
        active && value.settings.bothOutputs -> R.string.ui_showing_both
        active && value.settings.carlinkOutput -> R.string.ui_showing_carlink
        active -> R.string.ui_showing_browser
        value.phase in setOf(SessionPhase.CONNECTING, SessionPhase.RECONNECTING) -> R.string.ui_connecting_hint
        value.started -> if (value.wireless) R.string.ui_iphone_pairing else R.string.ui_usb_hint
        value.settings.autoStartWhenReady && value.autoStartPaused && !running -> R.string.ui_automatic_paused
        else -> R.string.ui_start_hint
    }
    return HomePresentation(headline, subline, tone, listOf(
        if (value.settings.carlinkOutput) R.string.ui_carlink else R.string.ui_browser,
        if (value.wireless) R.string.direlay_wireless else R.string.ui_usb,
        value.settings.wirelessNetwork.choice.label,
    ), checklist, ready && iphoneReady, running)
}
