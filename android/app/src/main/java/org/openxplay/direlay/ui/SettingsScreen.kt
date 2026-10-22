// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import android.content.Intent
import android.net.Uri
import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.ui.semantics.Role
import androidx.compose.foundation.Image
import androidx.compose.ui.res.painterResource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.LocalConfiguration
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.openxplay.direlay.relay.CarModels
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.*
import androidx.compose.ui.unit.dp
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.heading
import com.shilapi.xcertplay.orchestration.*
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun DetailPage(title: String, back: () -> Unit, modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit) {
    Scaffold(modifier, topBar = { TopAppBar(expandedHeight = 64.dp * LocalDensity.current.fontScale.coerceAtLeast(1f), title = { Heading(title) }, navigationIcon = {
        IconButton(onClick = back) { Symbol(R.drawable.symbol_back, stringResource(R.string.ui_back)) }
    }) }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).imePadding().verticalScroll(rememberScrollState())
            .padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp), content = content)
    }
}
@Composable fun SettingsRow(label: String, supporting: String, icon: Int,
    action: (() -> Unit)? = null, trailing: @Composable (() -> Unit)? = null) {
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
        .then(if (action != null) Modifier.clickable(onClick = action) else Modifier)
        .padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(48.dp), contentAlignment = Alignment.Center) { Symbol(icon, null) }
        Column(Modifier.weight(1f).padding(end = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(label, style = MaterialTheme.typography.bodyLarge)
            Text(supporting, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (trailing != null) trailing() else if (action != null) Symbol(R.drawable.symbol_chevron, null)
    }
}
@Composable fun SmallHeading(text: String) {
    Text(text, Modifier.padding(top = 16.dp, bottom = 8.dp).then(Modifier.semanticsHeading()),
        style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
}
private fun Modifier.semanticsHeading() = this.then(Modifier.semantics { heading() })

@Composable fun SettingsScreen(state: SettingsEditorState, wireless: (RelaySettings) -> Boolean,
    route: String, back: () -> Unit, open: (String) -> Unit, identityStatus: Int, identityView: DeveloperView,
    copy: (String) -> Unit, import: (Boolean) -> Unit, remove: (Boolean) -> Unit, currentWifi: String? = null,
    bondedDevices: org.openxplay.direlay.bluetooth.BondedDevices, useSharedWifi: (String) -> Unit = {},
    adminPage: @Composable () -> Unit = {}, accessSummary: String = "", runtime: org.openxplay.direlay.relay.RelayRuntime? = null) {
    var draft by state.draft
    var confirmCarlink by remember { mutableStateOf(false) }
    var confirmUsb by remember { mutableStateOf(false) }
    if (confirmUsb) UsbConnectionDialog(confirm = {
        val candidate = draft.copy(connectionMode = ConnectionMode.USB)
        draft = CarlinkConnectionPolicy.enabling(draft, candidate, wireless(candidate))
        confirmUsb = false
    }, dismiss = { confirmUsb = false })
    val errors = settingsErrors(draft, wireless(draft))
    val invalidPages = state.errorPages(wireless)
    val page = route.substringBefore('#')
    val target = route.substringAfter('#', "")
    var iphoneLocated by rememberSaveable { mutableStateOf(false) }
    var sharedWifiLocated by rememberSaveable { mutableStateOf(false) }
    var models by remember { mutableStateOf<CarModels?>(null) }
    LaunchedEffect(runtime) { models = withContext(Dispatchers.IO) { runtime?.carModels } }
    val carSummary = carSelectionSummary(draft.car, models, LocalConfiguration.current.locales[0],
        stringResource(R.string.car_none), stringResource(R.string.car_other))
    val ms = if (LocalRelayMotion.current) 220 else 0
    if (confirmCarlink) AlertDialog(onDismissRequest = { confirmCarlink = false },
        title = { Text(stringResource(R.string.ui_carlink)) },
        text = { Text(stringResource(R.string.carlink_wifi_direct_confirm)) },
        confirmButton = { TextButton(onClick = {
            draft = CarlinkConnectionPolicy.enabling(draft, draft.copy(carlinkOutput = true), wireless(draft))
            confirmCarlink = false
        }) { Text(stringResource(android.R.string.ok)) } },
        dismissButton = { TextButton(onClick = { confirmCarlink = false }) { Text(stringResource(android.R.string.cancel)) } })
    DetailPage(stringResource(settingsPageTitle(page)), back) {
        when (page) {
            "settings" -> SettingsMenu(draft, invalidPages, wireless(draft), accessSummary, carSummary, identityStatus, open)
            "settings/identity" -> IdentitySettings(state, identityView, copy, import, remove)
            "settings/output" -> {
                CollapsibleCard("output", stringResource(R.string.ui_output), outputSummary(draft),
                    initiallyExpanded = true, targeted = target == "output") {
                    Column(Modifier.selectableGroup()) {
                        outputOptions.forEachIndexed { index, option ->
                            RadioOptionRow(option, (if (draft.carlinkOutput) 1 else 0) == index,
                                extra = if (index == 1) ({ NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.WIFI) }) else null) {
                                val candidate = draft.copy(carlinkOutput = index == 1)
                                if (CarlinkConnectionPolicy.enabling(draft, candidate, wireless(candidate)) != candidate) confirmCarlink = true
                                else draft = candidate
                            }
                        }
                    }
                    if (CarlinkConnectionPolicy.enabled(draft)) Hint(stringResource(R.string.carlink_wifi_direct_constraint))
                    AnimatedVisibility(draft.carlinkOutput || draft.bothOutputs, enter = expandVertically(tween(ms)) + fadeIn(tween(ms)), exit = shrinkVertically(tween(ms)) + fadeOut(tween(ms))) {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            SettingField(stringResource(R.string.direlay_carlink_name), draft.carlinkDeviceName, errors["name"]) { draft = draft.copy(carlinkDeviceName = it) }
                            SettingsRow(stringResource(R.string.vehicle_title), stringResource(R.string.vehicle_privacy), R.drawable.symbol_car,
                                trailing = { LabeledSwitch(stringResource(R.string.vehicle_title), draft.vehicleInformation, { draft = draft.copy(vehicleInformation = it) }) })
                            SettingsRow(stringResource(R.string.carlink_auto_reconnect), stringResource(R.string.carlink_auto_reconnect_hint), R.drawable.symbol_sync,
                                trailing = { LabeledSwitch(stringResource(R.string.carlink_auto_reconnect), draft.carlinkAutoReconnect, { draft = draft.copy(carlinkAutoReconnect = it) }) })
                            SettingsRow(stringResource(R.string.carlink_resize_reconnect), stringResource(R.string.carlink_resize_reconnect_hint), R.drawable.symbol_sync,
                                trailing = { LabeledSwitch(stringResource(R.string.carlink_resize_reconnect), draft.carlinkReconnectOnResize, { draft = draft.copy(carlinkReconnectOnResize = it) }) })
                        }
                    }
                }
            }
            "settings/car" -> {
                if (runtime != null) CarScreenSettings(draft, runtime) { draft = it }
            }
            "settings/picture" -> {
                CollapsibleCard("resolution", stringResource(R.string.picture_resolution), resolutionSummary(draft),
                    initiallyExpanded = target.isEmpty() || target == "resolution", targeted = target == "resolution") {
                    Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.picture_native), "1080p", "720p", stringResource(R.string.picture_screen)),
                        CarPlayResolution.entries.indexOf(draft.carplayResolution)) { draft = draft.copy(carplayResolution = CarPlayResolution.entries[it]) }
                    if (draft.carplayResolution == CarPlayResolution.NATIVE) Hint(stringResource(R.string.picture_native_hint))
                }
                CollapsibleCard("width", stringResource(R.string.picture_screen_width), "${draft.carplayScreenWidth.mm} mm",
                    initiallyExpanded = false, targeted = target == "width") {
                    Options(CarPlayScreenWidth.entries.map { "${it.mm} mm" },
                        CarPlayScreenWidth.entries.indexOf(draft.carplayScreenWidth)) { draft = draft.copy(carplayScreenWidth = CarPlayScreenWidth.entries[it]) }
                    Hint(stringResource(R.string.picture_screen_width_hint))
                }
                CollapsibleCard("codec", stringResource(R.string.picture_video_codec), codecSummary(draft),
                    initiallyExpanded = false, targeted = target == "codec") {
                    Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.picture_force_h265), stringResource(R.string.picture_force_h264)),
                        VideoCodecSetting.entries.indexOf(draft.videoCodec)) { draft = draft.copy(videoCodec = VideoCodecSetting.entries[it]) }
                    Hint(stringResource(R.string.picture_video_codec_hint))
                }
                CollapsibleCard("stats", stringResource(R.string.video_stats_title), stringResource(listOf(R.string.video_stats_off, R.string.video_stats_5, R.string.video_stats_15)[draft.videoStats.ordinal]),
                    initiallyExpanded = false, targeted = target == "stats") {
                    Options(listOf(stringResource(R.string.video_stats_off), stringResource(R.string.video_stats_5), stringResource(R.string.video_stats_15)),
                        VideoStatsWindow.entries.indexOf(draft.videoStats)) { draft = draft.copy(videoStats = VideoStatsWindow.entries[it]) }
                    Hint(stringResource(R.string.video_stats_hint))
                }
            }
            "settings/audio" -> {
                SettingsRow(stringResource(R.string.audio_car_bluetooth), stringResource(R.string.audio_car_bluetooth_hint), R.drawable.symbol_bluetooth,
                    trailing = { LabeledSwitch(stringResource(R.string.audio_car_bluetooth), draft.carBluetoothAudio, { draft = draft.copy(carBluetoothAudio = it) }) })
            }
            "settings/connection" -> {
                CollapsibleCard("connection", stringResource(R.string.ui_iphone_connection), stringResource(if (wireless(draft)) R.string.direlay_wireless else R.string.ui_usb),
                    initiallyExpanded = true, targeted = target == "connection" || target == "iphone") {
                    RadioOptions(connectionOptions, if (wireless(draft)) 0 else 1) {
                        val candidate = draft.copy(connectionMode = if (it == 0) ConnectionMode.WIRELESS else ConnectionMode.USB)
                        if (needsUsbConfirmation(draft.connectionMode, candidate.connectionMode, wireless(draft))) confirmUsb = true
                        else draft = CarlinkConnectionPolicy.enabling(draft, candidate, wireless(candidate))
                    }
                    AnimatedVisibility(wireless(draft), enter = expandVertically(tween(ms)) + fadeIn(tween(ms)),
                        exit = shrinkVertically(tween(ms)) + fadeOut(tween(ms))) {
                        Column {
                            IphonePickerSetting(draft.iphoneAddress, bondedDevices, target == "iphone" && !iphoneLocated) {
                                draft = draft.copy(iphoneAddress = it)
                            }
                            LaunchedEffect(Unit) { iphoneLocated = true }
                        }
                    }
                }
                if (wireless(draft)) SettingsMenuRow(R.string.ui_network, stringResource(networkOption(draft.wirelessNetwork.choice).title),
                    R.drawable.symbol_wifi, "settings/connection/network" in invalidPages) { open("settings/connection/network") }
            }
            "settings/connection/network" -> {
                Column(Modifier.selectableGroup()) {
                    WirelessNetworkChoice.entries.forEach { choice ->
                        val key = choice.name
                        CollapsibleCard(key, stringResource(networkOption(choice).title), stringResource(networkOption(choice).supporting),
                            initiallyExpanded = draft.wirelessNetwork.choice == choice && target.isEmpty(),
                            selected = draft.wirelessNetwork.choice == choice, error = draft.wirelessNetwork.choice == choice && "settings/connection/network" in invalidPages, targeted = target == key || (target == "sharedWifi" && choice == WirelessNetworkChoice.EXISTING_WIFI)) {
                            RadioOptionRow(networkOption(choice), draft.wirelessNetwork.choice == choice,
                                extra = {
                                    NetworkDiagram(choice, draft.wirelessNetwork.choice == choice)
                                    if (choice == WirelessNetworkChoice.PHONE_HOTSPOT)
                                        NetworkChoiceReminder(org.openxplay.direlay.network.NetworkRequirement.HOTSPOT)
                                    if (choice == WirelessNetworkChoice.WIFI_DIRECT && !CarlinkConnectionPolicy.wifiDirectAllowed(draft))
                                        Hint(stringResource(R.string.carlink_wifi_direct_constraint))
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
                SettingsRow(stringResource(R.string.ui_automatic), stringResource(R.string.settings_automatic_hint), R.drawable.symbol_sync,
                    trailing = { LabeledSwitch(stringResource(R.string.ui_automatic), draft.autoStartWhenReady, { draft = draft.copy(autoStartWhenReady = it) }) })
                SettingsRow(stringResource(R.string.ui_boot), stringResource(R.string.direlay_boot_hint), R.drawable.symbol_play,
                    trailing = { LabeledSwitch(stringResource(R.string.ui_boot), draft.autoStartOnBoot, { draft = draft.copy(autoStartOnBoot = it) }) })
            }
            "settings/access" -> {
                CollapsibleCard("admin", stringResource(R.string.admin_page), accessSummary, initiallyExpanded = true) { adminPage() }
                SettingsMenuRow(R.string.public_name_title, stringResource(R.string.public_name_summary), R.drawable.symbol_link) {
                    open("settings/access/publicName")
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
@Composable fun RadioOptionRow(option: RadioOption, selected: Boolean, extra: @Composable (() -> Unit)? = null, enabled: Boolean = true, select: () -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp)
        .selectable(enabled = enabled, selected = selected, role = Role.RadioButton, onClick = select).padding(vertical = 12.dp),
        // With a diagram below, the radio button stays level with the title instead of the row centre.
        verticalAlignment = if (extra == null) Alignment.CenterVertically else Alignment.Top) {
        RadioButton(selected = selected, enabled = enabled, onClick = null, modifier = Modifier.padding(horizontal = 12.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(stringResource(option.title), style = MaterialTheme.typography.bodyLarge)
            Text(stringResource(option.supporting), style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            extra?.invoke()
        }
    }
}
@OptIn(ExperimentalLayoutApi::class)
@Composable fun Options(labels: List<String>, selected: Int, enabled: Boolean = true, select: (Int) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        labels.forEachIndexed { index, label -> FilterChip(index == selected, { select(index) },
            modifier = Modifier.heightIn(min = 48.dp), enabled = enabled, label = { Text(label) }) }
    }
}
@Composable fun SettingField(label: String, value: String, error: Int? = null, number: Boolean = false,
    secret: Boolean = false, enabled: Boolean = true, change: (String) -> Unit) {
    OutlinedTextField(value, change, Modifier.fillMaxWidth(), label = { Text(label) }, singleLine = true,
        enabled = enabled, isError = error != null, supportingText = error?.let { { Text(stringResource(it)) } },
        keyboardOptions = KeyboardOptions(keyboardType = if (number) KeyboardType.Number else if (secret) KeyboardType.Password else KeyboardType.Text),
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None)
}
@Composable fun KeyValue(label: String, value: String, copy: ((String) -> Unit)? = null) {
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(value, Modifier.weight(1f), fontFamily = FontFamily.Monospace)
            if (copy != null) IconButton(onClick = { copy(value) }) { Symbol(R.drawable.symbol_copy, stringResource(R.string.settings_copy_value, label)) }
        }
    }
    HorizontalDivider()
}
@Composable fun AboutScreen(back: () -> Unit) {
    val context = LocalContext.current
    fun link(url: String) { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
    DetailPage(stringResource(R.string.direlay_about), back) {
        CollapsibleCard("project", stringResource(R.string.direlay_about), stringResource(R.string.about_project), initiallyExpanded = true) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Image(painterResource(R.mipmap.ic_launcher_foreground), null, Modifier.size(64.dp))
                RelayBrand()
            }
            Text(context.packageManager.getPackageInfo(context.packageName, 0).versionName.orEmpty(),
                style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            SettingsRow(stringResource(R.string.direlay_attribution), stringResource(R.string.about_project), R.drawable.symbol_info,
                { link(context.getString(R.string.direlay_project_url)) })
            SettingsRow(stringResource(R.string.about_source), stringResource(R.string.about_source_hint), R.drawable.symbol_code,
                { link(context.getString(R.string.direlay_source_url)) })
            SettingsRow(stringResource(R.string.about_licence), stringResource(R.string.about_licence_hint), R.drawable.symbol_info,
                { link("https://github.com/OpenXPlay/OpenXPlay-Relay/blob/main/NOTICE") })
        }
        SmallHeading(stringResource(R.string.direlay_acknowledgements))
        Text(stringResource(R.string.direlay_acknowledgement_text))
        SettingsRow("DiPlay", stringResource(R.string.about_diplay_hint), R.drawable.symbol_car,
            { link(context.getString(R.string.direlay_diplay_url)) })
        SettingsRow("xcertplay", stringResource(R.string.about_xcertplay_hint), R.drawable.symbol_phone,
            { link(context.getString(R.string.direlay_xcertplay_url)) })
        Text(stringResource(R.string.direlay_trademark), Modifier.padding(top = 16.dp),
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
