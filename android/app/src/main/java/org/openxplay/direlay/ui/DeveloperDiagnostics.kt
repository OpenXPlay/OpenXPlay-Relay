// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R

@Composable
fun DeveloperDiagnostics(view: DeveloperView, sample: Boolean, setSample: (Boolean) -> Unit,
    openVehicle: () -> Unit, reconnect: () -> Unit, copy: (String) -> Unit) {
    CollapsibleCard("sample", stringResource(R.string.dev_sample_vehicle), menuToggleSummary(sample), initiallyExpanded = true) {
        Toggle(stringResource(R.string.dev_sample_vehicle), sample, setSample)
        Hint(stringResource(R.string.dev_sample_hint))
        SettingsRow(stringResource(R.string.vehicle_title), stringResource(R.string.dev_sample_open), R.drawable.symbol_car, openVehicle)
    }
    CollapsibleCard("runtime", stringResource(R.string.dev_runtime), view.rows.firstOrNull()?.let {
        if (it.valueKey == 0) it.value else stringResource(it.valueKey, *it.valueArguments.toTypedArray())
    } ?: stringResource(R.string.dev_unavailable), initiallyExpanded = false) {
        KeyValue(stringResource(R.string.dev_connected_clients), stringResource(R.string.dev_unavailable))
        view.rows.forEach { row -> KeyValue(stringResource(row.label, row.argument), if (row.valueKey == 0) row.value else stringResource(row.valueKey, *row.valueArguments.toTypedArray())) }
        OutlinedButton(onClick = reconnect, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.direlay_carlink_reconnect)) }
    }
    CollapsibleCard("urls", stringResource(R.string.dev_urls), view.urls.firstOrNull() ?: stringResource(R.string.dev_unavailable), initiallyExpanded = false) {
        view.urls.forEach { KeyValue(stringResource(R.string.dev_url), it, copy) }
    }
}
