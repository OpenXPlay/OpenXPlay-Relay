// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.ui.design.*

enum class CarlinkPictureAvailability { AVAILABLE, OUTPUT_OFF, FIXED_DISPLAY }
enum class CarlinkPictureSave { READY, INVALID, DUPLICATE, LIMIT }

@Immutable
data class CarlinkPictureUiModel(val sizes: List<PictureSize?>, val saved: List<PictureSize>,
    val selected: PictureSize?, val default: PictureSize?, val inUse: PictureSize?,
    val availability: CarlinkPictureAvailability, val width: String, val height: String,
    val custom: PictureSize?, val save: CarlinkPictureSave) {
    val enabled get() = availability == CarlinkPictureAvailability.AVAILABLE
    val differsFromDefault get() = selected != default
    val canSave get() = enabled && (save == CarlinkPictureSave.READY || save == CarlinkPictureSave.DUPLICATE)
}

fun carlinkPictureUiModel(settings: RelaySettings, trial: CarlinkPictureTrial?, effectiveCanvas: PictureSize? = null,
    width: String = "", height: String = "", effective: CarlinkPictureTrial? = null): CarlinkPictureUiModel {
    val custom = CarlinkPictureSizes.parse(width, height)
    val choice = trial ?: effective
    val selected = if (choice != null) choice.size else settings.carlinkPictureDefault
    return CarlinkPictureUiModel(listOf(null) + CarlinkPictureSizes.presets + settings.carlinkPictureSizes,
        settings.carlinkPictureSizes, selected, settings.carlinkPictureDefault,
        effectiveCanvas ?: selected,
        when {
            !settings.carlinkOutput -> CarlinkPictureAvailability.OUTPUT_OFF
            settings.displayMode == org.openxplay.direlay.settings.DisplayMode.FIXED -> CarlinkPictureAvailability.FIXED_DISPLAY
            else -> CarlinkPictureAvailability.AVAILABLE
        }, width, height, custom, when {
            custom == null -> CarlinkPictureSave.INVALID
            custom in CarlinkPictureSizes.presets || custom in settings.carlinkPictureSizes -> CarlinkPictureSave.DUPLICATE
            settings.carlinkPictureSizes.size >= CarlinkPictureSizes.MAX_SAVED -> CarlinkPictureSave.LIMIT
            else -> CarlinkPictureSave.READY
        })
}

@Composable private fun pictureLabel(size: PictureSize?): String =
    size?.let { "${it.width} × ${it.height}" } ?: stringResource(R.string.display_automatic)

@Composable fun CarlinkPictureSettings(model: CarlinkPictureUiModel, select: (PictureSize?) -> Unit,
    setDefault: (PictureSize?) -> Unit, delete: (PictureSize) -> Unit,
    input: (String, String) -> Unit, save: () -> Unit) {
    val hint = when (model.availability) {
        CarlinkPictureAvailability.OUTPUT_OFF -> R.string.carlink_picture_output_off
        CarlinkPictureAvailability.FIXED_DISPLAY -> R.string.carlink_picture_fixed
        else -> null
    }
    SettingsGroup("carlinkPicture", R.drawable.symbol_monitor, IconHue.PICTURE,
        stringResource(R.string.carlink_picture_title), pictureLabel(model.inUse), initiallyExpanded = false,
        footer = hint?.let { stringResource(it) }) {
        Column(Modifier.alpha(if (model.enabled) 1f else .38f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.selectableGroup()) {
                model.sizes.forEach { size ->
                    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
                        Row(Modifier.weight(1f).heightIn(min = 48.dp).selectable(model.selected == size,
                            enabled = model.enabled, role = Role.RadioButton, onClick = { select(size) }),
                            verticalAlignment = Alignment.CenterVertically) {
                            RadioButton(model.selected == size, onClick = null, enabled = model.enabled,
                                modifier = Modifier.padding(end = 8.dp))
                            Column(Modifier.weight(1f)) {
                                Text(pictureLabel(size), style = RelayTokens.body)
                                size?.ratioLabel?.let { Text(it, style = RelayTokens.footnote,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant) }
                            }
                        }
                        IconToggleButton(checked = model.default == size,
                            onCheckedChange = { setDefault(if (it) size else null) }, enabled = model.enabled,
                            modifier = Modifier.size(48.dp)) {
                            Symbol(if (model.default == size) R.drawable.carlink_picture_star_filled else R.drawable.carlink_picture_star,
                                stringResource(if (model.default == size) R.string.carlink_picture_default else R.string.carlink_picture_set_default))
                        }
                        if (size != null && size in model.saved) IconButton(onClick = { delete(size) },
                            enabled = model.enabled, modifier = Modifier.size(48.dp)) {
                            Symbol(R.drawable.carlink_picture_delete, stringResource(R.string.carlink_picture_delete))
                        }
                    }
                }
            }
            if (model.differsFromDefault) Hint(stringResource(R.string.carlink_picture_trial,
                pictureLabel(model.inUse), pictureLabel(model.default)))
            listOf("carlinkPictureWidth" to R.string.direlay_width, "carlinkPictureHeight" to R.string.direlay_height).forEach { (key, label) ->
                val value = if (key == "carlinkPictureWidth") model.width else model.height
                SettingField(stringResource(label), value,
                    R.string.carlink_picture_invalid.takeIf { model.custom == null && value.isNotEmpty() },
                    number = true, enabled = model.enabled) { input(key, it) }
            }
            Button(onClick = save, enabled = model.canSave,
                modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.carlink_picture_save)) }
            // An invalid size is reported under its fields.
            val reason = when (model.save) {
                CarlinkPictureSave.DUPLICATE -> R.string.carlink_picture_duplicate
                CarlinkPictureSave.LIMIT -> R.string.carlink_picture_limit
                else -> null
            }
            reason?.let { Hint(stringResource(it)) }
        }
    }
}
