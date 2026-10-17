// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import org.openxplay.direlay.R

@Composable fun AdminPageSettings(enabled: Boolean, urls: List<String>, onEnabled: (Boolean) -> Unit,
    onCopy: (String) -> Unit, onReset: suspend () -> Unit) {
    var confirm by remember { mutableStateOf(false) }
    var reset by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    SmallHeading(stringResource(R.string.admin_page))
    SettingsRow(stringResource(R.string.admin_page), stringResource(R.string.admin_page_hint), R.drawable.symbol_settings,
        trailing = { LabeledSwitch(stringResource(R.string.admin_page), enabled, onEnabled) })
    urls.forEach { url ->
        Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
            Text(url, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
            IconButton(onClick = { onCopy(url) }) { Symbol(R.drawable.symbol_copy, stringResource(R.string.admin_copy_url)) }
        }
    }
    TextButton(onClick = { confirm = true }) { Text(stringResource(R.string.admin_reset_password)) }
    if (reset) Text(stringResource(R.string.admin_password_reset), color = MaterialTheme.colorScheme.primary)
    if (failed) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
    if (confirm) AlertDialog(onDismissRequest = { confirm = false },
        title = { Text(stringResource(R.string.admin_reset_password)) },
        text = { Text(stringResource(R.string.admin_reset_confirm)) },
        confirmButton = { TextButton(onClick = {
            confirm = false
            scope.launch {
                runCatching { onReset() }.onSuccess { reset = true; failed = false }.onFailure { failed = true }
            }
        }) { Text(stringResource(R.string.admin_reset_password)) } },
        dismissButton = { TextButton(onClick = { confirm = false }) { Text(stringResource(android.R.string.cancel)) } })
}
