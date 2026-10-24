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
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R

@Composable fun AdminPageSettings(enabled: Boolean, urls: List<String>, onEnabled: (Boolean) -> Unit,
    onCopy: (String) -> Unit, onReset: suspend () -> Unit) {
    var confirm by remember { mutableStateOf(false) }
    var reset by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    InsetGroup(footer = stringResource(R.string.admin_page_hint)) {
        InsetToggleRow(R.drawable.symbol_lock, IconHue.PERMISSIONS, stringResource(R.string.admin_page), enabled, onEnabled)
        urls.forEach { url ->
            InsetDivider()
            InsetRow(R.drawable.symbol_globe, IconHue.NETWORK, stringResource(R.string.admin_copy_url), url,
                onClick = { onCopy(url) }, trailing = { Symbol(R.drawable.symbol_copy, null) })
        }
        InsetDivider()
        InsetRow(R.drawable.symbol_key_round, IconHue.PERMISSIONS, stringResource(R.string.admin_reset_password),
            onClick = { confirm = true })
        if (reset || failed) Column(Modifier.padding(16.dp)) {
            if (reset) Text(stringResource(R.string.admin_password_reset), color = MaterialTheme.colorScheme.primary)
            if (failed) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        }
    }
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
