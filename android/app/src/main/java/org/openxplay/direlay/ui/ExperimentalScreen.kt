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
        experimentalSections.forEach { section ->
            section.options.forEachIndexed { index, option ->
                ExperimentalOptionGroup(state, option, header = section.version.takeIf { index == 0 })
            }
        }
        if (state.savingError) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        if ("experimental" in state.errorPages(wireless))
            Text(stringResource(R.string.settings_advanced_error), color = MaterialTheme.colorScheme.error)
    }
}

@Composable private fun ExperimentalOptionGroup(state: SettingsEditorState, option: ExperimentalOption, header: String?) {
    val options = state.draft.value.experimental
    when (option) {
        ExperimentalOption.CARLINK_WIFI_RECOVERY -> SettingsToggleGroup(stringResource(R.string.experimental_carlink_wifi_recovery), options.carlinkWifiRecovery,
            R.drawable.symbol_wifi, IconHue.NETWORK,
            footer = stringResource(R.string.experimental_carlink_wifi_recovery_hint), enabled = options.active, header = header) { enabled ->
            state.changeExperimental { it.copy(carlinkWifiRecovery = enabled) }
        }
        ExperimentalOption.SEND_BLUETOOTH_ADDRESS -> {
            SettingsToggleGroup(stringResource(R.string.experimental_send_address), options.sendBluetoothAddress,
                R.drawable.symbol_bluetooth, IconHue.BLUETOOTH, footer = stringResource(R.string.experimental_scope_carlink) + "\n\n" + stringResource(R.string.experimental_send_address_hint),
                enabled = options.active, header = header) { enabled ->
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
        }
    }
}
