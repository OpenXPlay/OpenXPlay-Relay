// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import android.widget.Toast
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.miracast.MiracastSystem
import org.openxplay.direlay.relay.RelayRuntime
import org.openxplay.direlay.settings.*
import org.openxplay.stream.cast.CastRenderer

@Composable fun WirelessDisplaySettings(draft: RelaySettings, runtime: RelayRuntime?,
    editor: SettingsEditorState, openCarScreen: () -> Unit, change: (RelaySettings) -> Unit) {
    if (!draft.dlnaOutput && !draft.miracastOutput) return
    val context = LocalContext.current
    var picker by rememberSaveable { mutableStateOf(false) }
    var overlayAllowed by remember { mutableStateOf(MiracastSystem.canDrawOverlays(context)) }
    LaunchedEffect(draft.miracastOutput) {
        while (draft.miracastOutput) {
            overlayAllowed = MiracastSystem.canDrawOverlays(context)
            delay(1000)
        }
    }
    if (picker) DlnaReceiverPicker(draft.dlnaRenderer, runtime, dismiss = { picker = false }) {
        change(draft.copy(dlnaRenderer = DlnaRendererChoice(it.udn, it.friendlyName))); picker = false
    }
    SettingsGroup("wireless-display", R.drawable.symbol_cast, IconHue.CAR, stringResource(R.string.wireless_display),
        wirelessDisplaySummary(draft), initiallyExpanded = true) {
        if (draft.dlnaOutput) {
            InsetGroup(footer = stringResource(R.string.dlna_network_hint)) {
                InsetRow(R.drawable.symbol_cast, IconHue.CAR, stringResource(R.string.dlna_receiver),
                    draft.dlnaRenderer?.name ?: stringResource(R.string.dlna_choose_receiver), onClick = { picker = true })
            }
        }
        if (draft.miracastOutput) {
            SettingsToggleGroup(stringResource(R.string.miracast_fill_screen), draft.miracastStretch, R.drawable.symbol_monitor, IconHue.PICTURE,
                footer = stringResource(R.string.miracast_fill_screen_hint)) { change(draft.copy(miracastStretch = it)) }
            NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.WIFI)
            TextButton(onClick = {
                if (!MiracastSystem.openCastSettings(context))
                    Toast.makeText(context, R.string.miracast_settings_unavailable, Toast.LENGTH_LONG).show()
            }) { Text(stringResource(R.string.miracast_open_settings)) }
            if (!overlayAllowed) InsetGroup(footer = stringResource(R.string.miracast_overlay_hint)) {
                InsetRow(R.drawable.symbol_shield_check, IconHue.PERMISSIONS, stringResource(R.string.miracast_allow_overlay),
                    value = stringResource(R.string.ui_allow), onClick = { context.startActivity(MiracastSystem.overlayPermissionIntent(context)) })
            }
            if (CarlinkConnectionPolicy.enabled(draft)) Hint(stringResource(R.string.miracast_carlink_hint))
            Hint(stringResource(R.string.miracast_wifi_direct_constraint))
        }
        if (draft.dlnaOutput || draft.miracastOutput) WirelessPictureSettings(editor, runtime, openCarScreen)
    }
}

@Composable private fun wirelessDisplaySummary(draft: RelaySettings): String =
    listOfNotNull(R.string.wireless_dlna.takeIf { draft.dlnaOutput }, R.string.wireless_miracast.takeIf { draft.miracastOutput }).map { stringResource(it) }.joinToString(" · ")
        .ifEmpty { stringResource(R.string.menu_off) }

@Composable internal fun DlnaReceiverPicker(choice: DlnaRendererChoice?, runtime: RelayRuntime?, dismiss: () -> Unit,
    choose: (CastRenderer) -> Unit) {
    var revision by remember { mutableIntStateOf(0) }
    var searching by remember { mutableStateOf(true) }
    var failed by remember { mutableStateOf(false) }
    var renderers by remember { mutableStateOf(emptyList<CastRenderer>()) }
    LaunchedEffect(runtime, revision) {
        searching = true; failed = false
        try {
            renderers = withContext(Dispatchers.IO) { checkNotNull(runtime).searchDlnaRenderers() }
        } catch (cancelled: kotlinx.coroutines.CancellationException) { throw cancelled }
        catch (_: Exception) { renderers = emptyList(); failed = true }
        finally { searching = false }
    }
    AlertDialog(onDismissRequest = dismiss, title = { Text(stringResource(R.string.dlna_receiver)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Hint(stringResource(R.string.dlna_network_hint))
                if (searching) {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(stringResource(R.string.dlna_searching))
                } else if (failed || renderers.isEmpty()) Text(stringResource(if (failed) R.string.dlna_search_failed else R.string.dlna_no_receivers))
                LazyColumn(Modifier.heightIn(max = 320.dp).selectableGroup()) {
                    items(renderers, key = { it.udn }) { renderer ->
                        SettingsRow(renderer.friendlyName,
                            listOf(listOf(renderer.manufacturer, renderer.modelName).filter { it.isNotBlank() }.joinToString(" "),
                                renderer.address).filter { it.isNotBlank() }.joinToString("\n"), R.drawable.symbol_car,
                            action = { choose(renderer) }, trailing = { RadioButton(choice?.udn == renderer.udn, onClick = { choose(renderer) }) })
                    }
                }
            }
        }, confirmButton = { TextButton(onClick = { revision++ }, enabled = !searching) { Text(stringResource(R.string.dlna_search_again)) } },
        dismissButton = { TextButton(onClick = dismiss) { Text(stringResource(android.R.string.cancel)) } })
}
