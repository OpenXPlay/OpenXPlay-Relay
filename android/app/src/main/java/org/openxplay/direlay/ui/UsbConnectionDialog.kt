// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.ui.design.*

@Composable fun UsbConnectionDialog(confirm: () -> Unit, dismiss: () -> Unit) {
    AlertDialog(onDismissRequest = dismiss,
        shape = RelayTokens.card, containerColor = MaterialTheme.colorScheme.surface,
        icon = { IconTile(R.drawable.symbol_usb, IconHue.IPHONE) },
        title = { Text(stringResource(R.string.usb_confirm_title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.usb_confirm_intro))
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(stringResource(R.string.usb_confirm_power_title), fontWeight = FontWeight.Bold)
                    Text(stringResource(R.string.usb_confirm_power))
                }
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(stringResource(R.string.usb_confirm_cable_title), fontWeight = FontWeight.Bold)
                    Text(stringResource(R.string.usb_confirm_cable))
                }
            }
        },
        confirmButton = { TextButton(onClick = confirm) { Text(stringResource(R.string.usb_confirm_use)) } },
        dismissButton = { TextButton(onClick = dismiss) { Text(stringResource(R.string.usb_confirm_keep)) } })
}
