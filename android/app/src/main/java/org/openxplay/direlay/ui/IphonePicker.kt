// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.bluetooth.BluetoothDevice
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.provider.Settings
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.openxplay.direlay.R
import org.openxplay.direlay.bluetooth.BondedDevice
import org.openxplay.direlay.bluetooth.BondedDevices
import org.openxplay.direlay.bluetooth.IphoneSelection

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun IphonePickerSetting(choice: String?, source: BondedDevices, initiallyOpen: Boolean,
    choose: (String?) -> Unit) {
    val context = LocalContext.current
    var visible by rememberSaveable { mutableStateOf(initiallyOpen) }
    var revision by remember { mutableIntStateOf(0) }
    var devices by remember { mutableStateOf(emptyList<BondedDevice>()) }
    var localName by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(source, visible, revision) {
        val snapshot = withContext(Dispatchers.IO) { source.read() to source.localName() }
        devices = snapshot.first
        localName = snapshot.second
    }
    DisposableEffect(context, visible) {
        val registered = visible
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) { revision++ }
        }
        if (registered) {
            val filter = IntentFilter().apply {
                addAction(BluetoothDevice.ACTION_BOND_STATE_CHANGED)
                addAction(BluetoothDevice.ACTION_ACL_CONNECTED)
                addAction(BluetoothDevice.ACTION_ACL_DISCONNECTED)
            }
            // Bluetooth broadcasts originate from the privileged Bluetooth process.
            ContextCompat.registerReceiver(context, receiver, filter, ContextCompat.RECEIVER_EXPORTED)
        }
        onDispose { if (registered) context.unregisterReceiver(receiver) }
    }
    val result = IphoneSelection.select(choice, devices)
    val resolved = devices.firstOrNull { it.address == result.address }
    val unnamed = stringResource(R.string.iphone_unnamed)
    val value = if (choice == null) {
        resolved?.let { stringResource(R.string.iphone_automatic_name, it.name?.takeIf(String::isNotBlank) ?: unnamed) }
            ?: stringResource(R.string.iphone_automatic)
    } else devices.firstOrNull { it.address.equals(choice, ignoreCase = true) }?.name?.takeIf(String::isNotBlank) ?: unnamed
    SettingsRow(stringResource(R.string.iphone_title), stringResource(R.string.iphone_hint),
        R.drawable.symbol_phone, { visible = true }, trailing = {
            Text(value, Modifier.widthIn(max = 160.dp), style = MaterialTheme.typography.bodyMedium)
            Symbol(R.drawable.symbol_chevron, null)
        })
    if (visible) ModalBottomSheet(onDismissRequest = { visible = false }) {
        Column(Modifier.fillMaxWidth().heightIn(max = 640.dp).padding(horizontal = 16.dp)) {
            Heading(stringResource(R.string.iphone_choose))
            LazyColumn(Modifier.weight(1f, fill = false).selectableGroup()) {
                item {
                    IphoneRadioRow(stringResource(R.string.iphone_automatic_recommended), emptyList(),
                        R.drawable.symbol_sync, choice == null) { choose(null); visible = false }
                }
                items(devices, key = { it.address }) { device ->
                    val labels = buildList {
                        if (device.connected) add(stringResource(R.string.iphone_connected))
                        if (device.strong) add(stringResource(R.string.iphone_likely))
                    }
                    IphoneRadioRow(device.name?.takeIf(String::isNotBlank) ?: unnamed, labels,
                        when { device.phoneClass -> R.drawable.symbol_phone
                            device.carClass -> R.drawable.symbol_car
                            else -> R.drawable.symbol_link },
                        device.address.equals(choice, ignoreCase = true)) {
                        choose(device.address)
                        visible = false
                    }
                }
            }
            HorizontalDivider()
            TextButton(onClick = { context.startActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS)) },
                modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.iphone_pair_new)) }
            Text(stringResource(R.string.iphone_pair_hint, localName ?: stringResource(R.string.direlay_this_device)),
                Modifier.padding(bottom = 24.dp), style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable private fun IphoneRadioRow(name: String, labels: List<String>, icon: Int,
    selected: Boolean, select: () -> Unit) {
    // Merge the name and evidence labels into the radio row's accessibility announcement.
    Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}
        .selectable(selected, role = Role.RadioButton, onClick = select).padding(vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically) {
        Symbol(icon, null)
        Column(Modifier.weight(1f).padding(horizontal = 16.dp)) {
            Text(name, style = MaterialTheme.typography.bodyLarge)
            if (labels.isNotEmpty()) Text(labels.joinToString(" · "), style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        RadioButton(selected, onClick = null)
    }
}
