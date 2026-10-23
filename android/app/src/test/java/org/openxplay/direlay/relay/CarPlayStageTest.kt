// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23
package org.openxplay.direlay.relay

import com.shilapi.xcertplay.orchestration.CarPlayStatus
import org.junit.Assert.*
import org.junit.Test
import org.openxplay.stream.core.SessionConfig
import org.openxplay.stream.core.SessionId
import org.openxplay.stream.core.SessionPhase
import org.openxplay.stream.core.SessionState
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class CarPlayStageTest {
    @Test fun wirelessSequenceUsesTheLatestInputIncludingANewAttemptAfterInterruption() {
        val tracker = CarPlayStageTracker()
        assertEquals(CarPlayStage.NONE, tracker.stage)
        val preparation = listOf(CarPlayStatus.DiscoveringMfi, CarPlayStatus.WaitingForMfi,
            CarPlayStatus.RequestingMfiPermission, CarPlayStatus.MfiReady, CarPlayStatus.StartingHotspot,
            CarPlayStatus.HotspotReady("network", "5GHz", 36, "", "", "p2p"))
        preparation.forEach { status ->
            tracker.onSessionEnded()
            tracker.onStatus(status)
            assertEquals(status.toString(), CarPlayStage.NONE, tracker.stage)
        }
        tracker.onStatus(CarPlayStatus.WaitingForPairedIphone)
        assertEquals(CarPlayStage.WAITING, tracker.stage)
        tracker.onStatus(CarPlayStatus.ConnectingBluetooth)
        assertEquals(CarPlayStage.BLUETOOTH, tracker.stage)
        tracker.onStatus(CarPlayStatus.RunningWireless)
        assertEquals(CarPlayStage.BLUETOOTH_CONNECTED, tracker.stage)
        tracker.onSessionActive()
        assertEquals(CarPlayStage.SESSION, tracker.stage)
        tracker.onStatus(CarPlayStatus.WirelessActive)
        assertEquals(CarPlayStage.BLUETOOTH_CONNECTED, tracker.stage)
        tracker.onStatus(CarPlayStatus.WirelessActiveFallback)
        assertEquals(CarPlayStage.BLUETOOTH_CONNECTED, tracker.stage)
        tracker.onSessionEnded()
        assertEquals(CarPlayStage.INTERRUPTED, tracker.stage)
        tracker.onStatus(CarPlayStatus.WaitingForPairedIphone)
        assertEquals(CarPlayStage.WAITING, tracker.stage)
        tracker.onSessionEnded()
        tracker.onStatus(CarPlayStatus.ConnectingBluetooth)
        assertEquals(CarPlayStage.BLUETOOTH, tracker.stage)
    }

    @Test fun wiredSequenceTracksUsbSetupSessionAndControlEnd() {
        val tracker = CarPlayStageTracker()
        for (status in listOf(CarPlayStatus.DiscoveringIphone, CarPlayStatus.WaitingForIphone)) {
            tracker.onSessionEnded()
            tracker.onStatus(status)
            assertEquals(CarPlayStage.WAITING, tracker.stage)
        }
        for (status in listOf(CarPlayStatus.RequestingIphonePermission, CarPlayStatus.WaitingForReenumeration,
            CarPlayStatus.SelectingConfiguration, CarPlayStatus.OpeningDataPaths, CarPlayStatus.Pairing,
            CarPlayStatus.ConnectingControl, CarPlayStatus.AttachingNetwork, CarPlayStatus.RunningControl)) {
            tracker.onSessionEnded()
            tracker.onStatus(status)
            assertEquals(status.toString(), CarPlayStage.USB_CONNECTED, tracker.stage)
        }
        tracker.onSessionActive()
        assertEquals(CarPlayStage.SESSION, tracker.stage)
        tracker.onStatus(CarPlayStatus.ControlEnded)
        assertEquals(CarPlayStage.INTERRUPTED, tracker.stage)
        tracker.onStatus(CarPlayStatus.RequestingIphonePermission)
        assertEquals(CarPlayStage.USB_CONNECTED, tracker.stage)
    }

    @Test fun onlyStreamEndChangesTheStageAndResetClearsIt() {
        val tracker = CarPlayStageTracker()
        tracker.onSessionActive()
        for (phase in listOf(SessionPhase.CONNECTING, SessionPhase.ACTIVE, SessionPhase.RECONNECTING)) {
            tracker.onStreamPhase(phase)
            assertEquals(CarPlayStage.SESSION, tracker.stage)
        }
        tracker.onStreamPhase(SessionPhase.ENDED)
        assertEquals(CarPlayStage.INTERRUPTED, tracker.stage)
        tracker.reset()
        assertEquals(CarPlayStage.NONE, tracker.stage)
        tracker.onStatus(CarPlayStatus.ConnectingBluetooth)
        tracker.reset()
        assertEquals(CarPlayStage.NONE, tracker.stage)
    }

    @Test fun relayStatusFeedsStreamEndOncePerPhaseTransition() {
        val tracker = CarPlayStageTracker()
        val status = RelayStatus(carPlayStage = tracker)
        fun session(phase: SessionPhase, id: Long = 1) =
            status.onSession(SessionState(SessionId(id), phase), SessionConfig(emptyMap()))
        tracker.onSessionActive()
        session(SessionPhase.CONNECTING)
        session(SessionPhase.ACTIVE)
        assertEquals(CarPlayStage.SESSION, tracker.stage)
        session(SessionPhase.ENDED)
        assertEquals(CarPlayStage.INTERRUPTED, tracker.stage)
        tracker.onStatus(CarPlayStatus.WaitingForPairedIphone)
        session(SessionPhase.ENDED)
        assertEquals(CarPlayStage.WAITING, tracker.stage)
        session(SessionPhase.ENDED, id = 2)
        assertEquals(CarPlayStage.INTERRUPTED, tracker.stage)
        tracker.reset()
        session(SessionPhase.ENDED, id = 2)
        assertEquals(CarPlayStage.NONE, tracker.stage)
    }

    @Test fun failureLeavesEveryStageUnchanged() {
        val tracker = CarPlayStageTracker()
        val inputs: List<() -> Unit> = listOf({ tracker.reset() },
            { tracker.onStatus(CarPlayStatus.WaitingForPairedIphone) },
            { tracker.onStatus(CarPlayStatus.ConnectingBluetooth) },
            { tracker.onStatus(CarPlayStatus.RunningWireless) },
            { tracker.onStatus(CarPlayStatus.Pairing) },
            { tracker.onSessionActive() }, { tracker.onSessionEnded() })
        inputs.forEach { input ->
            input()
            val before = tracker.stage
            tracker.onStatus(CarPlayStatus.Failed("failed"))
            assertEquals(before, tracker.stage)
        }
    }

    @Test fun callbacksAndReadsFromDifferentThreadsFinishWithoutBlocking() {
        val tracker = CarPlayStageTracker()
        val workers = Executors.newFixedThreadPool(4)
        try {
            val tasks = (0..3).map { worker -> workers.submit {
                repeat(200) {
                    when (worker) {
                        0 -> tracker.onStatus(CarPlayStatus.ConnectingBluetooth)
                        1 -> { tracker.onSessionActive(); tracker.onSessionEnded() }
                        2 -> { tracker.onStreamPhase(SessionPhase.ENDED); tracker.reset() }
                        else -> {
                            tracker.onStatus(CarPlayStatus.Failed("failed"))
                            assertTrue(tracker.stage in CarPlayStage.entries)
                        }
                    }
                }
            } }
            tasks.forEach { it.get(10, TimeUnit.SECONDS) }
            tracker.onStatus(CarPlayStatus.WaitingForPairedIphone)
            assertEquals(CarPlayStage.WAITING, tracker.stage)
        } finally { workers.shutdownNow() }
    }
}
