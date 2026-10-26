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
        SettingsToggleGroup(stringResource(R.string.experimental_wireless_recovery), options.recoverWirelessNetwork,
            R.drawable.symbol_phone, IconHue.IPHONE,
            header = stringResource(R.string.experimental_iphone_group),
            footer = stringResource(R.string.experimental_scope_both) + "\n\n" + stringResource(R.string.experimental_wireless_recovery_hint),
            enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(recoverWirelessNetwork = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_isolate_network), options.isolateCarlinkNetwork,
            R.drawable.symbol_wifi, IconHue.CAR,
            header = stringResource(R.string.experimental_head_unit_group),
            footer = stringResource(R.string.experimental_scope_carlink) + "\n\n" + stringResource(R.string.experimental_isolate_network_hint),
            enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(isolateCarlinkNetwork = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_wait_for_carlink), options.waitForCarlink,
            R.drawable.symbol_sync, IconHue.CAR,
            footer = stringResource(R.string.experimental_wait_for_carlink_hint),
            enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(waitForCarlink = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_remember_subscription), options.rememberSubscription,
            R.drawable.symbol_bluetooth, IconHue.BLUETOOTH,
            footer = stringResource(R.string.experimental_scope_carlink) + "\n\n" + stringResource(R.string.experimental_carlink_hint) + "\n\n" + stringResource(R.string.experimental_remember_subscription_hint),
            enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(rememberSubscription = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_keep_scanning), options.keepScanning,
            R.drawable.symbol_scan_search, IconHue.BLUETOOTH, footer = stringResource(R.string.experimental_scope_carlink) + "\n\n" + stringResource(R.string.experimental_keep_scanning_hint),
            enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(keepScanning = enabled) }
        }
        SettingsToggleGroup(stringResource(R.string.experimental_record_advertisements), options.recordAdvertisements,
            R.drawable.symbol_activity, IconHue.DEVELOPER, footer = stringResource(R.string.experimental_scope_carlink) + "\n\n" + stringResource(R.string.experimental_record_advertisements_hint),
            enabled = options.active) { enabled ->
            state.changeExperimental { it.copy(recordAdvertisements = enabled) }
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
