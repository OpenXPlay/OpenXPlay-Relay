// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import org.openxplay.stream.core.FrameRatePreference
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*
import org.openxplay.stream.clink.ClinkPictureSize

class SettingsEditorStateTest {
    private val initial = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null,
        FrameRatePreference.AUTO, false, true, false, null)
    private fun editor() = SettingsEditorState(mutableStateOf(initial), mutableStateOf(initial),
        mutableStateOf(false), mutableStateMapOf(), mutableStateMapOf())

    @Test fun wirelessCustomStartsFromAutoAndKeepsInvalidTextOffTheTypedDraftAndOnTheOutputPage() {
        val state = editor()
        state.chooseWirelessPicture(WirelessPictureMode.CUSTOM, WirelessPictureSize(1920, 720))
        assertEquals(WirelessPictureSize(1920, 720), state.draft.value.wirelessPictureSize)
        state.updateWirelessPicture("wirelessPictureWidth", "")
        assertEquals("", state.numberText["wirelessPictureWidth"])
        assertEquals(setOf("settings/output"), state.errorPages { false })
        assertEquals(WirelessPictureSize(1920, 720), state.draft.value.wirelessPictureSize)
        state.updateWirelessPicture("wirelessPictureWidth", "720")
        state.updateWirelessPicture("wirelessPictureHeight", "1281")
        assertEquals(setOf("settings/output"), state.errorPages { false })
        state.updateWirelessPicture("wirelessPictureHeight", "1280")
        assertEquals(WirelessPictureSize(720, 1280), state.draft.value.wirelessPictureSize)
        assertTrue(state.invalidNumbers.isEmpty())
        state.updateWirelessPicture("wirelessPictureWidth", "999999999999")
        state.presetWirelessPicture(WirelessPictureSize(1280, 480))
        assertTrue(state.invalidNumbers.isEmpty()); assertTrue(state.numberText.isEmpty())
        assertEquals(WirelessPictureSize(1280, 480), state.draft.value.wirelessPictureSize)
        state.updateWirelessPicture("wirelessPictureHeight", "bad")
        state.chooseWirelessPicture(WirelessPictureMode.AUTO, WirelessPictureSize())
        assertTrue(state.invalidNumbers.isEmpty()); assertTrue(state.numberText.isEmpty())
        assertEquals(WirelessPictureMode.AUTO, state.draft.value.wirelessPicture)
        state.draft.value = state.draft.value.copy(wirelessPictureSize = WirelessPictureSize(320, 240))
        assertEquals(setOf("settings/output"), state.errorPages { false })
        assertEquals(mapOf("wirelessPictureSize" to org.openxplay.direlay.R.string.settings_advanced_error),
            settingsSaveErrors(state.draft.value, false))
    }

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
        assertEquals(setOf("settings/picture", "developer/video"), state.errorPages { false })
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
    @Test fun experimentalAddressErrorsFollowThePageAndMasterOffRetainsChoices() {
        val state = editor()
        state.synchronize(initial, true)
        state.changeExperimental { it.copy(active = true, keepScanning = true, sendBluetoothAddress = true, bluetoothAddress = "bad") }
        assertEquals(setOf("experimental"), state.errorPages { false })
        assertTrue(settingsSaveErrors(state.draft.value, false).containsKey("experimental.bluetoothAddress"))
        val stack = RelayBackStack().open("experimental").open("settings/startup")
        state.synchronize(initial, settingsSessionActive(stack.entries))
        assertEquals("bad", state.draft.value.experimental.bluetoothAddress)
        state.changeExperimental { it.copy(active = false) }
        assertTrue(state.errorPages { false }.isEmpty())
        assertTrue(state.draft.value.experimental.keepScanning)
        assertTrue(state.draft.value.experimental.sendBluetoothAddress)
        assertEquals(ExperimentalOptions.OFF, state.draft.value.experimental.effective())
        state.changeExperimental { it.copy(active = true, bluetoothAddress = "A0:B1:C2:D3:E4:F5") }
        assertTrue(state.errorPages { false }.isEmpty())
        assertNull(settingsFeedback(initial, state.draft.value, carlinkConnected = true, running = true))
    }


    @Test fun manualAutomaticManualKeepsTheSharedProfileAndUseReplacesAllFields() {
        val state = editor()
        val reference = DisplayProfile(1920, 720, 240)
        state.chooseDisplayMode(DisplayMode.FIXED, reference)
        assertEquals(reference, state.draft.value.display)
        state.updateDisplay("width", "1600")
        val edited = checkNotNull(state.draft.value.display)
        state.chooseDisplayMode(DisplayMode.AUTOMATIC)
        assertEquals(edited, state.draft.value.display)
        state.chooseDisplayMode(DisplayMode.FIXED, DisplayProfile(800, 480, 160))
        assertEquals(edited, state.draft.value.display)
        state.updateDisplay("height", "")
        assertEquals(setOf("settings/picture", "developer/video"), state.errorPages { false })
        state.useDisplayReference(reference)
        assertEquals(reference, state.draft.value.display)
        assertTrue(state.invalidNumbers.isEmpty()); assertTrue(state.numberText.isEmpty())
        assertTrue(state.customDisplay)
        state.chooseDisplayMode(DisplayMode.AUTOMATIC)
        assertFalse(state.customDisplay)
        state.draft.value = state.draft.value.copy(displayMode = DisplayMode.FIXED, display = DisplayProfile.presets.first())
        state.updateDisplay("width", "1000")
        assertTrue(state.customDisplay)
    }

    @Test fun customWidthStartsFromTheReferenceAndUseClearsInvalidText() {
        val state = editor()
        state.chooseScreenWidth(referenceMm = 250)
        assertEquals(250, state.draft.value.carplayScreenWidthCustomMm)
        state.editNumber("carplayScreenWidthCustomMm", "401", { it.toIntOrNull() in 100..400 }) {
            fail("Invalid width must not enter the draft")
        }
        assertEquals(setOf("settings/picture"), state.errorPages { false })
        state.useScreenWidthReference(225)
        assertTrue(state.invalidNumbers.isEmpty()); assertTrue(state.numberText.isEmpty())
        assertEquals(225, state.draft.value.carplayScreenWidthCustomMm)
        state.chooseScreenWidth(CarPlayScreenWidth.MM125)
        assertNull(state.draft.value.carplayScreenWidthCustomMm)
        assertEquals(CarPlayScreenWidth.MM125, state.draft.value.carplayScreenWidth)
    }

    @Test fun manualAreasRequireOneCompletePairKeepPartialTextAndAllowClearingEitherPair() {
        val state = editor()
        state.chooseViewAreas(mode = ViewAreasMode.MANUAL)
        assertEquals(setOf("settings/car"), state.errorPages { false })
        state.updateViewArea("viewAreasFullscreenWidth", "3840")
        assertNull(state.draft.value.viewAreasFullscreen)
        state.updateViewArea("viewAreasFullscreenHeight", "2160")
        assertEquals(PictureSize(3840, 2160), state.draft.value.viewAreasFullscreen)
        assertTrue(state.errorPages { false }.isEmpty())
        state.updateViewArea("viewAreasWindowedWidth", "320")
        state.updateViewArea("viewAreasWindowedHeight", "241")
        assertNull(state.draft.value.viewAreasWindowed)
        assertEquals(setOf("settings/car"), state.errorPages { false })
        state.updateViewArea("viewAreasWindowedHeight", "240")
        assertEquals(PictureSize(320, 240), state.draft.value.viewAreasWindowed)
        state.updateViewArea("viewAreasFullscreenWidth", "")
        state.updateViewArea("viewAreasFullscreenHeight", "")
        assertNull(state.draft.value.viewAreasFullscreen)
        assertTrue(state.errorPages { false }.isEmpty())
        state.updateViewArea("viewAreasWindowedWidth", "")
        state.updateViewArea("viewAreasWindowedHeight", "")
        assertEquals(setOf("settings/car"), state.errorPages { false })
        state.chooseViewAreas(enabled = false)
        assertTrue(state.errorPages { false }.isEmpty())
    }

    @Test fun useAreaReferenceReplacesBothPairsAndNormalizesRecordedPixelsToInputBounds() {
        val state = editor()
        state.chooseViewAreas(mode = ViewAreasMode.MANUAL)
        state.updateViewArea("viewAreasFullscreenWidth", "bad")
        state.useViewAreaReference(org.openxplay.direlay.relay.ViewAreaReference(
            org.openxplay.direlay.relay.ViewAreaSize(1533, 1015), org.openxplay.direlay.relay.ViewAreaSize(5000, 200)))
        assertEquals(PictureSize(1532, 1014), state.draft.value.viewAreasFullscreen)
        assertEquals(PictureSize(3840, 240), state.draft.value.viewAreasWindowed)
        assertTrue(state.invalidNumbers.isEmpty()); assertTrue(state.numberText.isEmpty())
        state.useViewAreaReference(org.openxplay.direlay.relay.ViewAreaReference(windowed = org.openxplay.direlay.relay.ViewAreaSize(800, 600)))
        assertNull(state.draft.value.viewAreasFullscreen)
        assertEquals(PictureSize(800, 600), state.draft.value.viewAreasWindowed)
        state.chooseViewAreas(mode = ViewAreasMode.AUTOMATIC)
        assertEquals(PictureSize(800, 600), state.draft.value.viewAreasWindowed)
    }

}
