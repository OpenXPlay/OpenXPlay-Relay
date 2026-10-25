// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
package org.openxplay.direlay.ui

import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.ui.design.*
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.video.CarlinkGeometry
import org.openxplay.direlay.video.VideoCompatibility

data class DeveloperRow(val label: Int, val value: String, val argument: String = "", val valueKey: Int = 0,
    val valueArguments: List<String> = emptyList())
data class DeveloperView(val rows: List<DeveloperRow> = emptyList(), val urls: List<String> = emptyList(),
    val fingerprint: String = "", val identity: Int = R.string.direlay_missing,
    val carlinkGeometry: CarlinkGeometry? = null, val carlinkGeometryMode: VideoCompatibility.Mode? = null)
@Composable fun DeveloperScreen(state: SettingsEditorState, view: DeveloperView, route: String, back: () -> Unit,
    open: (String) -> Unit, copy: (String) -> Unit, reconnect: () -> Unit,
    sample: Boolean, setSample: (Boolean) -> Unit, openVehicle: () -> Unit, wireless: (RelaySettings) -> Boolean,
    sources: org.openxplay.direlay.presentation.ScreenSources, actions: org.openxplay.direlay.presentation.ScreenActions,
    shareTrace: (java.io.File) -> Unit) {
    val invalidPages = state.errorPages(wireless)
    val draft = state.draft.value
    DetailPage(stringResource(developerPageTitle(route)), back) {
        when (route) {
            "developer" -> {
                SettingsToggleGroup(stringResource(R.string.dev_use_options), state.developerDraft.value.active,
                    R.drawable.symbol_code, IconHue.DEVELOPER, footer = stringResource(R.string.dev_use_options_hint)) { enabled ->
                    state.changeDeveloper { it.copy(active = enabled) }
                }
                InsetGroup {
                    SettingsMenuRow(R.string.design_gallery, null, R.drawable.symbol_settings_2) { open("developer/design") }
                    InsetDivider()
                    SettingsMenuRow(R.string.menu_diagnostics, stringResource(R.string.menu_diagnostics_summary, view.rows.size, view.urls.size), R.drawable.symbol_info) { open("developer/diagnostics") }
                    InsetDivider()
                    SettingsMenuRow(R.string.ui_carlink, "${hevcSummary(draft)} · ${menuToggleSummary(draft.carlinkTrace)}", R.drawable.symbol_car) { open("developer/carlink") }
                    InsetDivider()
                    SettingsMenuRow(R.string.menu_video, developerDisplaySummary(draft), R.drawable.symbol_settings,
                        "developer/video" in invalidPages) { open("developer/video") }
                    InsetDivider()
                    SettingsMenuRow(R.string.menu_connection_outputs, "${outputSummary(draft)} · ${connectionModeSummary(draft)}", R.drawable.symbol_wifi,
                        "developer/connection" in invalidPages) { open("developer/connection") }
                }
            }
            "developer/diagnostics" -> DeveloperDiagnostics(view, sample, setSample, openVehicle, reconnect, copy,
                enabled = state.developerDraft.value.active)
            "developer/carlink" -> DeveloperCarlink(state, view, sources, actions, shareTrace)
            "developer/video" -> DeveloperVideo(state)
            "developer/connection" -> DeveloperConnection(state, wireless)
        }
        if (state.savingError) Text(stringResource(R.string.settings_save_error), color = MaterialTheme.colorScheme.error)
        if (route in invalidPages || (route == "developer" && invalidPages.any { it.startsWith("developer/") }))
            Text(stringResource(R.string.settings_advanced_error), color = MaterialTheme.colorScheme.error)
    }
}

fun developerPageTitle(route: String) = when (route) {
    "developer/diagnostics" -> R.string.menu_diagnostics
    "developer/carlink" -> R.string.ui_carlink
    "developer/video" -> R.string.menu_video
    "developer/connection" -> R.string.menu_connection_outputs
    else -> R.string.ui_developer
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
