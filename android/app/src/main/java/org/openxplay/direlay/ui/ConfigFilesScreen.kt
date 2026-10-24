// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.config.ConfigFiles
import org.openxplay.direlay.config.ConfigFileResult
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable fun ConfigFilesScreen(files: ConfigFiles) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var privateData by rememberSaveable { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var available by remember { mutableStateOf(false) }
    var confirmExport by rememberSaveable { mutableStateOf(false) }
    var importUri by rememberSaveable { mutableStateOf<Uri?>(null) }
    var result by remember { mutableStateOf<ConfigFileResult?>(null) }
    var exported by remember { mutableStateOf(false) }
    LaunchedEffect(files, result) { available = files.hasPrivateData() }
    val create = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/json")) { uri ->
        if (uri != null) scope.launch {
            busy = true
            try { exported = true; result = files.export(uri, privateData) } finally { busy = false }
        }
    }
    val open = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> importUri = uri }
    fun export() {
        val kind = if (privateData) "privacy" else "user"
        create.launch("openxplay-relay-$kind-${SimpleDateFormat("yyyyMMdd-HHmm", Locale.ROOT).format(Date())}.json")
    }
    for (privacy in listOf(false, true)) InsetGroup(footer = stringResource(if (privacy) R.string.config_privacy_hint else R.string.config_user_hint)) {
        InsetRow(R.drawable.symbol_code, IconHue.LOGS, stringResource(if (privacy) R.string.config_privacy else R.string.config_user))
        InsetDivider()
        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (privacy && !available) Text(stringResource(R.string.config_empty))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(enabled = !busy && (!privacy || available), onClick = {
                    privateData = privacy
                    if (privacy) confirmExport = true else export()
                }) { Text(stringResource(R.string.config_export)) }
                TextButton(enabled = !busy, onClick = {
                    privateData = privacy
                    open.launch(arrayOf("application/json", "text/plain", "application/octet-stream"))
                }) { Text(stringResource(R.string.config_import)) }
            }
        }
    }
    if (confirmExport) AlertDialog(onDismissRequest = { confirmExport = false },
        title = { Text(stringResource(R.string.config_privacy)) }, text = { Text(stringResource(R.string.config_privacy_warning)) },
        confirmButton = { TextButton(onClick = { confirmExport = false; export() }) { Text(stringResource(R.string.config_export_confirm)) } },
        dismissButton = { TextButton(onClick = { confirmExport = false }) { Text(stringResource(android.R.string.cancel)) } })
    importUri?.let { uri -> AlertDialog(onDismissRequest = { importUri = null },
        title = { Text(stringResource(R.string.config_import)) }, text = { Text(stringResource(R.string.config_replace)) },
        confirmButton = { TextButton(onClick = {
            importUri = null
            scope.launch {
                busy = true
                try { exported = false; result = files.import(uri, privateData) } finally { busy = false }
            }
        }) { Text(stringResource(android.R.string.ok)) } },
        dismissButton = { TextButton(onClick = { importUri = null }) { Text(stringResource(android.R.string.cancel)) } }) }
    result?.let { report ->
        val message = if (report.errors.isEmpty()) {
            if (exported) stringResource(R.string.config_exported) else stringResource(R.string.config_imported, report.changedCount) +
                if (report.ignoredKeys.isEmpty()) "" else "\n" + stringResource(R.string.config_ignored, report.ignoredKeys.joinToString(", "))
        } else report.errors.entries.joinToString("\n") { (key, code) ->
            val resource = when (code) {
                "wrong-kind" -> if (privateData) R.string.config_expected_privacy else R.string.config_expected_user
                "too-large" -> R.string.config_too_large
                "bad-version" -> R.string.config_version
                "not-json" -> R.string.config_not_json
                "io" -> R.string.config_io
                else -> R.string.config_invalid
            }
            context.getString(resource, key)
        }
        AlertDialog(onDismissRequest = { result = null }, title = { Text(stringResource(R.string.config_title)) },
            text = { Text(message) }, confirmButton = { TextButton(onClick = { result = null }) { Text(stringResource(android.R.string.ok)) } })
    }
}
