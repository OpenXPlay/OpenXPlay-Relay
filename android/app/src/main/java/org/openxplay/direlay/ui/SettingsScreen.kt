// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.content.Intent
import android.net.Uri
import androidx.compose.animation.*
import org.openxplay.direlay.ui.design.*
import androidx.compose.foundation.background
import androidx.compose.ui.draw.clip
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.ui.semantics.Role
import androidx.compose.foundation.Image
import androidx.compose.ui.res.painterResource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.LocalConfiguration
import org.openxplay.stream.core.FrameRatePreference
import org.openxplay.direlay.relay.CarPlayFrameRate
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.*
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.heading
import com.shilapi.xcertplay.orchestration.*
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun DetailPage(title: String, back: () -> Unit, modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit) {
    Box(modifier) { LargeTitleScaffold(title, back, content = content) }
}
@Composable fun SettingsRow(label: String, supporting: String, icon: Int,
    action: (() -> Unit)? = null, trailing: @Composable (() -> Unit)? = null) {
    InsetGroup {
        Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
            .then(if (action != null) Modifier.clickable(role = Role.Button, onClick = action) else Modifier)
            .padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            IconTile(icon, when (icon) {
                R.drawable.symbol_bluetooth -> IconHue.BLUETOOTH
                R.drawable.symbol_wifi -> IconHue.NETWORK
                R.drawable.symbol_car -> IconHue.CAR
                R.drawable.symbol_phone -> IconHue.IPHONE
                else -> IconHue.DEVELOPER
            })
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(label, style = RelayTokens.body)
                if (supporting.isNotEmpty()) Text(supporting, style = RelayTokens.footnote,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (trailing != null) trailing() else if (action != null) Symbol(R.drawable.symbol_chevron, null, Modifier.size(16.dp))
        }
    }
}
@Composable fun SmallHeading(text: String) {
    Text(text, Modifier.padding(top = 16.dp, bottom = 8.dp).then(Modifier.semanticsHeading()),
        style = RelayTokens.footnote, color = MaterialTheme.colorScheme.onSurfaceVariant)
}
private fun Modifier.semanticsHeading() = this.then(Modifier.semantics { heading() })

@Composable fun SettingsScreen(state: SettingsEditorState, wireless: (RelaySettings) -> Boolean,
    route: String, back: () -> Unit, open: (String) -> Unit,
    copy: (String) -> Unit, import: (Boolean) -> Unit, remove: (Boolean) -> Unit, currentWifi: String? = null,
    bondedDevices: suspend () -> org.openxplay.direlay.presentation.IphonePickerView, useSharedWifi: (String) -> Unit = {},
    sources: org.openxplay.direlay.presentation.ScreenSources, actions: org.openxplay.direlay.presentation.ScreenActions,
    support: org.openxplay.direlay.presentation.SupportActions, running: Boolean = false) {
    val common by sources.settings.screenState()
    val settings = common ?: return
    var draft by state.draft
    var confirmOutput by remember { mutableStateOf<RelaySettings?>(null) }
    fun selectOutput(choice: OutputChoice, kind: WirelessDisplayKind) {
        val next = OutputSelection.choose(draft, choice, kind, wireless(draft))
        val candidate = draft.copy(outputs = OutputSelection.outputs(choice, kind))
        if (CarlinkConnectionPolicy.enabling(draft, candidate, wireless(candidate)) != candidate) confirmOutput = next
        else draft = next
    }
    var confirmUsb by remember { mutableStateOf(false) }
    if (confirmUsb) UsbConnectionDialog(confirm = {
        val candidate = draft.copy(connectionMode = ConnectionMode.USB)
        draft = CarlinkConnectionPolicy.enabling(draft, candidate, wireless(candidate))
        confirmUsb = false
    }, dismiss = { confirmUsb = false })
    val appliedFps = settings.appliedFps
    val negotiatedFps = settings.negotiatedFps
    val frameRate = if (running) appliedFps else CarPlayFrameRate.requested(draft.frameRate, negotiatedFps)
    val errors = settingsErrors(draft, wireless(draft))
    val invalidPages = state.errorPages(wireless)
    val page = route.substringBefore('#')
    val target = route.substringAfter('#', "")
    var iphoneLocated by rememberSaveable { mutableStateOf(false) }
    var sharedWifiLocated by rememberSaveable { mutableStateOf(false) }
    val models = settings.models
    val carSummary = carSelectionSummary(draft.car, models, LocalConfiguration.current.locales[0],
        stringResource(R.string.car_none), stringResource(R.string.car_other))
    val ms = if (LocalRelayMotion.current) 220 else 0
    confirmOutput?.let { next ->
        val miracast = next.miracastOutput
        AlertDialog(onDismissRequest = { confirmOutput = null },
            title = { Text(stringResource(if (miracast) R.string.wireless_miracast else R.string.ui_carlink)) },
            text = { Text(stringResource(if (miracast) R.string.miracast_wifi_direct_confirm else R.string.carlink_wifi_direct_confirm)) },
            confirmButton = { TextButton(onClick = { draft = next; confirmOutput = null }) { Text(stringResource(android.R.string.ok)) } },
            dismissButton = { TextButton(onClick = { confirmOutput = null }) { Text(stringResource(android.R.string.cancel)) } })
    }
    DetailPage(stringResource(settingsPageTitle(page)), back) {
        when (page) {
            "settings" -> {
                val identityStatus by sources.identity.screenState()
                val summary = stringResource(R.string.menu_access_summary,
                    menuToggleSummary(settings.adminEnabled), menuToggleSummary(settings.publicNameEnabled))
                SettingsMenu(draft, invalidPages, wireless(draft), summary, carSummary, identityStatus, frameRate, open)
            }
            "settings/configFiles" -> ConfigFilesScreen(support.config)
            "settings/logs" -> LogsScreen(support.logs, support.upload)
            "settings/identity" -> {
                val identityView by sources.developer.screenState()
                IdentitySettings(state, identityView, copy, import, remove)
            }
            "settings/output" -> {
                SettingsGroup("output", R.drawable.symbol_car, IconHue.CAR, stringResource(R.string.ui_output), outputSummary(draft),
                    initiallyExpanded = true, targeted = target == "output",
                    footer = if (OutputSelection.choice(draft.outputs) == null) stringResource(R.string.output_multiple_hint) else null) {
                    val selected = OutputSelection.choice(draft.outputs)
                    val kind = OutputSelection.wirelessKind(draft.outputs)
                    Column(Modifier.selectableGroup()) {
                        outputOptions.forEachIndexed { index, option ->
                            val choice = OutputChoice.entries[index]
                            RadioOptionRow(option, selected == choice, extra = when {
                                choice == OutputChoice.CARLINK -> ({ NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.WIFI) })
                                choice == OutputChoice.WIRELESS && selected == choice -> ({
                                    Options(listOf("DLNA", "Miracast"), kind.ordinal) { selectOutput(choice, WirelessDisplayKind.entries[it]) }
                                })
                                else -> null
                            }) { selectOutput(choice, kind) }
                        }
                    }
                    if (selected == null) InsetRow(R.drawable.symbol_link, IconHue.DEVELOPER,
                        stringResource(R.string.output_multiple), outputSummary(draft), onClick = { open("developer") })
                    if (CarlinkConnectionPolicy.enabled(draft)) Hint(stringResource(R.string.carlink_wifi_direct_constraint))
                    AnimatedVisibility(draft.carlinkOutput, enter = expandVertically(tween(ms), expandFrom = Alignment.Top) + fadeIn(tween(ms)), exit = shrinkVertically(tween(ms), shrinkTowards = Alignment.Top) + fadeOut(tween(ms))) {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            SettingField(stringResource(R.string.direlay_carlink_name), draft.carlinkDeviceName, errors["name"]) { draft = draft.copy(carlinkDeviceName = it) }
                            SettingsToggleGroup(stringResource(R.string.vehicle_title), draft.vehicleInformation, R.drawable.symbol_car, IconHue.CAR,
                                footer = stringResource(R.string.vehicle_privacy)) { draft = draft.copy(vehicleInformation = it) }
                            SettingsToggleGroup(stringResource(R.string.carlink_auto_reconnect), draft.carlinkAutoReconnect, R.drawable.symbol_sync, IconHue.CAR,
                                footer = stringResource(R.string.carlink_auto_reconnect_hint)) { draft = draft.copy(carlinkAutoReconnect = it) }
                            SettingsToggleGroup(stringResource(R.string.carlink_resize_reconnect), draft.carlinkReconnectOnResize, R.drawable.symbol_sync, IconHue.CAR,
                                footer = stringResource(R.string.carlink_resize_reconnect_hint)) { draft = draft.copy(carlinkReconnectOnResize = it) }
                        }
                    }
                }
                if (draft.dlnaOutput || draft.miracastOutput) {
                    val picture by sources.wirelessPicture.screenState()
                    WirelessDisplaySettings(draft, picture, actions.searchRenderers, state, openCarScreen = { open("settings/car") }) { draft = it }
                }
            }
            "settings/car" -> {
                val view by sources.car.screenState()
                CarScreenSettings(draft, view, wireless, actions.clearHeadUnit,
                    openTrustedHttps = { open("settings/access/publicName") },
                    openOutput = { open("settings/output") }) { draft = it }
            }
            "settings/picture" -> {
                SettingsGroup("resolution", R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.picture_resolution), resolutionSummary(draft),
                    initiallyExpanded = target.isEmpty() || target == "resolution", targeted = target == "resolution",
                    footer = if (draft.carplayResolution == CarPlayResolution.NATIVE) stringResource(R.string.picture_native_hint) else null) {
                    Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.picture_native), "1080p", "720p", stringResource(R.string.picture_screen)),
                        CarPlayResolution.entries.indexOf(draft.carplayResolution)) { draft = draft.copy(carplayResolution = CarPlayResolution.entries[it]) }
                }
                SettingsGroup("frameRate", R.drawable.symbol_activity, IconHue.PICTURE, stringResource(R.string.picture_frame_rate),
                    frameRateSummary(draft.frameRate, frameRate, stringResource(R.string.picture_frame_rate_auto)),
                    initiallyExpanded = false, targeted = target == "frameRate", footer = stringResource(R.string.picture_frame_rate_hint)) {
                    Options(listOf(stringResource(R.string.picture_frame_rate_auto)) + (30..60 step 5).map(Int::toString),
                        FrameRatePreference.entries.indexOf(draft.frameRate)) { draft = draft.copy(frameRate = FrameRatePreference.entries[it]) }
                }
                SettingsGroup("width", R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.picture_screen_width), "${draft.carplayScreenWidth.mm} mm",
                    initiallyExpanded = false, targeted = target == "width", footer = stringResource(R.string.picture_screen_width_hint)) {
                    Options(CarPlayScreenWidth.entries.map { "${it.mm} mm" },
                        CarPlayScreenWidth.entries.indexOf(draft.carplayScreenWidth)) { draft = draft.copy(carplayScreenWidth = CarPlayScreenWidth.entries[it]) }
                }
                SettingsGroup("codec", R.drawable.symbol_monitor, IconHue.PICTURE, stringResource(R.string.picture_video_codec), codecSummary(draft),
                    initiallyExpanded = false, targeted = target == "codec", footer = stringResource(R.string.picture_video_codec_hint)) {
                    Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.picture_force_h265), stringResource(R.string.picture_force_h264)),
                        VideoCodecSetting.entries.indexOf(draft.videoCodec)) { draft = draft.copy(videoCodec = VideoCodecSetting.entries[it]) }
                }
                SettingsGroup("stats", R.drawable.symbol_activity, IconHue.PICTURE, stringResource(R.string.video_stats_title), stringResource(listOf(R.string.video_stats_off, R.string.video_stats_5, R.string.video_stats_15)[draft.videoStats.ordinal]),
                    initiallyExpanded = false, targeted = target == "stats", footer = stringResource(R.string.video_stats_hint)) {
                    Options(listOf(stringResource(R.string.video_stats_off), stringResource(R.string.video_stats_5), stringResource(R.string.video_stats_15)),
                        VideoStatsWindow.entries.indexOf(draft.videoStats)) { draft = draft.copy(videoStats = VideoStatsWindow.entries[it]) }
                }
            }
            "settings/audio" -> {
                SettingsToggleGroup(stringResource(R.string.audio_car_bluetooth), draft.carBluetoothAudio, R.drawable.symbol_bluetooth, IconHue.BLUETOOTH,
                    footer = stringResource(R.string.audio_car_bluetooth_hint)) { draft = draft.copy(carBluetoothAudio = it) }
            }
            "settings/connection" -> {
                SettingsGroup("connection", R.drawable.symbol_phone, IconHue.IPHONE, stringResource(R.string.ui_iphone_connection), stringResource(if (wireless(draft)) R.string.direlay_wireless else R.string.ui_usb),
                    initiallyExpanded = true, targeted = target == "connection" || target == "iphone") {
                    RadioOptions(connectionOptions, if (wireless(draft)) 0 else 1) {
                        val candidate = draft.copy(connectionMode = if (it == 0) ConnectionMode.WIRELESS else ConnectionMode.USB)
                        if (needsUsbConfirmation(draft.connectionMode, candidate.connectionMode, wireless(draft))) confirmUsb = true
                        else draft = CarlinkConnectionPolicy.enabling(draft, candidate, wireless(candidate))
                    }
                    AnimatedVisibility(wireless(draft), enter = expandVertically(tween(ms), expandFrom = Alignment.Top) + fadeIn(tween(ms)),
                        exit = shrinkVertically(tween(ms), shrinkTowards = Alignment.Top) + fadeOut(tween(ms))) {
                        Column {
                            IphonePickerSetting(draft.iphoneAddress, bondedDevices, target == "iphone" && !iphoneLocated) {
                                draft = draft.copy(iphoneAddress = it)
                            }
                            LaunchedEffect(Unit) { iphoneLocated = true }
                        }
                    }
                }
                if (wireless(draft)) InsetGroup {
                    SettingsMenuRow(R.string.ui_network, stringResource(networkOption(draft.wirelessNetwork.choice).title),
                        R.drawable.symbol_wifi, "settings/connection/network" in invalidPages) { open("settings/connection/network") }
                }
            }
            "settings/connection/network" -> {
                Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    WirelessNetworkChoice.entries.forEach { choice ->
                        val key = choice.name
                        SettingsGroup(key, R.drawable.symbol_wifi, IconHue.NETWORK, stringResource(networkOption(choice).title), stringResource(networkOption(choice).supporting),
                            initiallyExpanded = draft.wirelessNetwork.choice == choice && target.isEmpty(),
                            selected = draft.wirelessNetwork.choice == choice, error = draft.wirelessNetwork.choice == choice && "settings/connection/network" in invalidPages, targeted = target == key || (target == "sharedWifi" && choice == WirelessNetworkChoice.EXISTING_WIFI)) {
                            RadioOptionRow(networkOption(choice), draft.wirelessNetwork.choice == choice,
                                extra = {
                                    NetworkDiagram(choice, draft.wirelessNetwork.choice == choice)
                                    if (choice == WirelessNetworkChoice.PHONE_HOTSPOT)
                                        NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.HOTSPOT)
                                    if (choice == WirelessNetworkChoice.WIFI_DIRECT && !CarlinkConnectionPolicy.wifiDirectAllowed(draft))
                                        Hint(stringResource(if (draft.miracastOutput && !draft.carlinkOutput) R.string.miracast_wifi_direct_constraint else R.string.carlink_wifi_direct_constraint))
                                }, enabled = choice != WirelessNetworkChoice.WIFI_DIRECT || CarlinkConnectionPolicy.wifiDirectAllowed(draft)) {
                                draft = draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(choice = choice))
                            }
                            if (draft.wirelessNetwork.choice == choice && choice != WirelessNetworkChoice.WIFI_DIRECT) {
                                NetworkFields(draft.wirelessNetwork, choice, errors, currentWifi, target == "sharedWifi" && !sharedWifiLocated, useSharedWifi) {
                                    draft = draft.copy(wirelessNetwork = it)
                                }
                                if (choice == WirelessNetworkChoice.EXISTING_WIFI) LaunchedEffect(Unit) { sharedWifiLocated = true }
                            }
                        }
                    }
                }
            }
            "settings/startup" -> {
                SettingsToggleGroup(stringResource(R.string.ui_automatic), draft.autoStartWhenReady, R.drawable.symbol_sync, IconHue.PERMISSIONS,
                    footer = stringResource(R.string.settings_automatic_hint)) { draft = draft.copy(autoStartWhenReady = it) }
                SettingsToggleGroup(stringResource(R.string.ui_boot), draft.autoStartOnBoot, R.drawable.symbol_power, IconHue.PERMISSIONS,
                    footer = stringResource(R.string.direlay_boot_hint)) { draft = draft.copy(autoStartOnBoot = it) }
            }
            "settings/access" -> {
                val access by sources.admin.screenState()
                val nameView by sources.name.screenState()
                val name = nameView.view
                AdminPageSettings(access.enabled, access.urls, actions.adminEnabled, copy, actions.resetPassword)
                InsetGroup(footer = stringResource(R.string.public_name_summary)) {
                    SettingsMenuRow(R.string.public_name_title, name?.name?.takeIf { it.isNotEmpty() }
                        ?: menuToggleSummary(name?.enabled == true), R.drawable.symbol_globe) { open("settings/access/publicName") }
                }
            }
        }
        if (state.savingError) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        errors["advanced"]?.let { Text(stringResource(it), color = MaterialTheme.colorScheme.error) }
    }
}
@Composable fun NetworkFields(network: WirelessNetworkSettings, choice: WirelessNetworkChoice,
    errors: Map<String, Int>, currentWifi: String?, openSharedWifi: Boolean, useSharedWifi: (String) -> Unit, credentialsModifier: Modifier = Modifier, change: (WirelessNetworkSettings) -> Unit) {
    val hotspot = choice == WirelessNetworkChoice.PHONE_HOTSPOT
    val context = LocalContext.current
    val opener: NetworkSettingsOpener = remember(context) { NetworkSettingsLauncher(context) }
    if (!hotspot) {
        SharedWifiEditor(network.sharedWifiNetworks, currentWifi, openSharedWifi, useSharedWifi) { change(network.copy(sharedWifiNetworks = it)) }
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Hint(stringResource(if (hotspot) R.string.direlay_car_hotspot_hint else R.string.direlay_existing_wifi_hint))
        // Apps cannot read the hotspot band, so the advice is unconditional (2.4 GHz stutters on real cars).
        Hint(stringResource(R.string.hotspot_5ghz_hint))
        TextButton(onClick = { opener.open(hotspot) }) {
            Text(stringResource(if (hotspot) R.string.settings_open_hotspot else R.string.ui_open_wifi))
        }
        Column(credentialsModifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SettingField(stringResource(if (hotspot) R.string.settings_hotspot_name else R.string.direlay_network_ssid), network.manualSsid, errors["ssid"]) {
                change(network.copy(manualSsid = it))
            }
            SettingField(stringResource(if (hotspot) R.string.settings_hotspot_password else R.string.direlay_network_passphrase), network.manualPassphrase,
                errors["password"], secret = true) {
                change(network.copy(manualPassphrase = it,
                    security = if (it.isEmpty()) ManualHotspotSecurity.OPEN else ManualHotspotSecurity.WPA2))
            }
        }
        if (hotspot) {
            SmallHeading(stringResource(R.string.direlay_hotspot_band))
            Options(listOf(stringResource(R.string.direlay_band_auto), stringResource(R.string.direlay_band_24), stringResource(R.string.direlay_band_5)),
                ManualHotspotBand.entries.indexOf(network.band)) {
                change(network.copy(band = ManualHotspotBand.entries[it], channel = 0))
            }
        }
    }
}
@Composable fun RadioOptions(options: List<RadioOption>, selected: Int, select: (Int) -> Unit) {
    Column(Modifier.selectableGroup()) {
        options.forEachIndexed { index, option -> RadioOptionRow(option, index == selected) { select(index) } }
    }
}
@Composable fun RadioOptionRow(option: RadioOption, selected: Boolean, extra: @Composable (() -> Unit)? = null,
    enabled: Boolean = true, supportingText: String? = null, select: () -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
        .clip(RelayTokens.group).background(if (selected) MaterialTheme.colorScheme.primary.copy(alpha = .14f) else MaterialTheme.colorScheme.surface)
        .selectable(enabled = enabled, selected = selected, role = Role.RadioButton, onClick = select).padding(vertical = 12.dp, horizontal = 4.dp),
        // With a diagram below, the radio button stays level with the title instead of the row centre.
        verticalAlignment = if (extra == null) Alignment.CenterVertically else Alignment.Top) {
        RadioButton(selected = selected, enabled = enabled, onClick = null, modifier = Modifier.padding(horizontal = 12.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(stringResource(option.title), style = RelayTokens.headline)
            val supporting = supportingText ?: stringResource(option.supporting)
            if (supporting.isNotEmpty()) Text(supporting, style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            extra?.invoke()
        }
    }
}
@OptIn(ExperimentalLayoutApi::class)
@Composable fun Options(labels: List<String>, selected: Int, enabled: Boolean = true, select: (Int) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        labels.forEachIndexed { index, label -> FilterChip(index == selected, { select(index) },
            modifier = Modifier.heightIn(min = 48.dp), shape = RelayTokens.chip, enabled = enabled, label = { Text(label, textAlign = TextAlign.Center) }) }
    }
}
@Composable fun SettingField(label: String, value: String, error: Int? = null, number: Boolean = false,
    secret: Boolean = false, enabled: Boolean = true, change: (String) -> Unit) {
    OutlinedTextField(value, change, Modifier.fillMaxWidth(), label = { Text(label) }, singleLine = true,
        shape = RelayTokens.group, enabled = enabled, isError = error != null, supportingText = error?.let { { Text(stringResource(it)) } },
        keyboardOptions = KeyboardOptions(keyboardType = if (number) KeyboardType.Number else if (secret) KeyboardType.Password else KeyboardType.Text),
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None)
}
@Composable fun KeyValue(label: String, value: String, copy: ((String) -> Unit)? = null) {
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(value, Modifier.weight(1f), style = RelayTokens.mono)
            if (copy != null) IconButton(onClick = { copy(value) }) { Symbol(R.drawable.symbol_copy, stringResource(R.string.settings_copy_value, label)) }
        }
    }
    InsetDivider()
}
@Composable fun AboutScreen(back: () -> Unit, updates: org.openxplay.direlay.presentation.UpdateActions) {
    val context = LocalContext.current
    fun link(url: String) { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
    DetailPage(stringResource(R.string.direlay_about), back) {
        InsetGroup(footer = stringResource(R.string.about_project)) {
            Row(Modifier.fillMaxWidth().padding(16.dp), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Image(painterResource(R.mipmap.ic_launcher_foreground), null, Modifier.size(64.dp))
                Column(Modifier.weight(1f)) {
                    RelayBrand()
                    Text(LocalAppVersion.current,
                        style = RelayTokens.callout, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            InsetDivider()
            InsetRow(R.drawable.symbol_globe, IconHue.NETWORK, stringResource(R.string.direlay_attribution),
                onClick = { link(context.getString(R.string.direlay_project_url)) })
            InsetDivider()
            InsetRow(R.drawable.symbol_code, IconHue.DEVELOPER, stringResource(R.string.about_source),
                stringResource(R.string.about_source_hint), onClick = { link(context.getString(R.string.direlay_source_url)) })
            InsetDivider()
            InsetRow(R.drawable.symbol_info, IconHue.DEVELOPER, stringResource(R.string.about_licence_title),
                stringResource(R.string.about_licence), onClick = { link("https://github.com/OpenXPlay/OpenXPlay-Relay/blob/main/NOTICE") })
        }
        UpdateSection(updates)
        InsetGroup(header = stringResource(R.string.direlay_acknowledgements), footer = stringResource(R.string.direlay_trademark)) {
            Column(Modifier.padding(16.dp)) { Text(stringResource(R.string.direlay_acknowledgement_text), style = RelayTokens.footnote) }
            InsetDivider()
            InsetRow(R.drawable.symbol_car, IconHue.CAR, "DiPlay", stringResource(R.string.about_diplay_hint),
                onClick = { link(context.getString(R.string.direlay_diplay_url)) })
            InsetDivider()
            InsetRow(R.drawable.symbol_phone, IconHue.IPHONE, "xcertplay", stringResource(R.string.about_xcertplay_hint),
                onClick = { link(context.getString(R.string.direlay_xcertplay_url)) })
        }
    }
}
