// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.network.*

val LocalNetworkReadings = compositionLocalOf { NetworkReadings() }

@Composable fun NetworkReadinessDialog(pending: NetworkReadiness?, startAnyway: () -> Unit, cancel: () -> Unit) {
    if (pending == null) return
    val context = LocalContext.current
    val opener = remember(context) { NetworkSettingsLauncher(context) }
    AlertDialog(onDismissRequest = cancel,
        title = { Text(stringResource(R.string.network_readiness_title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                pending.problems.forEach { problem ->
                    Text(stringResource(problem.problem()))
                    TextButton(onClick = { opener.open(problem == NetworkRequirement.HOTSPOT) },
                        modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(problem.settingsLabel())) }
                }
            }
        },
        confirmButton = { TextButton(onClick = startAnyway) { Text(stringResource(R.string.network_start_anyway)) } },
        dismissButton = { TextButton(onClick = cancel) { Text(stringResource(android.R.string.cancel)) } })
}

/** Static reminders belong only under choices whose platform state cannot be read. */
@Composable internal fun NetworkChoiceReminder(requirement: NetworkRequirement) {
    networkChoiceReminder(requirement, LocalNetworkReadings.current)?.let { Hint(stringResource(it)) }
}
