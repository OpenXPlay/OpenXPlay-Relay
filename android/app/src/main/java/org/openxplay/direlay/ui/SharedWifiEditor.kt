// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.design.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun SharedWifiEditor(networks: List<SharedWifiNetwork>, current: String?, openEditor: Boolean,
    use: (String) -> Unit, change: (List<SharedWifiNetwork>) -> Unit) {
    var editing by rememberSaveable { mutableStateOf(openEditor && current != null && networks.none { it.ssid == current }) }
    var original by rememberSaveable { mutableStateOf<String?>(null) }
    var ssid by rememberSaveable { mutableStateOf(current?.takeIf { name -> networks.none { it.ssid == name } } ?: "") }
    var password by rememberSaveable { mutableStateOf("") }
    var reveal by rememberSaveable { mutableStateOf(false) }
    var attempted by rememberSaveable { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Hint(stringResource(R.string.direlay_existing_wifi_hint))
        if (current == null) Hint(stringResource(R.string.shared_wifi_unreadable))
        if (networks.isEmpty()) Text(stringResource(R.string.shared_wifi_empty))
        networks.forEach { entry ->
            Column {
                Text(entry.ssid, style = MaterialTheme.typography.titleMedium)
                if (entry.ssid == current) Surface(shape = RelayTokens.chip, color = MaterialTheme.colorScheme.primaryContainer,
                    contentColor = MaterialTheme.colorScheme.onPrimaryContainer) {
                    Text(stringResource(R.string.shared_wifi_connected), Modifier.padding(horizontal = 8.dp, vertical = 2.dp), style = RelayTokens.caption)
                }
                val editLabel = stringResource(R.string.shared_wifi_edit)
                val deleteLabel = stringResource(R.string.shared_wifi_delete)
                Row {
                    TextButton(modifier = Modifier.semantics { contentDescription = "$editLabel ${entry.ssid}" }, onClick = { original = entry.ssid; ssid = entry.ssid; password = entry.passphrase; attempted = false; editing = true }) { Text(stringResource(R.string.shared_wifi_edit)) }
                    TextButton(modifier = Modifier.semantics { contentDescription = "$deleteLabel ${entry.ssid}" }, onClick = { change(networks.filterNot { it.ssid == entry.ssid }) }) { Text(stringResource(R.string.shared_wifi_delete)) }
                    val useLabel = stringResource(R.string.shared_wifi_use)
                    if (current == null) TextButton(modifier = Modifier.semantics { contentDescription = "$useLabel ${entry.ssid}" }, onClick = { use(entry.ssid) }) { Text(stringResource(R.string.shared_wifi_use)) }
                }
            }
        }
        TextButton(enabled = networks.size < SharedWifiNetworks.MAX_SIZE, onClick = {
            original = null; ssid = current?.takeIf { name -> networks.none { it.ssid == name } } ?: ""
            password = ""; attempted = false; editing = true
        }) { Text(stringResource(R.string.shared_wifi_add)) }
    }
    if (editing) RelaySheet(onDismiss = { editing = false }) {
        val validSsid = runCatching { WirelessNetworkSettings.validateCredentials(ssid, "") }.isSuccess && networks.none { it.ssid == ssid && it.ssid != original }
        val validPassword = runCatching { WirelessNetworkSettings.validateCredentials("Wi-Fi", password) }.isSuccess
        Column(Modifier.imePadding(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Heading(stringResource(if (original == null) R.string.shared_wifi_add else R.string.shared_wifi_edit))
            SettingField(stringResource(R.string.direlay_network_ssid), ssid, if (attempted && !validSsid) R.string.settings_ssid_error else null) { ssid = it }
            SettingField(stringResource(R.string.direlay_network_passphrase), password, if (attempted && !validPassword) R.string.settings_password_error else null, secret = !reveal) { password = it }
            TextButton(onClick = { reveal = !reveal }) { Text(stringResource(if (reveal) R.string.shared_wifi_hide else R.string.shared_wifi_reveal)) }
            Row {
                TextButton(onClick = { editing = false }) { Text(stringResource(R.string.shared_wifi_cancel)) }
                Button(onClick = {
                    attempted = true
                    if (validSsid && validPassword) {
                        val entry = SharedWifiNetwork(ssid, password)
                        change(if (original == null) networks + entry else networks.map { if (it.ssid == original) entry else it })
                        editing = false
                    }
                }) { Text(stringResource(R.string.shared_wifi_save)) }
            }
        }
    }
}
