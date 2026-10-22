// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21
package org.openxplay.direlay.ui

import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.snapshots.Snapshot
import com.shilapi.xcertplay.orchestration.MfiTarget
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.direlay.settings.*

/** Drive the same collector launched by rememberSettingsEditorState, without a second autosave controller. */
class SettingsEditorCollectorTest {
    private val initial = RelaySettings(ConnectionMode.WIRELESS, MfiTarget.LOCAL, "", "", "", null,
        30, false, true, false, null, wirelessNetwork = WirelessNetworkSettings(
            choice = WirelessNetworkChoice.PHONE_HOTSPOT, manualSsid = "Car", manualPassphrase = "12345678"))
    private fun editor() = SettingsEditorState(mutableStateOf(initial), mutableStateOf(initial),
        mutableStateOf(false), mutableStateMapOf(), mutableStateMapOf())
    private suspend fun flushSnapshots() {
        Snapshot.sendApplyNotifications()
        repeat(3) { yield() }
    }
    private suspend fun awaitPublications(values: List<RelaySettings>, count: Int) = withTimeout(2_000) {
        while (values.size < count) flushSnapshots()
    }

    @Test fun invalidDeveloperNumberTextDoesNotBlockStartupEditsAcrossPages() = runBlocking {
        val state = editor()
        var stack = RelayBackStack().open("developer/connection")
        state.synchronize(initial, settingsSessionActive(stack.entries))
        val published = mutableListOf<RelaySettings>()
        val collector = launch(start = CoroutineStart.UNDISPATCHED) {
            state.collectChanges({ true }) { _, after -> published.add(after); true }
        }
        try {
            state.editNumber("port", "80", { it.isEmpty() || it.toIntOrNull() in 1024..65535 }) {
                state.draft.value = state.draft.value.copy(webPort = it.toIntOrNull())
            }
            state.customDisplay = true
            state.updateDisplay("width", "")
            state.updateDisplay("width", "1")
            assertNull(state.draft.value.display)
            state.updateDisplay("width", "")
            assertNull(state.draft.value.webPort)
            stack = stack.open("settings/startup")
            state.synchronize(initial, settingsSessionActive(stack.entries))
            state.draft.value = state.draft.value.copy(autoStartOnBoot = true)
            awaitPublications(published, 1)
            assertEquals(listOf(initial.copy(autoStartOnBoot = true)), published)
            assertEquals("80", state.numberText["port"])
            assertEquals("", state.numberText["width"])
            assertEquals(setOf("developer/connection", "developer/video"), state.errorPages { true })
            state.synchronize(published.single(), true)
            flushSnapshots()
            assertEquals(1, published.size)
            assertEquals(published.single(), state.lastSaved.value)
        } finally { collector.cancelAndJoin() }
    }

    @Test fun leavingAllMenusDiscardsDirtyDraftAndKeepsNewerExternalSettings() = runBlocking {
        val state = editor()
        var stack = RelayBackStack().open("settings:network")
        state.synchronize(initial, settingsSessionActive(stack.entries))
        val published = mutableListOf<RelaySettings>()
        val collector = launch(start = CoroutineStart.UNDISPATCHED) {
            state.collectChanges({ true }) { _, after -> published.add(after); true }
        }
        try {
            state.draft.value = state.draft.value.copy(wirelessNetwork = initial.wirelessNetwork.copy(manualPassphrase = "short"))
            state.customDisplay = true
            state.updateDisplay("width", "")
            flushSnapshots()
            assertTrue(published.isEmpty())
            while (stack.canBack) stack = stack.back()
            state.synchronize(initial, settingsSessionActive(stack.entries))
            assertEquals(initial, state.draft.value)
            assertEquals(initial, state.lastSaved.value)
            assertTrue(state.numberText.isEmpty())
            assertTrue(state.invalidNumbers.isEmpty())
            assertFalse(state.customDisplay)
            // Setup, admin and Vehicle can publish while there are no settings entries.
            val external = initial.copy(autoStartOnBoot = true, webPort = 9443, vehicleInformation = true)
            state.synchronize(external, false)
            flushSnapshots()
            assertTrue(published.isEmpty())
            stack = stack.open("settings:network")
            state.synchronize(external, settingsSessionActive(stack.entries))
            assertEquals(external, state.draft.value)
            state.draft.value = state.draft.value.copy(wirelessNetwork = external.wirelessNetwork.copy(manualPassphrase = "abcdefgh"))
            awaitPublications(published, 1)
            assertEquals(listOf(external.copy(wirelessNetwork = external.wirelessNetwork.copy(manualPassphrase = "abcdefgh"))), published)
            assertEquals(published.single(), state.lastSaved.value)
        } finally { collector.cancelAndJoin() }
    }
}
