// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27
package org.openxplay.direlay.ui.design

import android.app.Application
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.openxplay.direlay.ui.RelayAppSheet
import org.openxplay.direlay.ui.relayAppSheet
import org.openxplay.direlay.ui.rendererSelection
import org.openxplay.direlay.ui.stepNavigationDestination
import org.openxplay.direlay.path.StepAction
import org.openxplay.direlay.settings.DlnaRendererChoice
import org.openxplay.direlay.settings.RelaySettings
import org.openxplay.direlay.settings.ConnectionMode
import com.shilapi.xcertplay.orchestration.MfiTarget
import org.openxplay.stream.core.FrameRatePreference
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], application = Application::class)
class SurfaceHostTest {
    @Test fun requestsAreSingleInstanceAndOneDecisionIsVisibleAtATime() {
        val first = RelaySurfaceQueue().open("decision", 0)
        assertEquals(first, first.open("decision", 0))
        val pending = first.open("details", 0)
        assertEquals("decision", pending.visible)
        assertEquals("details", pending.close("decision").visible)
        val pairing = pending.open("pairing", 1)
        assertEquals("pairing", pairing.visible)
        assertEquals(pending, pairing.close("pairing"))
        assertNull(pairing.close("pairing").close("decision").close("details").visible)
    }

    @Test fun thePinPreemptsThePickerAndThePickerResumesAfterPairing() {
        assertNull(relayAppSheet(false, false))
        assertEquals(RelayAppSheet.RENDERER, relayAppSheet(true, false))
        assertEquals(RelayAppSheet.CARLINK_PIN, relayAppSheet(false, true))
        assertEquals(RelayAppSheet.CARLINK_PIN, relayAppSheet(true, true))
        assertEquals(RelayAppSheet.RENDERER, relayAppSheet(true, false))
    }

    @Test fun rendererSelectionUsesTheCurrentSettingsOutsideAMenuAndTheDraftInsideIt() {
        val initial = RelaySettings(ConnectionMode.USB, MfiTarget.LOCAL, "", "", "", null, FrameRatePreference.AUTO, false, false, false, null)
        val draft = initial.copy(vehicleInformation = !initial.vehicleInformation)
        val choice = DlnaRendererChoice("renderer", "Car")
        assertEquals(initial.copy(dlnaRenderer = choice), rendererSelection(initial, draft, false, choice))
        assertEquals(draft.copy(dlnaRenderer = choice), rendererSelection(initial, draft, true, choice))
    }

    @Test fun choosingARendererDoesNotOpenASettingsPageUnderTheSheet() {
        assertNull(stepNavigationDestination(StepAction.CHOOSE_RENDERER, "settings/identity"))
        assertEquals("settings/identity", stepNavigationDestination(StepAction.FIX_SETTINGS, "settings/identity"))
        assertEquals("settings:iphone", stepNavigationDestination(StepAction.CHOOSE_IPHONE, "settings"))
        assertEquals("settings:sharedWifi", stepNavigationDestination(StepAction.CHOOSE_SHARED_WIFI, "settings"))
        assertNull(stepNavigationDestination(null, "settings"))
    }

    private class EmptyApplier : AbstractApplier<Unit>(Unit) {
        override fun insertTopDown(index: Int, instance: Unit) = Unit
        override fun insertBottomUp(index: Int, instance: Unit) = Unit
        override fun remove(index: Int, count: Int) = Unit
        override fun move(from: Int, to: Int, count: Int) = Unit
        override fun onClear() = Unit
    }

    @Test fun outgoingPagesAndNativePromptsCannotLeaveASecondSurfaceVisible() = runBlocking {
        withTimeout(10000) {
            val clock = BroadcastFrameClock()
            val recomposer = Recomposer(coroutineContext + clock)
            val runner = launch(clock) { recomposer.runRecomposeAndApplyChanges() }
            val composition = Composition(EmptyApplier(), recomposer)
            val host = mutableStateOf(RelaySurfaceQueue())
            val active = mutableStateOf(true)
            val sheet = mutableStateOf(false)
            var oldPageVisible = false
            var appSheetVisible = false
            suspend fun settle() {
                repeat(12) {
                    Snapshot.sendApplyNotifications()
                    yield()
                    clock.sendFrame(it * 100_000_000L)
                    yield()
                }
            }
            try {
                composition.setContent {
                    CompositionLocalProvider(LocalRelaySurfaceHost provides host) {
                        CompositionLocalProvider(LocalRelaySurfaceActive provides (active.value && !sheet.value)) {
                            RelaySurface {
                                DisposableEffect(Unit) { oldPageVisible = true; onDispose { oldPageVisible = false } }
                            }
                        }
                        if (sheet.value) RelaySurface {
                            DisposableEffect(Unit) { appSheetVisible = true; onDispose { appSheetVisible = false } }
                        }
                    }
                }
                settle()
                assertTrue(oldPageVisible)
                sheet.value = true
                settle()
                assertFalse(oldPageVisible)
                assertTrue(appSheetVisible)
                assertEquals(1, host.value.requests.size)
                sheet.value = false
                active.value = false
                settle()
                assertFalse(oldPageVisible)
                assertFalse(appSheetVisible)
                assertNull(host.value.visible)
                active.value = true
                settle()
                assertTrue(oldPageVisible)
                assertEquals(1, host.value.requests.size)
            } finally { composition.dispose(); recomposer.cancel(); runner.cancelAndJoin() }
        }
    }
}
