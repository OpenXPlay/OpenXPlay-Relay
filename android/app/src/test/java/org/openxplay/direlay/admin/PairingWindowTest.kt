// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
package org.openxplay.direlay.admin

import org.junit.Assert.*
import org.junit.Test

class PairingWindowTest {
    private var now = 1_000L
    private val visibility = object : BluetoothVisibility {
        override var available = true
        var allowed = true
        var onScreen = true
        var failure: String? = null
        var visible: Boolean? = true
        var prompts = 0
        var withdrawn = 0
        override fun dialogAllowed() = allowed
        override fun prompt(): Boolean {
            failure?.let { throw PairingUnavailable(it) }
            prompts++
            return onScreen
        }
        override fun withdraw() { withdrawn++ }
        override fun discoverable() = visible
    }
    private val window = PairingWindow(visibility) { now }

    @Test fun allowedDialogWaitsForTheAnswerThenCountsDownWhatAndroidGranted() {
        assertEquals(PairingWindow.State(), window.state())
        assertNull(window.open().error)
        assertEquals(PairingWindow.State(needsConfirmation = true), window.state())
        assertEquals(PairingWindow.Prompt.DIALOG, window.take())
        assertNull("a prompt is taken once", window.take())
        now += 5
        window.dialogResult(300)
        assertEquals(PairingWindow.State(deadline = 1_305), window.state())
        assertNull(window.open().error)
        assertEquals("an open window is not requested twice", 1, visibility.prompts)
        now = 1_305
        assertEquals(PairingWindow.State(), window.state())
    }

    @Test fun android12WithoutNearbyDevicesAsksForItOnlyWhenTheWindowOpens() {
        visibility.allowed = false
        window.open()
        assertEquals(PairingWindow.State(needsPermission = true), window.state())
        assertEquals(PairingWindow.Prompt.PERMISSION, window.take())
        assertEquals(PairingWindow.State(needsPermission = true), window.state())
        assertEquals(PairingWindow.Prompt.DIALOG, window.permissionResult(true))
        assertEquals(PairingWindow.State(needsConfirmation = true), window.state())
        window.dialogResult(300)
        assertEquals(1_300L, window.state().deadline)
    }

    @Test fun declinedPermissionFallsBackToBluetoothSettingsAndSaysSo() {
        visibility.allowed = false
        window.open(); window.take()
        assertEquals(PairingWindow.Prompt.SETTINGS, window.permissionResult(false))
        assertEquals(PairingWindow.State(onRelay = true, permissionDenied = true), window.state())
        now += PairingWindow.SECONDS - 1
        assertEquals(PairingWindow.State(onRelay = true, permissionDenied = true), window.state())
        now += 1
        assertEquals(PairingWindow.State(), window.state())
    }

    @Test fun permissionAllowedMeanwhileGoesStraightToTheDialog() {
        visibility.allowed = false
        window.open()
        visibility.allowed = true
        assertEquals(PairingWindow.Prompt.DIALOG, window.take())
        assertEquals(PairingWindow.State(needsConfirmation = true), window.state())
    }

    @Test fun backgroundRequestWaitsInANotificationForTheWholeWindow() {
        visibility.allowed = false; visibility.onScreen = false
        window.open()
        assertEquals(PairingWindow.State(needsPermission = true, notified = true), window.state())
        now += PairingWindow.SECONDS - 1
        assertEquals(PairingWindow.State(needsPermission = true, notified = true), window.state())
        assertEquals(PairingWindow.Prompt.PERMISSION, window.take())
        assertEquals("taking the prompt removes its notification", 1, visibility.withdrawn)
        assertEquals(PairingWindow.State(needsPermission = true), window.state())
    }

    @Test fun ignoredNotificationExpiresWithTheWindowAndIsRemoved() {
        visibility.onScreen = false
        window.open()
        assertEquals(PairingWindow.State(needsConfirmation = true, notified = true), window.state())
        now += PairingWindow.SECONDS
        assertEquals(PairingWindow.State(), window.state())
        assertEquals(1, visibility.withdrawn)
        assertNull(window.take())
    }

    @Test fun unansweredPromptsOnTheRelayScreenCloseAfterTheGracePeriod() {
        window.open()
        now += PairingWindow.CONFIRM_SECONDS
        assertEquals("never taken", PairingWindow.State(), window.state())
        window.open(); window.take()
        now += PairingWindow.CONFIRM_SECONDS
        assertEquals("never answered", PairingWindow.State(), window.state())
        assertEquals("a late answer still counts", PairingWindow.Prompt.DIALOG, window.permissionResult(true))
        window.dialogResult(120)
        assertEquals(now + 120, window.state().deadline)
    }

    @Test fun declinedDialogClosesTheWindow() {
        window.open(); window.take()
        window.dialogResult(0)
        assertEquals(PairingWindow.State(), window.state())
        assertNull(window.open().error)
        assertEquals("a new window can open at once", 2, visibility.prompts)
    }

    @Test fun visibilityThatEndsEarlyClosesAfterTheGracePeriod() {
        window.open(); window.take(); window.dialogResult(300)
        visibility.visible = false
        assertEquals(1_300L, window.state().deadline)
        now += PairingWindow.CONFIRM_SECONDS
        assertEquals(PairingWindow.State(), window.state())
    }

    @Test fun unreadableScanModeTrustsTheReportedWindow() {
        visibility.visible = null
        window.open(); window.take(); window.dialogResult(300)
        now += 299
        assertEquals(1_300L, window.state().deadline)
    }

    @Test fun dialogThatCannotShowFallsBackToBluetoothSettings() {
        window.open(); window.take()
        assertEquals(PairingWindow.Prompt.SETTINGS, window.dialogFailed(permission = true))
        assertEquals(PairingWindow.State(onRelay = true, permissionDenied = true), window.state())
        window.settingsFailed()
        assertEquals(PairingWindow.State(), window.state())
        window.open(); window.take()
        window.dialogFailed(permission = false)
        assertEquals(PairingWindow.State(onRelay = true), window.state())
    }

    @Test fun failuresReturnTheErrorKey() {
        visibility.failure = "error.pairingOpenApp"
        assertEquals("error.pairingOpenApp", window.open().error)
        assertEquals(PairingWindow.State(), window.state())
        assertNull(window.take())
        visibility.failure = null; visibility.available = false
        assertEquals("error.pairing", window.open().error)
        assertFalse(window.available)
    }
}
