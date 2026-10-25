// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.animation.*
import androidx.compose.animation.core.animateFloatAsState
import org.openxplay.direlay.ui.design.*
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun CollapsibleCard(key: String, title: String, summary: String, initiallyExpanded: Boolean = false,
    targeted: Boolean = false, selected: Boolean? = null, error: Boolean = false, content: @Composable ColumnScope.() -> Unit) {
    var expanded by rememberSaveable(key) { mutableStateOf(initiallyExpanded || targeted) }
    var located by rememberSaveable(key) { mutableStateOf(false) }
    val requester = remember { BringIntoViewRequester() }
    val errorLabel = stringResource(R.string.settings_advanced_error)
    val expandedLabel = stringResource(if (expanded) R.string.menu_expanded else R.string.menu_collapsed)
    val motion = LocalRelayMotion.current
    val angle by animateFloatAsState(if (expanded) 90f else 0f, relaySpatial(motion), label = "card disclosure")
    LaunchedEffect(targeted) {
        if (targeted && !located) {
            expanded = true
            // Wait for layout so the deep-link header can be scrolled into view.
            withFrameNanos { }
            requester.bringIntoView()
            located = true
        }
    }
    Card(Modifier.fillMaxWidth(), shape = RelayTokens.group,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = if (LocalRelayDark.current) null else BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)) {
        Row(Modifier.fillMaxWidth().bringIntoViewRequester(requester)
            .semantics { stateDescription = expandedLabel; if (selected != null) this.selected = selected }
            .clickable(role = Role.Button) { expanded = !expanded }.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(title, Modifier.semantics { heading() }, style = RelayTokens.headline)
                if (!expanded) Text(if (selected == true) stringResource(R.string.menu_selected_summary, summary) else summary,
                    style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (error) Symbol(R.drawable.symbol_error, errorLabel)
            Box(Modifier.rotate(angle)) { Symbol(R.drawable.symbol_chevron, null) }
        }
        AnimatedVisibility(expanded, enter = expandVertically(relaySpatial(motion)) + fadeIn(relayEffects(motion)),
            exit = shrinkVertically(relaySpatial(motion)) + fadeOut(relayEffects(motion))) {
            Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp), content = content)
        }
    }
}

fun settingsPageTitle(route: String) = when (route) {
    "settings/output" -> R.string.ui_output
    "settings/car" -> R.string.car_and_screen
    "settings/picture" -> R.string.picture_title
    "settings/audio" -> R.string.audio_title
    "settings/connection" -> R.string.ui_iphone_connection
    "settings/connection/network" -> R.string.ui_network
    "settings/startup" -> R.string.settings_startup
    "settings/access" -> R.string.menu_access
    "settings/identity" -> R.string.menu_identity
    "settings/configFiles" -> R.string.config_title
    "settings/logs" -> R.string.logs_title
    else -> R.string.direlay_settings
}

internal data class SettingsFilePage(val title: Int, val route: String)
internal val settingsFilePages = listOf(
    SettingsFilePage(R.string.config_title, "settings/configFiles"),
    SettingsFilePage(R.string.logs_title, "settings/logs"))

@Composable
fun SettingsMenuRow(title: Int, summary: String?, icon: Int, error: Boolean = false, action: () -> Unit) {
    val hue = when (title) {
        R.string.audio_title -> IconHue.BLUETOOTH
        R.string.ui_network -> IconHue.NETWORK
        R.string.ui_output, R.string.car_and_screen, R.string.ui_carlink -> IconHue.CAR
        R.string.ui_iphone_connection -> IconHue.IPHONE
        R.string.menu_identity -> IconHue.IDENTITY
        R.string.menu_access -> IconHue.PERMISSIONS
        R.string.public_name_title, R.string.menu_connection_outputs -> IconHue.NETWORK
        R.string.settings_startup -> IconHue.PERMISSIONS
        R.string.picture_title, R.string.menu_video -> IconHue.PICTURE
        R.string.logs_title, R.string.config_title -> IconHue.LOGS
        else -> IconHue.DEVELOPER
    }
    val errorLabel = stringResource(R.string.settings_advanced_error)
    InsetRow(icon, hue, stringResource(title), summary, onClick = action, trailing = {
        if (error) CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.error) {
            Symbol(R.drawable.symbol_error, errorLabel)
        }
        Symbol(R.drawable.symbol_chevron, null, Modifier.size(16.dp))
    })
}

@Composable
fun SettingsMenu(draft: RelaySettings, invalidPages: Set<String>, wireless: Boolean,
    accessSummary: String, carSummary: String, identityStatus: Int, frameRate: Int, open: (String) -> Unit) {
    val experimental = draft.experimental
    val context = LocalContext.current
    val version = remember(context) { context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty() }
    InsetGroup {
        SettingsMenuRow(R.string.ui_output, outputSummary(draft),
            R.drawable.symbol_car, "settings/output" in invalidPages) { open("settings/output") }
        InsetDivider()
        SettingsMenuRow(R.string.car_and_screen, carSummary,
            R.drawable.symbol_car) { open("settings/car") }
        InsetDivider()
        SettingsMenuRow(R.string.picture_title, pictureMenuSummary(resolutionSummary(draft), draft.carplayScreenWidth.mm, codecSummary(draft), frameRate),
            R.drawable.symbol_monitor) { open("settings/picture") }
        InsetDivider()
        SettingsMenuRow(R.string.audio_title, stringResource(R.string.menu_audio_summary, menuToggleSummary(draft.carBluetoothAudio)),
            R.drawable.symbol_bluetooth) { open("settings/audio") }
    }
    InsetGroup {
        SettingsMenuRow(R.string.ui_iphone_connection, stringResource(if (wireless) R.string.direlay_wireless else R.string.ui_usb),
            R.drawable.symbol_phone, "settings/connection/network" in invalidPages) { open("settings/connection") }
        InsetDivider()
        SettingsMenuRow(R.string.settings_startup, stringResource(R.string.menu_startup_summary, menuToggleSummary(draft.autoStartWhenReady), menuToggleSummary(draft.autoStartOnBoot)),
            R.drawable.symbol_play) { open("settings/startup") }
    }
    InsetGroup {
        SettingsMenuRow(R.string.menu_access, accessSummary, R.drawable.symbol_lock) { open("settings/access") }
        InsetDivider()
        SettingsMenuRow(R.string.menu_identity, stringResource(identityStatus), R.drawable.symbol_file_key_2,
            "settings/identity" in invalidPages) { open("settings/identity") }
        InsetDivider()
        settingsFilePages.forEach { page ->
            SettingsMenuRow(page.title, null, R.drawable.symbol_code) { open(page.route) }
            if (page != settingsFilePages.last()) InsetDivider()
        }
    }
    InsetGroup {
        SettingsMenuRow(R.string.direlay_about, version, R.drawable.symbol_info) { open("settings/about") }
        InsetDivider()
        SettingsMenuRow(R.string.ui_developer, null, R.drawable.symbol_code,
            invalidPages.any { it.startsWith("developer/") }) { open("developer") }
        InsetDivider()
        SettingsMenuRow(R.string.experimental_title,
            if (experimental.active) stringResource(R.string.experimental_summary, experimental.optionCount) else stringResource(R.string.menu_off),
            R.drawable.symbol_settings, "experimental" in invalidPages) { open("experimental") }
    }
}

@Composable
fun menuToggleSummary(enabled: Boolean) = stringResource(if (enabled) R.string.menu_on else R.string.menu_off)

@Composable
fun resolutionSummary(draft: RelaySettings): String = listOf(stringResource(R.string.display_automatic),
    stringResource(R.string.picture_native), "1080p", "720p", stringResource(R.string.picture_screen))[draft.carplayResolution.ordinal]

@Composable
fun codecSummary(draft: RelaySettings): String = listOf(stringResource(R.string.display_automatic),
    stringResource(R.string.picture_force_h265), stringResource(R.string.picture_force_h264))[draft.videoCodec.ordinal]

@Composable
fun outputSummary(draft: RelaySettings): String {
    val labels = outputSummaryLabels(draft).associateWith { stringResource(it) }
    return outputSummary(draft, labels::getValue)
}
