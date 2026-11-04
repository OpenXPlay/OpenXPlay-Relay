// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.relay.*
import org.openxplay.direlay.settings.*

@Composable fun WirelessPictureSettings(editor: SettingsEditorState, view: org.openxplay.direlay.presentation.WirelessPictureView, openPictureSize: () -> Unit) {
    val draft by editor.draft
    val automatic = view.automatic
    val negotiated = view.negotiated
    InsetGroup(footer = stringResource(R.string.wireless_picture_priority)) {
        InsetRow(R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.wireless_picture))
        Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.selectableGroup()) {
                RadioOptionRow(RadioOption(R.string.wireless_picture_auto, R.string.wireless_picture_auto_hint),
                    draft.wirelessPicture == WirelessPictureMode.AUTO) {
                    editor.chooseWirelessPicture(WirelessPictureMode.AUTO, draft.wirelessPictureSize)
                }
                RadioOptionRow(RadioOption(R.string.direlay_custom, R.string.wireless_picture_custom_hint),
                    draft.wirelessPicture == WirelessPictureMode.CUSTOM, enabled = automatic != null) {
                    automatic?.let { editor.chooseWirelessPicture(WirelessPictureMode.CUSTOM, WirelessPicturePolicy.customStart(draft, it, negotiated)) }
                }
            }
            if (draft.wirelessPicture == WirelessPictureMode.AUTO) {
                if (draft.dlnaOutput) {
                    automatic?.let { picture -> Text(stringResource(when (picture.source) {
                        WirelessPictureSource.BROWSER -> R.string.wireless_picture_browser
                        WirelessPictureSource.CAR_MODEL -> R.string.wireless_picture_model
                        else -> R.string.wireless_picture_default
                    }, view.canvas?.width ?: picture.size.width, view.canvas?.height ?: picture.size.height)) }
                }
            } else {
                listOf("wirelessPictureWidth" to R.string.direlay_width, "wirelessPictureHeight" to R.string.direlay_height).forEach { (key, label) ->
                    val initial = if (key == "wirelessPictureWidth") draft.wirelessPictureSize.width else draft.wirelessPictureSize.height
                    SettingField(stringResource(label), editor.numberText[key] ?: initial.toString(),
                        R.string.settings_advanced_error.takeIf { key in editor.invalidNumbers }, number = true) { editor.updateWirelessPicture(key, it) }
                }
                val presets = listOf(WirelessPictureSize(1280, 720), WirelessPictureSize(1920, 1080), WirelessPictureSize(1920, 720),
                    WirelessPictureSize(1024, 600), WirelessPictureSize(1280, 480))
                Options(presets.map { "${it.width} × ${it.height}" }, presets.indexOf(draft.wirelessPictureSize)) { editor.presetWirelessPicture(presets[it]) }
            }
            if (draft.displayMode == org.openxplay.direlay.settings.DisplayMode.FIXED) {
                Hint(stringResource(R.string.wireless_picture_fixed))
                InsetRow(R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.picture_resolution), onClick = openPictureSize)
            }
        }
    }
}
