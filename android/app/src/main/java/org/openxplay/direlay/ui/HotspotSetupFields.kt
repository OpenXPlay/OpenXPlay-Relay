// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-31
package org.openxplay.direlay.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.PersistableBundle
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.shilapi.xcertplay.orchestration.ManualHotspotSecurity
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.HotspotSetupMethod
import org.openxplay.direlay.settings.WirelessNetworkSettings
import org.openxplay.direlay.ui.design.RelayTokens

internal fun copyHotspotValue(context: Context, label: String, value: String, sensitive: Boolean) {
    val clip = ClipData.newPlainText(label, value)
    if (sensitive) clip.description.extras = PersistableBundle().apply {
        putBoolean("android.content.extra.IS_SENSITIVE", true)
    }
    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(clip)
}

@Composable internal fun HotspotSetupFields(network: WirelessNetworkSettings, errors: Map<String, Int>,
    opener: NetworkSettingsOpener, change: (WirelessNetworkSettings) -> Unit) {
    Column(Modifier.selectableGroup()) {
        RadioOptionRow(RadioOption(R.string.hotspot_copy_method, R.string.hotspot_copy_hint),
            network.hotspotSetupMethod == HotspotSetupMethod.COPY_TO_SYSTEM) {
            change(network.copy(hotspotSetupMethod = HotspotSetupMethod.COPY_TO_SYSTEM))
        }
        RadioOptionRow(RadioOption(R.string.hotspot_existing_method, R.string.hotspot_existing_hint),
            network.hotspotSetupMethod == HotspotSetupMethod.EXISTING) {
            change(network.copy(hotspotSetupMethod = HotspotSetupMethod.EXISTING))
        }
    }
    if (network.hotspotSetupMethod == HotspotSetupMethod.EXISTING) {
        SettingField(stringResource(R.string.settings_hotspot_name), network.manualSsid, errors["ssid"]) {
            change(network.copy(manualSsid = it))
        }
        SettingField(stringResource(R.string.settings_hotspot_password), network.manualPassphrase, errors["password"], secret = true) {
            change(network.copy(manualPassphrase = it,
                security = if (it.isEmpty()) ManualHotspotSecurity.OPEN else ManualHotspotSecurity.WPA2))
        }
        TextButton(onClick = { opener.open(true) }) { Text(stringResource(R.string.settings_open_hotspot)) }
    } else {
        CopyHotspotField(stringResource(R.string.settings_hotspot_name), network.manualSsid,
            network.manualSsid, false, errors["ssid"], R.string.hotspot_copy_name, R.string.hotspot_name_copied, opener)
        CopyHotspotField(stringResource(R.string.settings_hotspot_password),
            if (network.manualPassphrase.isEmpty()) stringResource(R.string.hotspot_no_password)
            else "•".repeat(network.manualPassphrase.length), network.manualPassphrase, true,
            errors["password"], R.string.hotspot_copy_password, R.string.hotspot_password_copied, opener)
        val security = stringResource(when (network.security) {
            ManualHotspotSecurity.OPEN -> R.string.dev_security_open
            ManualHotspotSecurity.WPA2 -> R.string.dev_security_wpa2
            ManualHotspotSecurity.WPA3_TRANSITION -> R.string.dev_security_mixed
            ManualHotspotSecurity.WPA3 -> R.string.dev_security_wpa3
        })
        Text(stringResource(R.string.hotspot_system_options, security), style = RelayTokens.footnote,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable private fun CopyHotspotField(label: String, shown: String, value: String, sensitive: Boolean,
    error: Int?, action: Int, copiedAction: Int, opener: NetworkSettingsOpener) {
    val context = LocalContext.current
    var copied by rememberSaveable(value) { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(shown, fontFamily = FontFamily.Monospace, style = RelayTokens.body)
        error?.let { Text(stringResource(it), color = MaterialTheme.colorScheme.error, style = RelayTokens.footnote) }
        FilledTonalButton(onClick = {
            copyHotspotValue(context, label, value, sensitive)
            copied = true
            opener.open(true)
        }, Modifier.fillMaxWidth().heightIn(min = 48.dp), enabled = error == null && (sensitive || value.isNotBlank())) {
            Text(stringResource(if (copied) copiedAction else action))
        }
    }
}
