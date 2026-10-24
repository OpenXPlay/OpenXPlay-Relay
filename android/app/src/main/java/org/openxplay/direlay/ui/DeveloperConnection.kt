// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.res.stringResource
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import com.shilapi.xcertplay.orchestration.isManualHotspotChannelCompatible
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*

@Composable
fun DeveloperConnection(state: SettingsEditorState, wireless: (RelaySettings) -> Boolean) {
    var draft by state.draft
    val overrides by state.developerDraft
    var confirmCarlink by remember { mutableStateOf(false) }
    var confirmUsb by remember { mutableStateOf(false) }
    if (confirmUsb) UsbConnectionDialog(confirm = {
        val next = draft.copy(connectionMode = ConnectionMode.USB)
        draft = CarlinkConnectionPolicy.enabling(draft, next, wireless(next))
        confirmUsb = false
    }, dismiss = { confirmUsb = false })
    if (confirmCarlink) AlertDialog(onDismissRequest = { confirmCarlink = false },
        title = { Text(stringResource(R.string.ui_carlink)) },
        text = { Text(stringResource(R.string.carlink_wifi_direct_confirm)) },
        confirmButton = { TextButton(onClick = {
            val next = draft.copy(bothOutputs = true)
            draft = CarlinkConnectionPolicy.enabling(draft, next, wireless(next))
            confirmCarlink = false
        }) { Text(stringResource(android.R.string.ok)) } },
        dismissButton = { TextButton(onClick = { confirmCarlink = false }) { Text(stringResource(android.R.string.cancel)) } })
    SettingsGroup("outputs", R.drawable.symbol_car, IconHue.CAR, stringResource(R.string.dev_outputs), stringResource(R.string.menu_outputs_summary, outputSummary(draft),
        draft.webPort?.toString() ?: stringResource(R.string.display_automatic)), initiallyExpanded = true) {
        SettingsToggleRow(icon = R.drawable.symbol_settings_2, tile = IconHue.DEVELOPER, title = stringResource(R.string.dev_web_port), checked = overrides.webPortEnabled, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(webPortEnabled = enabled) }
        }
        DeveloperOverrideContent(overrides.webPortEnabled) {
            EditorNumberField(state, "port", R.string.dev_port, overrides.webPort?.toString().orEmpty(),
                valid = { it.isEmpty() || it.toIntOrNull() in 1024..65535 }, enabled = overrides.active) { value ->
                state.changeDeveloper { it.copy(webPort = value.toIntOrNull()) }
            }
        }
        SettingsToggleRow(icon = R.drawable.symbol_settings_2, tile = IconHue.DEVELOPER, title = stringResource(R.string.direlay_carlink_model), checked = overrides.carlinkDeviceModelEnabled, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(carlinkDeviceModelEnabled = enabled) }
        }
        DeveloperOverrideContent(overrides.carlinkDeviceModelEnabled) {
            val stored = state.numberText["model"]
            val model = stored?.takeIf { "model" in state.invalidNumbers || it == overrides.carlinkDeviceModel }
                ?: overrides.carlinkDeviceModel
            SettingField(stringResource(R.string.direlay_carlink_model), model,
                if ("model" in state.invalidNumbers) R.string.settings_advanced_error else null, enabled = overrides.active) { text ->
                state.editNumber("model", text, { value -> value.isNotBlank() && value.toByteArray(Charsets.UTF_8).size <= 128 &&
                    value.none { Character.isISOControl(it) } }) { value ->
                    state.changeDeveloper { it.copy(carlinkDeviceModel = value) }
                }
            }
        }
        SettingsToggleRow(icon = R.drawable.symbol_settings_2, tile = IconHue.DEVELOPER, title = stringResource(R.string.dev_both), checked = draft.bothOutputs) {
            val next = draft.copy(bothOutputs = it)
            if (CarlinkConnectionPolicy.enabling(draft, next, wireless(next)) != next) confirmCarlink = true
            else draft = next
        }
        if (CarlinkConnectionPolicy.enabled(draft)) Hint(stringResource(R.string.carlink_wifi_direct_constraint))
        SettingsToggleRow(icon = R.drawable.symbol_settings_2, tile = IconHue.DEVELOPER, title = stringResource(R.string.direlay_local_preview), checked = overrides.localPreview, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(localPreview = enabled) }
        }
        SettingsToggleRow(icon = R.drawable.symbol_settings_2, tile = IconHue.DEVELOPER, title = stringResource(R.string.direlay_play_audio), checked = overrides.playCarPlayAudio, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(playCarPlayAudio = enabled) }
        }
    }
    SettingsGroup("connection", R.drawable.symbol_phone, IconHue.IPHONE, stringResource(R.string.direlay_connection_mode), "${connectionModeSummary(draft)} · ${hotspotSecuritySummary(draft)}", initiallyExpanded = false) {
        Options(ConnectionMode.entries.map { stringResource(when (it) { ConnectionMode.USB -> R.string.direlay_usb; ConnectionMode.WIRELESS -> R.string.direlay_wireless; ConnectionMode.LAST_USED -> R.string.direlay_last_used }) }, ConnectionMode.entries.indexOf(draft.connectionMode)) {
            val next = draft.copy(connectionMode = ConnectionMode.entries[it])
            if (needsUsbConfirmation(draft.connectionMode, next.connectionMode, wireless(draft))) confirmUsb = true
            else draft = CarlinkConnectionPolicy.enabling(draft, next, wireless(next))
        }
        EditorNumberField(state, "channel", R.string.direlay_hotspot_channel, draft.wirelessNetwork.channel.toString(),
            valid = { text -> text.toIntOrNull()?.let { it == 0 || isManualHotspotChannelCompatible(draft.wirelessNetwork.band, it) } == true }) {
            draft = draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(channel = it.toInt()))
        }
        SmallHeading(stringResource(R.string.dev_hotspot_security))
        Options(ManualHotspotSecurity.entries.map { stringResource(when (it) {
            ManualHotspotSecurity.OPEN -> R.string.dev_security_open
            ManualHotspotSecurity.WPA2 -> R.string.dev_security_wpa2
            ManualHotspotSecurity.WPA3_TRANSITION -> R.string.dev_security_mixed
            ManualHotspotSecurity.WPA3 -> R.string.dev_security_wpa3
        }) }, ManualHotspotSecurity.entries.indexOf(draft.wirelessNetwork.security)) {
            draft = draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(security = ManualHotspotSecurity.entries[it]))
        }
    }
    SettingsGroup("mic", R.drawable.symbol_bluetooth, IconHue.BLUETOOTH, stringResource(R.string.direlay_microphone_source), microphoneSummary(draft), initiallyExpanded = false) {
        SettingsToggleRow(icon = R.drawable.symbol_settings_2, tile = IconHue.DEVELOPER, title = stringResource(R.string.direlay_microphone_source), checked = overrides.microphoneSourceEnabled, enabled = overrides.active) { enabled ->
            state.changeDeveloper { it.copy(microphoneSourceEnabled = enabled) }
        }
        DeveloperOverrideContent(overrides.microphoneSourceEnabled) {
            Options(listOf(stringResource(R.string.direlay_head_unit), stringResource(R.string.direlay_this_device)),
                MicrophoneSource.entries.indexOf(overrides.microphoneSource), enabled = overrides.active) { index ->
                state.changeDeveloper { it.copy(microphoneSource = MicrophoneSource.entries[index]) }
            }
        }
    }
}
