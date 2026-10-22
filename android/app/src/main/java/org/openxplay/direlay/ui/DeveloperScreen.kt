// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.shilapi.xcertplay.orchestration.*
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*
// material3 also defines DisplayMode (date pickers).
import org.openxplay.direlay.settings.DisplayMode
import org.openxplay.direlay.relay.RelayRuntime
import kotlinx.coroutines.*
import org.openxplay.direlay.storage.MaterialStatus
import org.openxplay.direlay.video.CarlinkGeometry
import org.openxplay.direlay.video.VideoCompatibility

internal fun carlinkGeometryValues(geometry: CarlinkGeometry) = with(geometry) {
    listOf("${window.width}×${window.height}", "${canvas.width}×${canvas.height}",
        "${area.x},${area.y} ${area.width}×${area.height}", "${picture.width}×${picture.height}")
}

data class DeveloperRow(val label: Int, val value: String, val argument: String = "", val valueKey: Int = 0,
    val valueArguments: List<String> = emptyList())
data class DeveloperView(val rows: List<DeveloperRow> = emptyList(), val urls: List<String> = emptyList(),
    val fingerprint: String = "", val identity: Int = R.string.direlay_missing,
    val carlinkGeometry: CarlinkGeometry? = null, val carlinkGeometryMode: VideoCompatibility.Mode? = null)
fun developerView(runtime: RelayRuntime): DeveloperView {
    return DeveloperView(buildList {
    runtime.carPlayScreenSummary.let { (size, source) ->
        add(DeveloperRow(R.string.dev_carplay_screen, "", valueKey = when (source) {
            "car screen" -> R.string.dev_screen_car
            "remembered" -> R.string.dev_screen_remembered
            "built-in model" -> R.string.dev_screen_model
            "default" -> R.string.dev_screen_default
            "fixed" -> R.string.dev_screen_fixed
            else -> R.string.dev_screen_relay
        }, valueArguments = listOf(size)))
    }
    runtime.displayPolicy.primaryDisplay?.viewport?.let {
        add(DeveloperRow(R.string.picture_window, "${it.width} × ${it.height} · ${"%.2f".format(java.util.Locale.ROOT, it.width.toDouble() / it.height)}:1"))
    }
    add(DeveloperRow(R.string.picture_video_codec, "${runtime.status.videoCodec ?: runtime.appliedVideoDecision.codec} · ${runtime.videoDecision.reason}"))
    val webReason = runtime.serverReason
    add(DeveloperRow(R.string.dev_receiver, "", valueKey = when (webReason?.failure) {
        org.openxplay.direlay.relay.WebStartFailure.PORT_BUSY -> R.string.ui_web_port_busy
        org.openxplay.direlay.relay.WebStartFailure.OTHER -> R.string.ui_web_start_failed
        null -> developerStateKey(runtime.serverState)
    }, valueArguments = if (webReason?.failure == org.openxplay.direlay.relay.WebStartFailure.PORT_BUSY)
        listOf(webReason.port.toString()) else emptyList()))
    add(DeveloperRow(R.string.ui_carlink, "", valueKey = developerStateKey(runtime.carlinkState)))
    add(DeveloperRow(R.string.dev_iphone_selection, runtime.iphoneSelection().diagnostic()))
    add(DeveloperRow(R.string.dev_session, "", valueKey = developerStateKey(runtime.status.session?.phase?.name ?: "idle")))
    runtime.status.frameCounts.toSortedMap().forEach { (screen, frames) -> add(DeveloperRow(R.string.dev_frames, frames.toString(), screen.toString())) }
    add(DeveloperRow(R.string.dev_clients, runtime.status.recentWebReporters.toString()))
    runtime.status.outputMeasurements.forEach { (output, stats) ->
        val source = "$output / ${stats.screen.value}"
        add(DeveloperRow(R.string.dev_decode, stats.decodeTimeUs?.toString() ?: "—", source))
        add(DeveloperRow(R.string.dev_queue, stats.decodeQueue.toString(), source))
        add(DeveloperRow(R.string.dev_dropped, stats.dropped.toString(), source))
        add(DeveloperRow(R.string.dev_rendered, stats.rendered.toString(), source))
        add(DeveloperRow(R.string.dev_latency, stats.endToEndLatencyUs?.toString() ?: "—", source))
        add(DeveloperRow(R.string.dev_audio_buffer, stats.audioBufferedUs?.toString() ?: "—", source))
        add(DeveloperRow(R.string.dev_underruns, stats.audioUnderruns.toString(), source))
    }
    runtime.input.keyframeLog.forEachIndexed { index, item ->
        val parts = Regex("(\\d+): Screen (\\d+): (.+)").matchEntire(item)?.groupValues
        val key = when (parts?.get(3)) {
            "requested" -> R.string.dev_keyframe_requested
            "no recovery handler" -> R.string.dev_keyframe_no_handler
            "request failed" -> R.string.dev_keyframe_failed
            "stale request" -> R.string.dev_keyframe_stale
            "queue full" -> R.string.dev_keyframe_queue_full
            else -> R.string.dev_unavailable
        }
        add(DeveloperRow(R.string.dev_keyframe, "", index.toString(), key,
            if (parts == null) emptyList() else listOf(snapshotTime(parts[1].toLong()), parts[2])))
    }
    runtime.status.failure?.let { add(DeveloperRow(R.string.dev_failure, "", valueKey = R.string.ui_error)) }
}, runtime.receiverUrls, runCatching { runtime.tlsStore.fingerprint() }.getOrNull() ?: "—",
    when (runtime.identityStore.status()) {
        MaterialStatus.Installed -> R.string.direlay_installed
        MaterialStatus.Missing -> R.string.direlay_missing
        is MaterialStatus.Invalid -> R.string.dev_invalid_identity
    }, runtime.carlinkGeometry, runtime.carlinkGeometryMode)
}

@Composable fun DeveloperScreen(initial: RelaySettings, overrides: DeveloperOverrides, view: DeveloperView, back: () -> Unit,
    save: suspend (RelaySettings, RelaySettings) -> Boolean, copy: (String) -> Unit,
    saveOverrides: suspend ((DeveloperOverrides) -> DeveloperOverrides) -> Boolean,
    reconnect: () -> Unit, import: (Boolean) -> Unit, remove: (Boolean) -> Unit,
    sample: Boolean, setSample: (Boolean) -> Unit, openVehicle: () -> Unit, wireless: (RelaySettings) -> Boolean,
    debug: RelayRuntime? = null) {
    var draft by rememberSaveable(stateSaver = relaySettingsSaver(initial)) { mutableStateOf(initial) }
    var saved by rememberSaveable(stateSaver = relaySettingsSaver(initial)) { mutableStateOf(initial) }
    var port by rememberSaveable { mutableStateOf(overrides.webPort?.toString().orEmpty()) }
    var model by rememberSaveable { mutableStateOf(overrides.carlinkDeviceModel) }
    var knownPort by remember { mutableStateOf(overrides.webPort) }
    var knownModel by remember { mutableStateOf(overrides.carlinkDeviceModel) }
    var custom by rememberSaveable { mutableStateOf(draft.displayMode == DisplayMode.FIXED && draft.display != null && draft.display !in DisplayProfile.presets) }
    var width by rememberSaveable { mutableStateOf((draft.display?.width ?: 1280).toString()) }
    var height by rememberSaveable { mutableStateOf((draft.display?.height ?: 720).toString()) }
    var density by rememberSaveable { mutableStateOf((draft.display?.density ?: 160).toString()) }
    var channel by rememberSaveable { mutableStateOf(draft.wirelessNetwork.channel.toString()) }
    val invalidPort = port.isNotEmpty() && port.toIntOrNull() !in 1024..65535
    val invalidModel = model.isBlank() || model.toByteArray(Charsets.UTF_8).size > 128 || model.any { Character.isISOControl(it) }
    val invalidNumber = channel.toIntOrNull() == null || (custom && listOf(width, height, density).any { it.toIntOrNull() == null })
    val candidate = draft.copy(displayMode = if (custom) DisplayMode.FIXED else draft.displayMode, display = if (custom && !invalidNumber)
        DisplayProfile(width.toInt(), height.toInt(), density.toInt()) else draft.display,
        wirelessNetwork = draft.wirelessNetwork.copy(channel = channel.toIntOrNull() ?: 0))
    var confirmCarlink by remember { mutableStateOf(false) }
    if (confirmCarlink) AlertDialog(onDismissRequest = { confirmCarlink = false },
        title = { Text(stringResource(R.string.ui_carlink)) },
        text = { Text(stringResource(R.string.carlink_wifi_direct_confirm)) },
        confirmButton = { TextButton(onClick = {
            val next = draft.copy(bothOutputs = true)
            draft = CarlinkConnectionPolicy.enabling(draft, next, wireless(next))
            confirmCarlink = false
        }) { Text(stringResource(android.R.string.ok)) } },
        dismissButton = { TextButton(onClick = { confirmCarlink = false }) { Text(stringResource(android.R.string.cancel)) } })
    var failed by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    fun changeOverrides(change: (DeveloperOverrides) -> DeveloperOverrides) {
        scope.launch { failed = !saveOverrides(change) }
    }
    LaunchedEffect(overrides.webPort) {
        if (port == knownPort?.toString().orEmpty()) port = overrides.webPort?.toString().orEmpty()
        knownPort = overrides.webPort
    }
    LaunchedEffect(overrides.carlinkDeviceModel) {
        if (model == knownModel) model = overrides.carlinkDeviceModel
        knownModel = overrides.carlinkDeviceModel
    }
    LaunchedEffect(port) {
        val value = port.toIntOrNull()
        if (!invalidPort && value != overrides.webPort)
            failed = !saveOverrides { it.copy(webPort = value) }
    }
    LaunchedEffect(model) {
        val value = model
        if (!invalidModel && value != overrides.carlinkDeviceModel)
            failed = !saveOverrides { it.copy(carlinkDeviceModel = value) }
    }
    LaunchedEffect(candidate, invalidNumber) {
        if (!invalidNumber && candidate != saved && settingsSaveErrors(candidate, wireless(candidate)).isEmpty()) {
            if (save(saved, candidate)) { saved = candidate; failed = false } else failed = true
        }
    }
    var confirm by remember { mutableStateOf<Boolean?>(null) }
    val ms = if (LocalRelayMotion.current) 220 else 0
    DetailPage(stringResource(R.string.ui_developer), back) {
        Toggle(stringResource(R.string.dev_use_options), overrides.active) { enabled -> changeOverrides { it.copy(active = enabled) } }
        Hint(stringResource(R.string.dev_use_options_hint))
        Toggle(stringResource(R.string.dev_sample_vehicle), sample && overrides.active, enabled = overrides.active, onChange = setSample)
        Hint(stringResource(R.string.dev_sample_hint), enabled = overrides.active)
        SettingsRow(stringResource(R.string.vehicle_title), stringResource(R.string.dev_sample_open), R.drawable.symbol_car, openVehicle)
        SmallHeading(stringResource(R.string.carlink_debug))
        Toggle(stringResource(R.string.carlink_trace_enabled), overrides.carlinkTrace, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(carlinkTrace = enabled) }
        }
        Toggle(stringResource(R.string.carlink_hevc), overrides.carlinkHevcEnabled, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(carlinkHevcEnabled = enabled) }
        }
        AnimatedVisibility(overrides.carlinkHevcEnabled, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Options(listOf(stringResource(R.string.carlink_hevc_stream_only), stringResource(R.string.carlink_hevc_off),
                    stringResource(R.string.carlink_hevc_signal)), overrides.carlinkHevc.ordinal, enabled = overrides.active) { index ->
                    changeOverrides { it.copy(carlinkHevc = org.openxplay.stream.clink.ClinkHevcMode.entries[index]) }
                }
                Hint(stringResource(R.string.carlink_hevc_hint), enabled = overrides.active)
            }
        }
        Toggle(stringResource(R.string.carlink_picture_size), overrides.carlinkPictureSizeEnabled, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(carlinkPictureSizeEnabled = enabled) }
        }
        AnimatedVisibility(overrides.carlinkPictureSizeEnabled, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Options(listOf(stringResource(R.string.carlink_picture_window), stringResource(R.string.carlink_picture_mode)),
                    overrides.carlinkPictureSize.ordinal, enabled = overrides.active) { index ->
                    changeOverrides { it.copy(carlinkPictureSize = org.openxplay.stream.clink.ClinkPictureSize.entries[index]) }
                }
                Hint(stringResource(R.string.carlink_picture_size_hint), enabled = overrides.active)
            }
        }
        view.carlinkGeometry?.let { geometry ->
            val mode = stringResource(when (view.carlinkGeometryMode) {
                VideoCompatibility.Mode.PASSTHROUGH -> R.string.carlink_passthrough
                VideoCompatibility.Mode.TRANSCODE -> R.string.carlink_transcode
                else -> R.string.dev_unavailable
            })
            KeyValue(stringResource(R.string.carlink_geometry), stringResource(R.string.carlink_geometry_value,
                *(carlinkGeometryValues(geometry) + mode).toTypedArray()))
        }
        if (debug != null) CarlinkDebugPanel(debug, initial.carlinkTrace)
        SmallHeading(stringResource(R.string.car_view_areas))
        if (debug != null) ViewAreasPanel(debug)
        Toggle(stringResource(R.string.dev_physical_width), overrides.physicalWidthEnabled, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(physicalWidthEnabled = enabled,
                physicalWidthOverrideMm = it.physicalWidthOverrideMm.takeIf { width -> width > 0 } ?: 200) }
        }
        AnimatedVisibility(overrides.physicalWidthEnabled, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                val widths = (100..400 step 25).toList()
                Options(widths.map { "$it mm" }, widths.indexOf(overrides.physicalWidthOverrideMm), enabled = overrides.active) { index ->
                    changeOverrides { it.copy(physicalWidthOverrideMm = widths[index]) }
                }
                Hint(stringResource(R.string.dev_physical_width_hint), enabled = overrides.active)
            }
        }
        SmallHeading(stringResource(R.string.dev_runtime))
        KeyValue(stringResource(R.string.dev_connected_clients), stringResource(R.string.dev_unavailable))
        view.rows.forEach { row -> KeyValue(stringResource(row.label, row.argument), if (row.valueKey == 0) row.value else stringResource(row.valueKey, *row.valueArguments.toTypedArray())) }
        OutlinedButton(onClick = reconnect, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.direlay_carlink_reconnect)) }
        SmallHeading(stringResource(R.string.dev_urls))
        view.urls.forEach { KeyValue(stringResource(R.string.dev_url), it, copy) }
        SmallHeading(stringResource(R.string.direlay_tls))
        KeyValue(stringResource(R.string.dev_fingerprint), view.fingerprint, copy)
        Hint(stringResource(R.string.dev_tls_hint))
        OutlinedButton(onClick = { import(false) }) { Text(stringResource(R.string.dev_import_tls)) }
        TextButton(onClick = { confirm = false }) { Text(stringResource(R.string.dev_remove_tls)) }
        SmallHeading(stringResource(R.string.direlay_identity))
        Hint(stringResource(R.string.direlay_identity_hint))
        KeyValue(stringResource(R.string.direlay_identity), stringResource(view.identity))
        OutlinedButton(onClick = { import(true) }) { Text(stringResource(R.string.direlay_manage_identity)) }
        TextButton(onClick = { confirm = true }) { Text(stringResource(R.string.dev_remove_identity)) }
        SmallHeading(stringResource(R.string.direlay_mfi_target))
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
        SmallHeading(stringResource(R.string.direlay_display_profile))
        val profiles = DisplayProfile.presets
        Options(listOf(stringResource(R.string.display_automatic), stringResource(R.string.display_relay_screen)) +
            profiles.map { "${it.width} × ${it.height} · ${it.density} dpi" } + stringResource(R.string.direlay_custom),
            if (custom) profiles.size + 2 else when (draft.displayMode) {
                DisplayMode.AUTOMATIC -> 0
                DisplayMode.RELAY_SCREEN -> 1
                DisplayMode.FIXED -> profiles.indexOf(draft.display) + 2
            }) { index ->
            custom = index == profiles.size + 2
            if (!custom) draft = draft.copy(
                displayMode = when (index) { 0 -> DisplayMode.AUTOMATIC; 1 -> DisplayMode.RELAY_SCREEN; else -> DisplayMode.FIXED },
                display = if (index < 2) null else profiles[index - 2])
        }
        Hint(stringResource(R.string.display_automatic_hint))
        AnimatedVisibility(custom, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            Column {
                SettingField(stringResource(R.string.direlay_width), width, number = true) { width = it }
                SettingField(stringResource(R.string.direlay_height), height, number = true) { height = it }
                SettingField(stringResource(R.string.direlay_density), density, number = true) { density = it }
            }
        }
        Toggle(stringResource(R.string.direlay_fps), overrides.fpsEnabled, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(fpsEnabled = enabled) }
        }
        AnimatedVisibility(overrides.fpsEnabled, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            Options((30..60 step 5).map(Int::toString), (overrides.fps - 30) / 5, enabled = overrides.active) { index ->
                changeOverrides { it.copy(fps = 30 + index * 5) }
            }
        }
        Toggle(stringResource(R.string.direlay_local_preview), overrides.localPreview, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(localPreview = enabled) }
        }
        Toggle(stringResource(R.string.direlay_play_audio), overrides.playCarPlayAudio, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(playCarPlayAudio = enabled) }
        }
        Toggle(stringResource(R.string.direlay_microphone_source), overrides.microphoneSourceEnabled, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(microphoneSourceEnabled = enabled) }
        }
        AnimatedVisibility(overrides.microphoneSourceEnabled, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            Options(listOf(stringResource(R.string.direlay_head_unit), stringResource(R.string.direlay_this_device)),
                MicrophoneSource.entries.indexOf(overrides.microphoneSource), enabled = overrides.active) { index ->
                changeOverrides { it.copy(microphoneSource = MicrophoneSource.entries[index]) }
            }
        }
        SmallHeading(stringResource(R.string.dev_outputs))
        Toggle(stringResource(R.string.dev_web_port), overrides.webPortEnabled, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(webPortEnabled = enabled) }
        }
        AnimatedVisibility(overrides.webPortEnabled, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            SettingField(stringResource(R.string.dev_port), port, if (invalidPort) R.string.settings_advanced_error else null,
                number = true, enabled = overrides.active) { port = it }
        }
        Toggle(stringResource(R.string.direlay_carlink_model), overrides.carlinkDeviceModelEnabled, enabled = overrides.active) { enabled ->
            changeOverrides { it.copy(carlinkDeviceModelEnabled = enabled) }
        }
        AnimatedVisibility(overrides.carlinkDeviceModelEnabled, enter = expandVertically(tween(ms)), exit = shrinkVertically(tween(ms))) {
            SettingField(stringResource(R.string.direlay_carlink_model), model, if (invalidModel) R.string.settings_advanced_error else null,
                enabled = overrides.active) { model = it }
        }
        Toggle(stringResource(R.string.dev_both), draft.bothOutputs) {
            val next = draft.copy(bothOutputs = it)
            if (CarlinkConnectionPolicy.enabling(draft, next, wireless(next)) != next) confirmCarlink = true
            else draft = next
        }
        if (CarlinkConnectionPolicy.enabled(draft)) Hint(stringResource(R.string.carlink_wifi_direct_constraint))
        SmallHeading(stringResource(R.string.direlay_connection_mode))
        Options(ConnectionMode.entries.map { stringResource(when (it) { ConnectionMode.USB -> R.string.direlay_usb; ConnectionMode.WIRELESS -> R.string.direlay_wireless; ConnectionMode.LAST_USED -> R.string.direlay_last_used }) }, ConnectionMode.entries.indexOf(draft.connectionMode)) {
            val next = draft.copy(connectionMode = ConnectionMode.entries[it])
            draft = CarlinkConnectionPolicy.enabling(draft, next, wireless(next))
        }
        SettingField(stringResource(R.string.direlay_hotspot_channel), channel, number = true) { channel = it }
        SmallHeading(stringResource(R.string.dev_hotspot_security))
        Options(ManualHotspotSecurity.entries.map { stringResource(when (it) {
            ManualHotspotSecurity.OPEN -> R.string.dev_security_open
            ManualHotspotSecurity.WPA2 -> R.string.dev_security_wpa2
            ManualHotspotSecurity.WPA3_TRANSITION -> R.string.dev_security_mixed
            ManualHotspotSecurity.WPA3 -> R.string.dev_security_wpa3
        }) }, ManualHotspotSecurity.entries.indexOf(draft.wirelessNetwork.security)) {
            draft = draft.copy(wirelessNetwork = draft.wirelessNetwork.copy(security = ManualHotspotSecurity.entries[it]))
        }
        if (failed) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        if (invalidNumber || settingsErrors(candidate, candidate.connectionMode == ConnectionMode.WIRELESS).isNotEmpty())
            Text(stringResource(R.string.settings_advanced_error), color = MaterialTheme.colorScheme.error)
    }
    confirm?.let { identity -> AlertDialog(onDismissRequest = { confirm = null },
        title = { Text(if (identity) stringResource(R.string.dev_remove_identity_question) else stringResource(R.string.dev_remove_tls_question)) },
        confirmButton = { TextButton(onClick = { remove(identity); confirm = null }) { Text(stringResource(R.string.direlay_remove)) } },
        dismissButton = { TextButton(onClick = { confirm = null }) { Text(stringResource(android.R.string.cancel)) } }) }
}

/** Presentation keys keep protocol enum names and platform status strings out of UI copy. */
fun developerStateKey(value: String): Int = when {
    value == "Disabled" || value == "Web receiver disabled" -> R.string.dev_disabled
    value == "Requires Android 10+" -> R.string.dev_android_required
    value == "Waiting for permissions" -> R.string.dev_permissions
    value == "CONNECTING" || value.startsWith("Starting web") -> R.string.ui_connecting
    value == "AUTHENTICATING" -> R.string.dev_authenticating
    value == "NEGOTIATING" -> R.string.dev_negotiating
    value == "ACTIVE" || value == "PLAYING" -> R.string.dev_active
    value == "RECONNECTING" -> R.string.dev_reconnecting
    value == "ENDED" -> R.string.dev_ended
    value.contains("Receiver assets missing") -> R.string.dev_assets_missing
    value.startsWith("TLS enabled") -> R.string.dev_listening
    value.startsWith("Web receiver unavailable") -> R.string.dev_receiver_failed
    else -> R.string.dev_idle
}
