// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.snapshots.SnapshotStateMap
import androidx.compose.ui.res.stringResource
import org.openxplay.direlay.R
import kotlinx.coroutines.flow.collect
import org.openxplay.direlay.settings.*
import org.openxplay.direlay.settings.DisplayMode

/** A single editing session, shared by every settings and developer destination. */
@Stable
class SettingsEditorState(val draft: MutableState<RelaySettings>, val lastSaved: MutableState<RelaySettings>,
    val customDisplayState: MutableState<Boolean>,
    val numberText: SnapshotStateMap<String, String>,
    val invalidNumbers: SnapshotStateMap<String, Boolean>,
    private val sessionActive: MutableState<Boolean> = mutableStateOf(false),
    val developerDraft: MutableState<DeveloperOverrides> = mutableStateOf(DeveloperOverrides.migrate(draft.value, 0)),
    val lastSavedDeveloper: MutableState<DeveloperOverrides> = mutableStateOf(developerDraft.value)) {
    private var generation by mutableIntStateOf(0)
    private var observedInitial: RelaySettings? = null
    private var observedDeveloper: DeveloperOverrides? = null
    var savingError by mutableStateOf(false)
    var customDisplay by customDisplayState
    /** Called at the app boundary; leaving the entire menu discards unpublished editing state. */
    fun synchronize(initial: RelaySettings, editing: Boolean, initialDeveloper: DeveloperOverrides = lastSavedDeveloper.value) {
        if (editing && sessionActive.value && observedInitial == initial && observedDeveloper == initialDeveloper) return
        observedInitial = initial
        observedDeveloper = initialDeveloper
        if (!editing || !sessionActive.value) {
            if (sessionActive.value != editing || draft.value != initial || lastSaved.value != initial ||
                developerDraft.value != initialDeveloper || lastSavedDeveloper.value != initialDeveloper ||
                numberText.isNotEmpty() || invalidNumbers.isNotEmpty() || savingError) {
                generation++
                draft.value = initial
                lastSaved.value = initial
                developerDraft.value = initialDeveloper
                lastSavedDeveloper.value = initialDeveloper
                numberText.clear()
                invalidNumbers.clear()
                customDisplay = initial.hasCustomDisplay()
                savingError = false
            }
        } else if (draft.value == lastSaved.value && developerDraft.value == lastSavedDeveloper.value) {
            if (initial != lastSaved.value && invalidNumbers.isEmpty()) {
                numberText.clear()
                customDisplay = initial.hasCustomDisplay()
            }
            val car = draft.value.car.takeIf { it.normalized() == initial.car } ?: initial.car
            draft.value = initial.copy(car = car)
            lastSaved.value = draft.value
            developerDraft.value = initialDeveloper
            lastSavedDeveloper.value = initialDeveloper
        }
        sessionActive.value = editing
    }

    /** Remember disabled choices while keeping the typed draft equal to the effective settings. */
    fun changeDeveloper(change: (DeveloperOverrides) -> DeveloperOverrides) {
        val next = change(developerDraft.value)
        developerDraft.value = next
        draft.value = next.effectiveSettings(draft.value)
    }

    fun changeExperimental(change: (ExperimentalOptions) -> ExperimentalOptions) {
        draft.value = draft.value.copy(experimental = change(draft.value.experimental))
    }

    /** Field text errors belong to that field; the typed draft contains only parsed values. */
    fun editNumber(key: String, text: String, valid: (String) -> Boolean, change: (String) -> Unit) {
        numberText[key] = text
        if (valid(text)) {
            change(text)
            invalidNumbers.remove(key)
        } else invalidNumbers[key] = true
    }

    internal suspend fun collectChanges(wireless: (RelaySettings) -> Boolean,
        saveDeveloper: (suspend (RelaySettings, RelaySettings, DeveloperOverrides) -> Boolean)? = null,
        save: suspend (RelaySettings, RelaySettings) -> Boolean) {
        snapshotFlow { if (sessionActive.value) Triple(draft.value, developerDraft.value, generation) else null }.collect { pending ->
            if (pending != null) {
                val (candidate, developer, version) = pending
                if ((candidate != lastSaved.value || developer != lastSavedDeveloper.value) &&
                    settingsSaveErrors(candidate, wireless(candidate)).isEmpty()) {
                    val saved = saveDeveloper?.invoke(lastSaved.value, candidate, developer) ?: save(lastSaved.value, candidate)
                    // An in-flight publication must not restore a session that was already closed.
                    if (sessionActive.value && generation == version) {
                        if (saved) {
                            lastSaved.value = candidate
                            lastSavedDeveloper.value = developer
                        }
                        savingError = !saved
                    }
                }
            }
        }
    }

    fun errorPages(wireless: (RelaySettings) -> Boolean): Set<String> = settingsInvalidPages(draft.value, lastSaved.value, wireless) +
        invalidNumbers.keys.map { when (it) {
            in wirelessPictureNumberKeys -> "settings/output"
            "width", "height", "density" -> "developer/video"
            else -> "developer/connection"
        } }
    fun chooseWirelessPicture(mode: WirelessPictureMode, automatic: WirelessPictureSize) {
        wirelessPictureNumberKeys.forEach { invalidNumbers.remove(it); numberText.remove(it) }
        draft.value = org.openxplay.direlay.relay.WirelessPicturePolicy.choose(draft.value, mode, automatic)
    }
    fun updateWirelessPicture(key: String, text: String) {
        require(key in wirelessPictureNumberKeys)
        numberText[key] = text
        val current = draft.value.wirelessPictureSize
        val size = parseWirelessPictureSize(numberText["wirelessPictureWidth"] ?: current.width.toString(),
            numberText["wirelessPictureHeight"] ?: current.height.toString())
        wirelessPictureNumberKeys.forEach { if (size == null) invalidNumbers[it] = true else invalidNumbers.remove(it) }
        if (size != null) draft.value = draft.value.copy(wirelessPictureSize = size)
    }
    fun presetWirelessPicture(size: WirelessPictureSize) {
        wirelessPictureNumberKeys.forEach { invalidNumbers.remove(it); numberText.remove(it) }
        draft.value = draft.value.copy(wirelessPictureSize = size)
    }
    fun clearDisplayErrors() {
        listOf("width", "height", "density").forEach { invalidNumbers.remove(it) }
    }
    fun updateDisplay(key: String? = null, value: String = "") {
        if (key != null) numberText[key] = value
        val current = draft.value.display ?: DisplayProfile(1280, 720, 160)
        val width = (numberText["width"] ?: current.width.toString()).toIntOrNull()
        val height = (numberText["height"] ?: current.height.toString()).toIntOrNull()
        val density = (numberText["density"] ?: current.density.toString()).toIntOrNull()
        listOf("width" to width, "height" to height, "density" to density).forEach { (name, number) ->
            if (number == null || !displayNumberValid(name, number.toString())) invalidNumbers[name] = true else invalidNumbers.remove(name)
        }
        if (width != null && height != null && density != null && listOf("width", "height", "density").none { it in invalidNumbers })
            draft.value = draft.value.copy(displayMode = DisplayMode.FIXED, display = DisplayProfile(width, height, density))
    }
}

internal val wirelessPictureNumberKeys = listOf("wirelessPictureWidth", "wirelessPictureHeight")

internal fun parseWirelessPictureSize(width: String, height: String): WirelessPictureSize? =
    width.toIntOrNull()?.let { w -> height.toIntOrNull()?.let { h ->
        WirelessPictureSize(w, h).takeIf { runCatching { it.validate() }.isSuccess }
    } }

internal fun displayNumberValid(key: String, text: String): Boolean {
    val number = text.toIntOrNull() ?: return false
    return when (key) {
        "width" -> number in 320..3840 && number % 2 == 0
        "height" -> number in 240..2160 && number % 2 == 0
        "density" -> number in 120..640
        else -> false
    }
}

/** SnapshotStateMap.toMap() returns a persistent map that Android cannot put in a Bundle. */
private fun <V> editorMapSaver() = Saver<SnapshotStateMap<String, V>, HashMap<String, V>>(
    save = { HashMap(it) }, restore = { it.toList().toMutableStateMap() })

internal val settingsNumberTextSaver = editorMapSaver<String>()
internal val settingsInvalidNumbersSaver = editorMapSaver<Boolean>()

private fun RelaySettings.hasCustomDisplay() = displayMode == DisplayMode.FIXED && display != null && display !in DisplayProfile.presets

@Composable
fun rememberSettingsEditorState(initial: RelaySettings, editing: Boolean, wireless: (RelaySettings) -> Boolean,
    save: suspend (RelaySettings, RelaySettings) -> Boolean,
    initialDeveloper: DeveloperOverrides = DeveloperOverrides.migrate(initial, 0),
    saveDeveloper: (suspend (RelaySettings, RelaySettings, DeveloperOverrides) -> Boolean)? = null): SettingsEditorState {
    val draft = rememberSaveable(stateSaver = relaySettingsSaver(initial)) { mutableStateOf(initial) }
    val lastSaved = rememberSaveable(stateSaver = relaySettingsSaver(initial)) { mutableStateOf(initial) }
    val custom = rememberSaveable { mutableStateOf(initial.hasCustomDisplay()) }
    val numbers = rememberSaveable(saver = settingsNumberTextSaver) { mutableStateMapOf<String, String>() }
    val invalid = rememberSaveable(saver = settingsInvalidNumbersSaver) { mutableStateMapOf<String, Boolean>() }
    val active = rememberSaveable { mutableStateOf(editing) }
    val developer = rememberSaveable(stateSaver = developerOverridesSaver(initialDeveloper)) { mutableStateOf(initialDeveloper) }
    val savedDeveloper = rememberSaveable(stateSaver = developerOverridesSaver(initialDeveloper)) { mutableStateOf(initialDeveloper) }
    val state = remember { SettingsEditorState(draft, lastSaved, custom, numbers, invalid, active, developer, savedDeveloper) }
    val currentSave by rememberUpdatedState(save)
    val currentSaveDeveloper by rememberUpdatedState(saveDeveloper)
    SideEffect { state.synchronize(initial, editing, initialDeveloper) }
    // One collector serializes publications. Destination transitions do not create new collectors.
    LaunchedEffect(state) {
        state.collectChanges(wireless, { before, after, choices ->
            currentSaveDeveloper?.invoke(before, after, choices) ?: currentSave(before, after)
        }) { before, after -> currentSave(before, after) }
    }
    return state
}

@Composable
fun EditorNumberField(state: SettingsEditorState, key: String, label: Int, initial: String,
    valid: (String) -> Boolean, enabled: Boolean = true, change: (String) -> Unit) {
    val stored = state.numberText[key]
    val text = if (stored != null && (key in state.invalidNumbers || stored.toIntOrNull() == initial.toIntOrNull())) stored else initial
    SettingField(stringResource(label), text,
        if (key in state.invalidNumbers) R.string.settings_advanced_error else null, number = true, enabled = enabled) {
        state.editNumber(key, it, valid, change)
    }
}
