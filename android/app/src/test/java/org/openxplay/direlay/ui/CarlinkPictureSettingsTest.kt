// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-25
package org.openxplay.direlay.ui

import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableStateMapOf
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.R
import org.openxplay.direlay.settings.*

class CarlinkPictureSettingsTest {
    private val settings = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        autoStartOnBoot = false, autoStartWhenReady = false, localPreview = false, webPort = null,
        outputs = setOf(RelayOutput.CARLINK))
    private val custom = PictureSize(1000, 600)
    private fun editor(value: RelaySettings = settings) = SettingsEditorState(mutableStateOf(value), mutableStateOf(value),
        mutableStateOf(false), mutableStateMapOf(), mutableStateMapOf())

    @Test fun saveStatesDistinguishInvalidDuplicateFullAndAvailable() {
        assertEquals(CarlinkPictureSave.INVALID, carlinkPictureUiModel(settings, null).save)
        assertEquals(CarlinkPictureSave.INVALID, carlinkPictureUiModel(settings, null, width = "1281", height = "720").save)
        assertEquals(CarlinkPictureSave.DUPLICATE, carlinkPictureUiModel(settings, null, width = "1280", height = "720").save)
        val saved = settings.copy(carlinkPictureSizes = listOf(custom))
        assertEquals(CarlinkPictureSave.DUPLICATE, carlinkPictureUiModel(saved, null, width = "1000", height = "600").save)
        val ready = carlinkPictureUiModel(settings, null, width = "1000", height = "600")
        assertEquals(CarlinkPictureSave.READY, ready.save); assertTrue(ready.canSave)
        assertFalse(carlinkPictureUiModel(settings.copy(outputs = setOf(RelayOutput.BROWSER)), null,
            width = "1000", height = "600").canSave)
        assertFalse(carlinkPictureUiModel(settings.copy(displayMode = DisplayMode.FIXED, display = CarDisplayFit.default), null,
            width = "1000", height = "600").canSave)
        val full = settings.copy(carlinkPictureSizes = (0..4).map { PictureSize(1000 + it * 2, 600) })
        assertEquals(CarlinkPictureSave.LIMIT, carlinkPictureUiModel(full, null, width = "1200", height = "600").save)
        assertEquals(CarlinkPictureSave.DUPLICATE, carlinkPictureUiModel(full, null, width = "1000", height = "600").save)
    }
    @Test fun modelOrdersRowsAndShowsTrialDefaultDecoderCanvasAndDisabledReasons() {
        val saved = settings.copy(carlinkPictureSizes = listOf(custom), carlinkPictureDefault = PictureSize(1280, 720))
        val trial = CarlinkPictureTrial(PictureSize(1920, 720))
        val canvas = PictureSize(1568, 588)
        val model = carlinkPictureUiModel(saved, trial, canvas)
        assertEquals(listOf(null) + CarlinkPictureSizes.presets + custom, model.sizes)
        assertEquals(trial.size, model.selected); assertEquals(canvas, model.inUse)
        assertTrue(model.differsFromDefault); assertTrue(model.enabled)
        assertFalse(carlinkPictureUiModel(saved, null).differsFromDefault)
        assertNull(carlinkPictureUiModel(saved, CarlinkPictureTrial(null)).inUse)
        assertEquals(CarlinkPictureAvailability.OUTPUT_OFF,
            carlinkPictureUiModel(saved.copy(outputs = setOf(RelayOutput.BROWSER)), trial).availability)
        val fixed = carlinkPictureUiModel(saved.copy(displayMode = DisplayMode.FIXED, display = CarDisplayFit.default), trial)
        assertFalse(fixed.enabled); assertEquals(CarlinkPictureAvailability.FIXED_DISPLAY, fixed.availability)
    }
    @Test fun aParkedDefaultDoesNotChangeTheRunningConnectionsRadio() {
        val next = settings.copy(carlinkPictureDefault = PictureSize(1920, 720))
        val automatic = carlinkPictureUiModel(next, null, effective = CarlinkPictureTrial(null))
        assertNull(automatic.selected); assertTrue(automatic.differsFromDefault)
        val running = carlinkPictureUiModel(next, null, effective = CarlinkPictureTrial(PictureSize(1280, 720)))
        assertEquals(PictureSize(1280, 720), running.selected)
        assertEquals(PictureSize(1280, 720), running.inUse)
        val requested = carlinkPictureUiModel(next, CarlinkPictureTrial(PictureSize(1024, 768)),
            effective = CarlinkPictureTrial(PictureSize(1280, 720)))
        assertEquals(PictureSize(1024, 768), requested.selected)
    }
    @Test fun saveSelectsOnlyNewValidSizesAndDuplicateInputSelectsItsRow() {
        val editor = editor()
        val selected = mutableListOf<PictureSize?>()
        editor.updateCarlinkPicture("carlinkPictureWidth", "1000")
        editor.updateCarlinkPicture("carlinkPictureHeight", "600")
        assertTrue(selected.isEmpty())
        editor.saveCarlinkPicture(selected::add)
        assertEquals(listOf(custom), editor.draft.value.carlinkPictureSizes)
        assertEquals(listOf(custom), selected)
        assertTrue(editor.numberText.isEmpty())
        editor.saveCarlinkPicture(selected::add)
        assertEquals(1, editor.draft.value.carlinkPictureSizes.size)
        assertEquals(1, selected.size)
        editor.updateCarlinkPicture("carlinkPictureWidth", "1280")
        editor.updateCarlinkPicture("carlinkPictureHeight", "720")
        assertEquals(1, selected.size)
        editor.saveCarlinkPicture(selected::add)
        assertEquals(PictureSize(1280, 720), selected.last())
        assertEquals(listOf(custom), editor.draft.value.carlinkPictureSizes)
        assertTrue(editor.numberText.isEmpty())
        assertTrue(editor.invalidNumbers.isEmpty())
    }
    @Test fun deletingDefaultReturnsToAutomaticAndLeavesOtherSavedSizes() {
        val saved = settings.copy(carlinkPictureSizes = listOf(custom, PictureSize(1200, 600)), carlinkPictureDefault = custom)
        val deleted = saved.deleteCarlinkPicture(custom)
        assertNull(deleted.carlinkPictureDefault)
        assertEquals(listOf(PictureSize(1200, 600)), deleted.carlinkPictureSizes)
        assertNull(carlinkPictureUiModel(deleted, null).selected)
    }
    @Test fun deletingTheUsedSizeClearsItsTrialAndOtherDeletesLeaveItAlone() {
        val other = PictureSize(1200, 600)
        val editor = editor(settings.copy(carlinkPictureSizes = listOf(custom, other), carlinkPictureDefault = other))
        var cleared = 0
        val trial = CarlinkPictureTrial(custom)
        editor.deleteCarlinkPicture(other, trial) { cleared++ }
        assertEquals(0, cleared); assertNull(editor.draft.value.carlinkPictureDefault)
        editor.deleteCarlinkPicture(custom, trial) { cleared++ }
        assertEquals(1, cleared); assertTrue(editor.draft.value.carlinkPictureSizes.isEmpty())
    }
    @Test fun defaultChangesOfferRestartUnlessTheTrialAlreadyUsesThatDefault() {
        val next = settings.copy(carlinkPictureDefault = PictureSize(1920, 720))
        assertEquals(R.string.settings_restart, settingsFeedback(settings, next, true, true)?.action)
        assertNull(settingsFeedback(settings, next, true, true, carlinkPictureTrial = CarlinkPictureTrial(next.carlinkPictureDefault)))
        assertEquals(R.string.settings_restart, settingsFeedback(settings, next, true, true, carlinkPictureTrial = CarlinkPictureTrial(null))?.action)
        assertNull(settingsFeedback(settings, next, false, false)?.action)
        assertNull(settingsFeedback(settings, settings.copy(carlinkPictureSizes = listOf(custom)), true, true))
        assertNull(settingsFeedback(next, settings, true, true, carlinkPictureTrial = CarlinkPictureTrial(null)))
    }
}
