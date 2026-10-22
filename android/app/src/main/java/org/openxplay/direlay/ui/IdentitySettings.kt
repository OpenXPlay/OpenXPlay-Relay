// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.res.stringResource
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.direlay.R
import org.openxplay.direlay.storage.MaterialStatus

fun identityStatusKey(status: MaterialStatus) = when (status) {
    MaterialStatus.Installed -> R.string.direlay_installed
    MaterialStatus.Missing -> R.string.direlay_missing
    is MaterialStatus.Invalid -> R.string.dev_invalid_identity
}

@Composable
fun IdentitySettings(state: SettingsEditorState, view: DeveloperView, copy: (String) -> Unit,
    import: (Boolean) -> Unit, remove: (Boolean) -> Unit) {
    var draft by state.draft
    var confirm by remember { mutableStateOf<Boolean?>(null) }
    val ms = if (LocalRelayMotion.current) 220 else 0
    CollapsibleCard("tls", stringResource(R.string.direlay_tls), view.fingerprint, initiallyExpanded = true) {
        KeyValue(stringResource(R.string.dev_fingerprint), view.fingerprint, copy)
        Hint(stringResource(R.string.dev_tls_hint))
        OutlinedButton(onClick = { import(false) }) { Text(stringResource(R.string.dev_import_tls)) }
        TextButton(onClick = { confirm = false }) { Text(stringResource(R.string.dev_remove_tls)) }
    }
    CollapsibleCard("identity", stringResource(R.string.direlay_identity), stringResource(view.identity), initiallyExpanded = false) {
        Hint(stringResource(R.string.direlay_identity_hint))
        KeyValue(stringResource(R.string.direlay_identity), stringResource(view.identity))
        OutlinedButton(onClick = { import(true) }) { Text(stringResource(R.string.direlay_manage_identity)) }
        TextButton(onClick = { confirm = true }) { Text(stringResource(R.string.dev_remove_identity)) }
    }
    CollapsibleCard("mfi", stringResource(R.string.direlay_mfi_target), mfiTargetSummary(draft), initiallyExpanded = false) {
        Options(MfiTarget.entries.map { stringResource(when (it) { MfiTarget.LOCAL -> R.string.dev_local; MfiTarget.REMOTE -> R.string.dev_remote; MfiTarget.USB_CH341 -> R.string.dev_usb_i2c; MfiTarget.I2C -> R.string.dev_linux_i2c }) }, MfiTarget.entries.indexOf(draft.mfiTarget)) { draft = draft.copy(mfiTarget = MfiTarget.entries[it]) }
        AnimatedVisibility(draft.mfiTarget == MfiTarget.I2C, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            SettingField(stringResource(R.string.direlay_i2c_path), draft.i2cPath) { draft = draft.copy(i2cPath = it) }
        }
        AnimatedVisibility(draft.mfiTarget == MfiTarget.REMOTE, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            Column {
                SettingField(stringResource(R.string.direlay_remote_server), draft.remoteServer) { draft = draft.copy(remoteServer = it) }
                SettingField(stringResource(R.string.direlay_remote_token), draft.remoteToken, secret = true) { draft = draft.copy(remoteToken = it) }
            }
        }
    }
    confirm?.let { identity -> AlertDialog(onDismissRequest = { confirm = null },
        title = { Text(if (identity) stringResource(R.string.dev_remove_identity_question) else stringResource(R.string.dev_remove_tls_question)) },
        confirmButton = { TextButton(onClick = { remove(identity); confirm = null }) { Text(stringResource(R.string.direlay_remove)) } },
        dismissButton = { TextButton(onClick = { confirm = null }) { Text(stringResource(android.R.string.cancel)) } }) }
}
