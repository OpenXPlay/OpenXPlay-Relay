// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-29
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.network.RadioChoice
import org.openxplay.direlay.network.RadioConflict
import org.openxplay.direlay.network.RadioConflictKind
import org.openxplay.direlay.settings.CarlinkConnectionPolicy
import org.openxplay.direlay.settings.ConnectionMode
import org.openxplay.direlay.settings.RelayOutput
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.settings.WirelessNetworkChoice
import org.openxplay.direlay.ui.design.*

/** Offers the iPhone connections this phone can carry; the first one is preselected. */
@Composable fun RadioConflictDialog(conflict: RadioConflict, choose: (RadioChoice) -> Unit, keep: () -> Unit) {
    var selected by remember(conflict) { mutableStateOf(conflict.choices.first()) }
    RelayDecisionDialog(onDismissRequest = keep,
        shape = RelayTokens.card, containerColor = MaterialTheme.colorScheme.surface,
        icon = { IconTile(R.drawable.symbol_wifi, IconHue.NETWORK) },
        title = { Text(stringResource(R.string.radio_conflict_title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(when (conflict.kind) {
                    RadioConflictKind.CAR_WIFI_WITH_HOTSPOT -> R.string.radio_conflict_car_wifi
                    RadioConflictKind.HOTSPOT_STOPS_WIFI -> R.string.radio_conflict_hotspot
                }))
                Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    conflict.choices.forEach { choice ->
                        RadioOptionRow(radioChoiceOption(choice), selected == choice) { selected = choice }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = { choose(selected) }) { Text(stringResource(R.string.radio_conflict_use)) } },
        dismissButton = { TextButton(onClick = keep) { Text(stringResource(R.string.radio_conflict_keep)) } })
}

internal fun radioChoiceOption(choice: RadioChoice) = when (choice) {
    RadioChoice.PHONE_HOTSPOT -> networkOption(WirelessNetworkChoice.PHONE_HOTSPOT)
    RadioChoice.SHARED_WIFI -> networkOption(WirelessNetworkChoice.EXISTING_WIFI)
    RadioChoice.BROWSER -> outputOptions[0]
    RadioChoice.USB -> connectionOptions[1]
}

/** The settings page where the choice lands, so missing hotspot or car-hotspot details show at once. */
internal fun radioChoiceRoute(choice: RadioChoice) = when (choice) {
    RadioChoice.PHONE_HOTSPOT -> "settings/connection/network#${WirelessNetworkChoice.PHONE_HOTSPOT.name}"
    RadioChoice.SHARED_WIFI -> "settings/connection/network#sharedWifi"
    RadioChoice.BROWSER -> "settings/output#output"
    RadioChoice.USB -> "settings/connection#connection"
}

internal fun radioChoiceSettings(draft: RelaySettings, choice: RadioChoice): RelaySettings = when (choice) {
    RadioChoice.PHONE_HOTSPOT -> draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(choice = WirelessNetworkChoice.PHONE_HOTSPOT))
    RadioChoice.SHARED_WIFI -> draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(choice = WirelessNetworkChoice.EXISTING_WIFI))
    RadioChoice.BROWSER -> draft.copy(outputs = setOf(RelayOutput.BROWSER))
    RadioChoice.USB -> CarlinkConnectionPolicy.enabling(draft, draft.copy(connectionMode = ConnectionMode.USB), wireless = false)
}
