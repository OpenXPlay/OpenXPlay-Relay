// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings

@Composable
fun ExperimentalScreen(state: SettingsEditorState, back: () -> Unit, wireless: (RelaySettings) -> Boolean) {
    val options = state.draft.value.experimental
    DetailPage(stringResource(R.string.experimental_title), back) {
        Toggle(stringResource(R.string.experimental_use_options), options.active) { enabled ->
            state.changeExperimental { it.copy(active = enabled) }
        }
        Hint(stringResource(R.string.experimental_use_options_hint))
        SmallHeading(stringResource(R.string.experimental_carlink))
        Hint(stringResource(R.string.experimental_carlink_hint))
        Toggle(stringResource(R.string.experimental_remember_subscription), options.rememberSubscription, options.active) { enabled ->
            state.changeExperimental { it.copy(rememberSubscription = enabled) }
        }
        Hint(stringResource(R.string.experimental_remember_subscription_hint), enabled = options.active)
        Toggle(stringResource(R.string.experimental_keep_scanning), options.keepScanning, options.active) { enabled ->
            state.changeExperimental { it.copy(keepScanning = enabled) }
        }
        Hint(stringResource(R.string.experimental_keep_scanning_hint), enabled = options.active)
        Toggle(stringResource(R.string.experimental_record_advertisements), options.recordAdvertisements, options.active) { enabled ->
            state.changeExperimental { it.copy(recordAdvertisements = enabled) }
        }
        Hint(stringResource(R.string.experimental_record_advertisements_hint), enabled = options.active)
        Toggle(stringResource(R.string.experimental_send_address), options.sendBluetoothAddress, options.active) { enabled ->
            state.changeExperimental { it.copy(sendBluetoothAddress = enabled) }
        }
        Hint(stringResource(R.string.experimental_send_address_hint), enabled = options.active)
        SettingField(stringResource(R.string.experimental_bluetooth_address), options.bluetoothAddress,
            if (options.invalidAddress) R.string.experimental_address_error else null,
            enabled = options.active && options.sendBluetoothAddress) { address ->
            state.changeExperimental { it.copy(bluetoothAddress = address) }
        }
        if (state.savingError) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        if ("experimental" in state.errorPages(wireless))
            Text(stringResource(R.string.settings_advanced_error), color = MaterialTheme.colorScheme.error)
    }
}
