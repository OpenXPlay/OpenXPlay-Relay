// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-11-04
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.WirelessNetworkChoice
import org.openxplay.direlay.settings.WirelessNetworkSettings
import org.openxplay.direlay.ui.design.*

@Composable fun LowDataModeDialog(network: WirelessNetworkSettings, confirm: () -> Unit, dismiss: () -> Unit) {
    val name = when (network.choice) {
        WirelessNetworkChoice.PHONE_HOTSPOT -> network.manualSsid
        WirelessNetworkChoice.EXISTING_WIFI -> stringResource(R.string.low_data_network_car)
        WirelessNetworkChoice.WIFI_DIRECT -> stringResource(R.string.low_data_network_direct)
    }
    RelayDecisionDialog(onDismissRequest = dismiss,
        shape = RelayTokens.card, containerColor = MaterialTheme.colorScheme.surface,
        icon = { IconTile(R.drawable.symbol_wifi, IconHue.NETWORK) },
        title = { Text(stringResource(R.string.low_data_confirm_title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.low_data_confirm_intro))
                Text(stringResource(R.string.low_data_confirm_steps_named, name))
            }
        },
        confirmButton = { TextButton(onClick = confirm) { Text(stringResource(R.string.low_data_confirm_done)) } },
        dismissButton = { TextButton(onClick = dismiss) { Text(stringResource(R.string.low_data_confirm_back)) } })
}
