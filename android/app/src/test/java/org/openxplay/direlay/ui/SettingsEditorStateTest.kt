// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.clink.ClinkPictureSize

class SettingsEditorStateTest {
    private val initial = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null)
    private fun editor() = SettingsEditorState(mutableStateOf(initial), mutableStateOf(initial),
        mutableStateOf(false), mutableStateMapOf(), mutableStateMapOf())

    @Test fun parkedWindowFitChoicesSurviveMenuChangesAndApplyWhenEnabled() {
        val state = editor()
        state.synchronize(initial, true)
        state.changeDeveloper { it.copy(carlinkPictureSize = ClinkPictureSize.MODE) }
        assertEquals(initial, state.draft.value)
        state.synchronize(initial, true, DeveloperOverrides())
        val stack = RelayBackStack().open("developer/carlink").open("settings/startup")
        state.draft.value = state.draft.value.copy(autoStartOnBoot = true)
        state.changeDeveloper { it.copy(carlinkPictureSizeEnabled = true) }
        assertEquals("settings/startup", stack.current)
        assertEquals(ClinkPictureSize.MODE, state.draft.value.carlinkPictureSize)
        assertTrue(state.draft.value.autoStartOnBoot)
        state.changeDeveloper { it.copy(active = false) }
        assertEquals(ClinkPictureSize.WINDOW, state.draft.value.carlinkPictureSize)
        assertEquals(ClinkPictureSize.MODE, state.developerDraft.value.carlinkPictureSize)
        state.changeDeveloper { it.copy(active = true) }
        assertEquals(ClinkPictureSize.MODE, state.draft.value.carlinkPictureSize)
        state.synchronize(initial, false, DeveloperOverrides())
        assertEquals(initial, state.draft.value)
        assertEquals(DeveloperOverrides(), state.developerDraft.value)
    }

    @Test fun normalAndDeveloperPagesKeepTheSameUnpublishedDraft() {
        val state = editor()
        var stack = RelayBackStack().open("settings:output")
        state.draft.value = state.draft.value.copy(carlinkDeviceName = "")
        stack = stack.open("developer/video")
        state.customDisplay = true
        state.updateDisplay("width", "1920")
        stack = stack.open("settings/startup")
        state.draft.value = state.draft.value.copy(autoStartOnBoot = true)
        assertEquals("settings/startup", stack.current)
        assertEquals("", state.draft.value.carlinkDeviceName)
        assertEquals(DisplayProfile(1920, 720, 160), state.draft.value.display)
        assertTrue(state.draft.value.autoStartOnBoot)
        assertEquals(initial, state.lastSaved.value)
        assertEquals(setOf("settings/output"), state.errorPages { false })
    }

    @Test fun incompleteCustomNumbersSurvivePageChangesUntilTheyAreValid() {
        val state = editor()
        state.customDisplay = true
        state.updateDisplay("width", "")
        assertEquals("", state.numberText["width"])
        assertEquals(setOf("developer/video"), state.errorPages { false })
        assertEquals(initial, state.draft.value)
        val stack = RelayBackStack().open("developer/video").back().open("settings/audio")
        state.draft.value = state.draft.value.copy(carBluetoothAudio = true)
        assertEquals("settings/audio", stack.current)
        assertEquals("", state.numberText["width"])
        state.updateDisplay("height", "1080")
        assertTrue("width" in state.invalidNumbers)
        state.updateDisplay("width", "1920")
        assertTrue(state.invalidNumbers.isEmpty())
        assertEquals(DisplayProfile(1920, 1080, 160), state.draft.value.display)
        assertTrue(state.draft.value.carBluetoothAudio)
        assertTrue(state.errorPages { false }.isEmpty())
    }
}
