// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.settings.DisplayMode

@Composable
fun DeveloperOverrideContent(visible: Boolean, content: @Composable ColumnScope.() -> Unit) {
    val ms = if (LocalRelayMotion.current) 220 else 0
    AnimatedVisibility(visible, enter = expandVertically(tween(ms), expandFrom = Alignment.Top), exit = shrinkVertically(tween(ms), shrinkTowards = Alignment.Top)) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp), content = content)
    }
}

@Composable
fun hevcSummary(draft: RelaySettings) = stringResource(listOf(R.string.carlink_hevc_stream_only,
    R.string.carlink_hevc_off, R.string.carlink_hevc_signal)[draft.carlinkHevc.ordinal])

@Composable
fun developerDisplaySummary(draft: RelaySettings) = when (draft.displayMode) {
    DisplayMode.AUTOMATIC -> stringResource(R.string.display_automatic)
    DisplayMode.RELAY_SCREEN -> stringResource(R.string.display_relay_screen)
    DisplayMode.FIXED -> draft.display?.let { "${it.width} × ${it.height} · ${it.density} dpi" }
        ?: stringResource(R.string.direlay_custom)
}

@Composable
fun connectionModeSummary(draft: RelaySettings) = stringResource(when (draft.connectionMode) {
    ConnectionMode.USB -> R.string.direlay_usb
    ConnectionMode.WIRELESS -> R.string.direlay_wireless
    ConnectionMode.LAST_USED -> R.string.direlay_last_used
})

@Composable
fun microphoneSummary(draft: RelaySettings) = stringResource(when (draft.microphoneSource) {
    MicrophoneSource.HEAD_UNIT -> R.string.direlay_head_unit
    MicrophoneSource.THIS_DEVICE -> R.string.direlay_this_device
})

@Composable
fun hotspotSecuritySummary(draft: RelaySettings) = stringResource(when (draft.wirelessNetwork.security) {
    ManualHotspotSecurity.OPEN -> R.string.dev_security_open
    ManualHotspotSecurity.WPA2 -> R.string.dev_security_wpa2
    ManualHotspotSecurity.WPA3_TRANSITION -> R.string.dev_security_mixed
    ManualHotspotSecurity.WPA3 -> R.string.dev_security_wpa3
})

@Composable
fun mfiTargetSummary(draft: RelaySettings) = stringResource(when (draft.mfiTarget) {
    MfiTarget.LOCAL -> R.string.dev_local
    MfiTarget.REMOTE -> R.string.dev_remote
    MfiTarget.USB_CH341 -> R.string.dev_usb_i2c
    MfiTarget.I2C -> R.string.dev_linux_i2c
})
