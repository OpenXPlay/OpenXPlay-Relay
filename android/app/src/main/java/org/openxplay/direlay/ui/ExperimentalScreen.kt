// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings

@Composable
fun ExperimentalScreen(state: SettingsEditorState, back: () -> Unit, wireless: (RelaySettings) -> Boolean) {
    val options = state.draft.value.experimental
    DetailPage(stringResource(R.string.experimental_title), back) {
        SettingsToggleGroup(stringResource(R.string.experimental_use_options), options.active,
            R.drawable.symbol_settings_2, IconHue.DEVELOPER, footer = stringResource(R.string.experimental_use_options_hint)) { enabled ->
            state.changeExperimental { it.copy(active = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_screen_session_isolation), options.screenSessionIsolation,
            R.drawable.symbol_activity, IconHue.DEVELOPER,
            footer = stringResource(R.string.experimental_screen_session_isolation_hint), enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(screenSessionIsolation = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_screen_reconnect_reset), options.screenReconnectReset,
            R.drawable.symbol_activity, IconHue.DEVELOPER,
            footer = stringResource(R.string.experimental_screen_reconnect_reset_hint), enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(screenReconnectReset = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_screen_delivery_serialization), options.screenDeliverySerialization,
            R.drawable.symbol_activity, IconHue.DEVELOPER,
            footer = stringResource(R.string.experimental_screen_delivery_serialization_hint), enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(screenDeliverySerialization = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_remote_playback_proof), options.remotePlaybackProof,
            R.drawable.symbol_activity, IconHue.DEVELOPER,
            footer = stringResource(R.string.experimental_remote_playback_proof_hint), enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(remotePlaybackProof = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_cancel_recovered_retries), options.cancelRecoveredRetries,
            R.drawable.symbol_activity, IconHue.DEVELOPER,
            footer = stringResource(R.string.experimental_cancel_recovered_retries_hint), enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(cancelRecoveredRetries = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_startup_status), options.startupStatus,
            R.drawable.symbol_activity, IconHue.DEVELOPER,
            footer = stringResource(R.string.experimental_startup_status_hint), enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(startupStatus = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_startup_diagnostics), options.startupDiagnostics,
            R.drawable.symbol_activity, IconHue.DEVELOPER,
            footer = stringResource(R.string.experimental_startup_diagnostics_hint), enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(startupDiagnostics = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_network_observation_backoff), options.networkObservationBackoff,
            R.drawable.symbol_activity, IconHue.DEVELOPER,
            footer = stringResource(R.string.experimental_network_observation_backoff_hint), enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(networkObservationBackoff = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_send_address), options.sendBluetoothAddress,
            R.drawable.symbol_bluetooth, IconHue.BLUETOOTH, footer = stringResource(R.string.experimental_scope_carlink) + "\n\n" + stringResource(R.string.experimental_send_address_hint),
            enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(sendBluetoothAddress = enabled) }
        }
        SettingsGroup("address", R.drawable.symbol_bluetooth, IconHue.BLUETOOTH,
            stringResource(R.string.experimental_bluetooth_address), options.bluetoothAddress, initiallyExpanded = true) {
            SettingField(stringResource(R.string.experimental_bluetooth_address), options.bluetoothAddress,
                if (options.invalidAddress) R.string.experimental_address_error else null,
                enabled = options.active && options.sendBluetoothAddress) { address ->
                state.changeExperimental { it.copy(bluetoothAddress = address) }
            }
        }
        if (state.savingError) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        if ("experimental" in state.errorPages(wireless))
            Text(stringResource(R.string.settings_advanced_error), color = MaterialTheme.colorScheme.error)
    }
}
